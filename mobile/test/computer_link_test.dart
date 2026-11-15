// The link's reactions to what the computer and the relay send, with a scripted relay and a desktop played by the test
// (its packets are sealed by the real Rust core). Everything here is made up.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/config/flavor.dart';
import 'package:phone/data/models.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/link/relay_session.dart';
import 'package:phone/src/rust/api/core.dart';
import 'package:phone/src/rust/api/messages.dart';

import 'support/core_library.dart';
import 'support/memory_secure_keys.dart';
import 'support/scripted_socket.dart';

Future<T> untilValue<T>(T? Function() f) async {
  final end = DateTime.now().add(const Duration(seconds: 5));
  while (DateTime.now().isBefore(end)) {
    final v = f();
    if (v != null) return v;
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  throw TimeoutException('timed out');
}

Future<void> until(bool Function() ok) async =>
    untilValue<bool>(() => ok() ? true : null);

void main() {
  late Directory tmp;
  late MemorySecureKeys keys;
  late LinkStateStore store;
  late DeviceIdentity phone, desktop;
  late ProtocolVersions versions;
  late PairedComputer computer;
  late ComputerLink link;
  final sockets = <ScriptedSocket>[];
  final urls = <String>[];
  var counter = DateTime.now().millisecondsSinceEpoch;

  setUpAll(initCoreLibrary);

  Future<void> build({Flavor flavor = Flavor.dev}) async {
    tmp = Directory.systemTemp.createTempSync('bobix-link-unit-');
    keys = MemorySecureKeys();
    store = LinkStateStore(tmp);
    phone = await generateIdentity();
    desktop = await generateIdentity();
    versions = await protocolVersions();
    await keys.putSecret(SecureKeys.auth, phone.authSeed);
    await keys.putSecret(SecureKeys.box, phone.boxSecret);
    computer = PairedComputer(
      id: desktop.deviceId,
      name: 'Studio Mac (demo)',
      relay: 'ws://127.0.0.1:1',
      phoneName: 'Test phone',
      desktopBoxPublic: desktop.boxPublic,
      rights: const PhoneRights(
        answer: true,
        plan: false,
        question: false,
        start: false,
        stop: false,
        activity: false,
      ),
      recvState: await recvStateNew(),
      eventReservation: BigInt.zero,
    );
    await store.save(computer);
    sockets.clear();
    urls.clear();
    link = ComputerLink(
      computer: computer,
      keys: keys,
      store: store,
      connect: (url) async {
        urls.add(url);
        final s = ScriptedSocket()
          ..push({'type': 'challenge', 'nonce': 'n${urls.length}'});
        sockets.add(s);
        return s;
      },
      flavor: flavor,
      identity: phone,
      versions: versions,
      delay: (_, _) => const Duration(milliseconds: 20),
    );
    link.resume();
    await until(() => sockets.isNotEmpty && sockets.first.sent.length >= 3);
  }

  tearDown(() async {
    await link.dispose();
    tmp.deleteSync(recursive: true);
  });

  /// A packet from the computer, sealed with its keys, as a relay `msg` frame.
  Future<Map<String, Object?>> fromComputer(
    Map<String, Object?> message, {
    int? version,
    BigInt? at,
    LinkDirection direction = LinkDirection.toPhone,
    LinkStream stream = LinkStream.event,
  }) async {
    final packet = await sealPacket(
      version: version ?? versions.current,
      direction: direction,
      stream: stream,
      counter: at ?? BigInt.from(++counter),
      senderBoxSecret: desktop.boxSecret,
      recipientBoxPublic: phone.boxPublic,
      plaintext: Uint8List.fromList(utf8.encode(jsonEncode(message))),
    );
    return {
      'type': 'msg',
      'pid': 'p$counter',
      'from': desktop.deviceId,
      'body': b64(packet),
    };
  }

  test(
    'on connect: peers, then a sealed sync; every packet is acknowledged',
    () async {
      await build();
      final s = sockets.first;
      expect(s.ofType('auth'), hasLength(1));
      expect(s.ofType('auth').single['relay'], 'ws://127.0.0.1:1');
      expect(s.ofType('peers').single['allow'], [desktop.deviceId]);
      expect(s.ofType('send').single['to'], desktop.deviceId);
      s.push(await fromComputer({'kind': 'error', 'code': 'x', 'who': null}));
      await until(() => s.ofType('ack').isNotEmpty);
      expect(keys.signCalls, 0);
    },
  );

  test('a replayed packet and one that travels the wrong way are dropped, not answered', () async {
    await build();
    final s = sockets.first;
    final seen = <String>[];
    link.messages.listen((m) => seen.add(m.kind));
    final notice = {
      'kind': 'notice',
      'what': 'done',
      'ref': 'r',
      'session': 's',
      'title': 't',
      'body': 'b',
    };
    final once = await fromComputer(notice);
    s.push(once);
    s.push(once); // the same counter again
    s.push(
      await fromComputer(notice, direction: LinkDirection.toDesktop),
    ); // reflected
    s.push(await fromComputer(notice, at: BigInt.one)); // far too old
    await until(() => s.ofType('ack').length == 4);
    await Future<void>.delayed(const Duration(milliseconds: 100));
    expect(seen, ['notice']);
    expect(link.status.maybeRevoked, isFalse);
    expect(link.status.revokedByComputer, isFalse);
    expect(await store.load(), isNotNull);
  });

  test('a sealed revoked erases the keys and the state at once', () async {
    await build();
    sockets.first.push(
      await fromComputer({'kind': 'revoked', 'why': 'removed on the computer'}),
    );
    await until(() => link.status.revokedByComputer);
    expect(keys.erased, isTrue);
    expect(keys.secrets, isEmpty);
    expect(await store.load(), isNull);
  });

  test('relayMoved: acknowledged first, then the new relay is used; an insecure one is ignored in prod', () async {
    await build();
    final s = sockets.first;
    s.push(
      await fromComputer({
        'kind': 'relayMoved',
        'url': 'wss://new-relay.example/ws',
      }),
    );
    await until(() => urls.length == 2);
    expect(urls.last, 'wss://new-relay.example/ws');
    expect(
      s.ofType('ack'),
      hasLength(1),
      reason: 'acknowledged on the old relay',
    );
    expect((await store.load())!.relay, 'wss://new-relay.example/ws');
    expect(
      link.status.movedHost,
      'new-relay.example',
      reason: 'the move is shown, not silent',
    );
    await link.dispose();
    tmp.deleteSync(recursive: true);

    await build(flavor: Flavor.prod);
    sockets.first.push(
      await fromComputer({
        'kind': 'relayMoved',
        'url': 'ws://elsewhere.example',
      }),
    );
    await until(() => sockets.first.ofType('ack').isNotEmpty);
    await Future<void>.delayed(const Duration(milliseconds: 100));
    expect(urls, hasLength(1), reason: 'prod never follows a plain address');
    expect((await store.load())!.relay, 'ws://127.0.0.1:1');
  });

  test('presence and the relay errors become what the person sees', () async {
    await build();
    final s = sockets.first;
    s.push({'type': 'presence', 'peer': desktop.deviceId, 'state': 'asleep'});
    await until(() => link.status.conn == ConnState.asleep);
    s.push({'type': 'presence', 'peer': desktop.deviceId, 'state': 'offline'});
    await until(() => link.status.conn == ConnState.offline);
    s.push({'type': 'presence', 'peer': desktop.deviceId, 'state': 'online'});
    await until(() => link.status.conn == ConnState.ok);
    s.push({'type': 'error', 'code': 'update', 'who': 'desktop'});
    await until(() => link.status.problem == LinkProblem.updateComputer);
    s.push({'type': 'error', 'code': 'invite'});
    await until(() => link.status.problem == LinkProblem.invite);
  });

  test(
    'a packet of a version this app does not know says the phone must update',
    () async {
      await build();
      sockets.first.push(
        await fromComputer({
          'kind': 'error',
          'code': 'x',
          'who': null,
        }, version: 99),
      );
      await until(() => link.status.problem == LinkProblem.updatePhone);
    },
  );

  test('the relay saying revoked ends the connection attempts and asks to pair again', () async {
    await build();
    sockets.first.push({'type': 'error', 'code': 'revoked', 'who': 'desktop'});
    await until(() => link.status.maybeRevoked);
    await sockets.first.close();
    await Future<void>.delayed(const Duration(milliseconds: 150));
    expect(
      urls,
      hasLength(1),
      reason: 'no new attempt: the relay would cut it again',
    );
    expect(
      await store.load(),
      isNotNull,
      reason: 'nothing is erased until the person says so',
    );
  });

  test(
    'an unreachable relay is no network or relay down; it retries',
    () async {
      tmp = Directory.systemTemp.createTempSync('bobix-link-unit-');
      expect(
        whyUnreachable(const SocketException('Failed host lookup: x')),
        Unreachable.noNetwork,
      );
      expect(
        whyUnreachable(const SocketException('Connection refused')),
        Unreachable.relayDown,
      );
      expect(whyUnreachable(TimeoutException('x')), Unreachable.relayDown);
      // `build` expects a running link; this test needs none.
      keys = MemorySecureKeys();
      store = LinkStateStore(tmp);
      phone = await generateIdentity();
      desktop = await generateIdentity();
      link = ComputerLink(
        computer: PairedComputer(
          id: desktop.deviceId,
          name: 'x',
          relay: 'ws://127.0.0.1:1',
          phoneName: 'p',
          desktopBoxPublic: desktop.boxPublic,
          rights: const PhoneRights(
            answer: false,
            plan: false,
            question: false,
            start: false,
            stop: false,
            activity: false,
          ),
          recvState: await recvStateNew(),
          eventReservation: BigInt.zero,
        ),
        keys: keys,
        store: store,
        connect: (_) async =>
            throw const SocketException('Failed host lookup: test'),
        flavor: Flavor.dev,
        identity: phone,
        versions: await protocolVersions(),
        delay: (_, _) => const Duration(milliseconds: 10),
      );
      link.resume();
      await until(() => link.status.conn == ConnState.noNetwork);
      await link.pause();
    },
  );

  test(
    'a goodbye is a bye signed with the auth key, which the computer verifies',
    () async {
      await build();
      final s = sockets.first;
      final before = s.ofType('send').length;
      await link.sayGoodbye(within: const Duration(milliseconds: 200));
      final frame = s.ofType('send').skip(before).single;
      expect(frame['to'], desktop.deviceId);
      final packet = b64d(frame['body'] as String);
      final plain = await openPacket(
        receiverBoxSecret: desktop.boxSecret,
        senderBoxPublic: phone.boxPublic,
        packet: packet,
      );
      final message = jsonDecode(utf8.decode(plain)) as Map<String, dynamic>;
      expect(message['kind'], 'bye');
      final header = await peekPacket(
        receiverSends: LinkDirection.toPhone,
        packet: packet,
      );
      final bytes = await byeBytes(
        version: versions.current,
        phoneId: phone.deviceId,
        computerId: desktop.deviceId,
        counter: header.counter,
      );
      expect(
        await verifyAuthSignature(
          authPublic: phone.authPublic,
          message: bytes,
          signature: b64d(message['sig'] as String),
        ),
        isTrue,
      );
    },
  );

  test('a phone that is not connected just leaves: no goodbye is sent, and nothing throws', () async {
    await build();
    await sockets.first.close();
    await until(() => link.status.conn != ConnState.ok || sockets.length > 1);
    await link.pause();
    await link.sayGoodbye(within: const Duration(milliseconds: 100));
  });
}
