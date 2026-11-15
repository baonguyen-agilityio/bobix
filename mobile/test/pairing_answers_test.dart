// How a pairing ends, from what the computer says: a scripted relay, and a desktop played by the test whose packets are
// sealed by the real Rust core. Everything here is made up.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/config/flavor.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/link/pairing_service.dart';
import 'package:phone/src/rust/api/core.dart';
import 'package:phone/src/rust/api/messages.dart';

import 'support/core_library.dart';
import 'support/memory_secure_keys.dart';
import 'support/scripted_socket.dart';

const all = PhoneRights(
  answer: true,
  plan: true,
  question: true,
  start: true,
  stop: true,
  activity: true,
);

void main() {
  late Directory tmp;
  late MemorySecureKeys keys;
  late LinkStateStore store;
  late DeviceIdentity desktop;
  late ProtocolVersions versions;
  late ScriptedSocket socket;
  setUpAll(initCoreLibrary);
  setUp(() async {
    tmp = Directory.systemTemp.createTempSync('bobix-answers-');
    keys = MemorySecureKeys();
    store = LinkStateStore(tmp);
    desktop = await generateIdentity();
    versions = await protocolVersions();
    socket = ScriptedSocket()..push({'type': 'challenge', 'nonce': 'n'});
  });
  tearDown(() => tmp.deleteSync(recursive: true));

  String code() =>
      'bobix://pair?r=${Uri.encodeComponent('ws://127.0.0.1:1')}&a=${b64(desktop.authPublic)}&b=${b64(desktop.boxPublic)}&t=${b64(Uint8List(32)..fillRange(0, 32, 5))}';

  /// Starts a pairing, lets the phone's `hello` go out, then has the computer say [message] sealed to the phone.
  Future<Object?> pairingThat(Map<String, Object?> message) async {
    final service = PairingService(
      keys: keys,
      store: store,
      connect: (_) async => socket,
      flavor: Flavor.dev,
      waitForComputer: const Duration(seconds: 5),
    );
    final attempt = service.start(
      PairRequest(uri: code(), phoneName: 'p', rights: all),
    );
    final result = attempt.result.then<Object?>(
      (c) => c,
      onError: (Object e) => e,
    );
    Map<String, dynamic> pairSend() => socket.sent.firstWhere(
      (f) => f['type'] == 'pairSend',
      orElse: () => {},
    );
    final end = DateTime.now().add(const Duration(seconds: 5));
    while (pairSend().isEmpty && DateTime.now().isBefore(end)) {
      await Future<void>.delayed(const Duration(milliseconds: 20));
    }
    // The body is the phone's box public key, then the hello sealed to the computer.
    final phoneBox = b64d(pairSend()['body'] as String).sublist(0, 32);
    final packet = await sealPacket(
      version: versions.current,
      direction: LinkDirection.toPhone,
      stream: LinkStream.event,
      counter: BigInt.from(DateTime.now().millisecondsSinceEpoch),
      senderBoxSecret: desktop.boxSecret,
      recipientBoxPublic: phoneBox,
      plaintext: Uint8List.fromList(utf8.encode(jsonEncode(message))),
    );
    socket.push({
      'type': 'msg',
      'pid': 'p1',
      'from': desktop.deviceId,
      'body': b64(packet),
    });
    if (message['kind'] == 'paired') {
      // The phone acknowledges; the relay says it passed that on.
      final send = await _until(
        () => socket.sent.where((f) => f['type'] == 'send').firstOrNull,
      );
      socket.push({'type': 'delivered', 'pid': send['pid']});
    }
    return result;
  }

  PairFailure? failureOf(Object? r) => r is PairException ? r.failure : null;

  test(
    'a sealed revoked that says the pairing was cancelled: two phones answered',
    () async {
      expect(
        failureOf(
          await pairingThat({'kind': 'revoked', 'why': 'pairing cancelled'}),
        ),
        PairFailure.twoPhones,
      );
    },
  );

  test('a sealed error with the cancelled code: two phones answered', () async {
    expect(
      failureOf(
        await pairingThat({
          'kind': 'error',
          'code': 'pairingCancelled',
          'who': 'desktop',
        }),
      ),
      PairFailure.twoPhones,
    );
  });

  test('the owner said no: from a revoked, or from the refused code', () async {
    expect(
      failureOf(await pairingThat({'kind': 'revoked', 'why': 'declined'})),
      PairFailure.computerSaidNo,
    );
  });

  test('the refused code', () async {
    expect(
      failureOf(
        await pairingThat({
          'kind': 'error',
          'code': 'pairingRefused',
          'who': null,
        }),
      ),
      PairFailure.computerSaidNo,
    );
  });

  test('a computer that says nothing: the code expires after the wait, and nothing is kept', () async {
    final service = PairingService(
      keys: keys,
      store: store,
      connect: (_) async => socket,
      flavor: Flavor.dev,
      waitForComputer: const Duration(milliseconds: 300),
    );
    final r = await service
        .start(PairRequest(uri: code(), phoneName: 'p', rights: all))
        .result
        .then<Object?>((c) => c, onError: (Object e) => e);
    expect(failureOf(r), PairFailure.codeExpired);
    expect(await store.load(), isNull);
    expect(keys.secrets, isEmpty);
  });

  test(
    'paired with fewer rights than asked: exactly the granted ones are kept',
    () async {
      final granted = {
        'answer': true,
        'plan': false,
        'question': false,
        'start': false,
        'stop': false,
        'activity': true,
      };
      final r = await pairingThat({
        'kind': 'paired',
        'computer': 'Studio Mac (demo)',
        'rights': granted,
      });
      final c = r as PairedComputer;
      expect(c.name, 'Studio Mac (demo)');
      expect(rightsToJson(c.rights), granted);
      expect((await store.load())!.id, desktop.deviceId);
      expect(keys.secrets.keys, containsAll(['auth', 'box']));
    },
  );
}

Future<Map<String, dynamic>> _until(Map<String, dynamic>? Function() f) async {
  final end = DateTime.now().add(const Duration(seconds: 5));
  while (DateTime.now().isBefore(end)) {
    final v = f();
    if (v != null) return v;
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
  throw TimeoutException('timed out');
}
