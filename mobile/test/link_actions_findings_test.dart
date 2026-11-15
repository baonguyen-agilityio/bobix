// What the action source does with what the computer sends, with a scripted relay and a desktop played by the test (its
// packets are sealed by the real Rust core): several questions, hidden characters, a cut plan, why a request was handed
// back, and Stop from the computer's own flag. Everything here is made up.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/config/flavor.dart';
import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/actions_source.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/link_actions_source.dart';
import 'package:phone/link/link_state.dart';
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
  late DeviceIdentity phone, desktop;
  late ProtocolVersions versions;
  late ScriptedSocket socket;
  late ComputerLink link;
  late LinkActionsSource source;
  var counter = DateTime.now().millisecondsSinceEpoch;

  setUpAll(initCoreLibrary);

  setUp(() async {
    tmp = Directory.systemTemp.createTempSync('bobix-findings-');
    keys = MemorySecureKeys();
    store = LinkStateStore(tmp);
    phone = await generateIdentity();
    desktop = await generateIdentity();
    versions = await protocolVersions();
    await keys.putSecret(SecureKeys.auth, phone.authSeed);
    await keys.putSecret(SecureKeys.box, phone.boxSecret);
    final computer = PairedComputer(
      id: desktop.deviceId,
      name: 'Studio Mac (demo)',
      relay: 'ws://127.0.0.1:1',
      phoneName: 'p',
      desktopBoxPublic: desktop.boxPublic,
      rights: all,
      recvState: await recvStateNew(),
      eventReservation: BigInt.zero,
    );
    await store.save(computer);
    socket = ScriptedSocket()..push({'type': 'challenge', 'nonce': 'n'});
    link = ComputerLink(
      computer: computer,
      keys: keys,
      store: store,
      connect: (_) async => socket,
      flavor: Flavor.dev,
      identity: phone,
      versions: versions,
      delay: (_, _) => const Duration(milliseconds: 20),
    );
    link.resume();
    await _until(() => socket.sent.length >= 3 ? true : null);
    source = LinkActionsSource(
      link: link,
      keys: keys,
      onKeyLost: () {},
      answerWithin: const Duration(milliseconds: 500),
    );
  });

  tearDown(() async {
    await source.dispose();
    await link.dispose();
    await store.flush();
    tmp.deleteSync(recursive: true);
  });

  /// What the computer's `home` says: sessions and waiting items.
  void home({
    List<Map<String, Object?>> waiting = const [],
    List<Map<String, Object?>> sessions = const [],
  }) {
    link.home = HomeSnapshot({
      'kind': 'home',
      'sentAt': DateTime.now().millisecondsSinceEpoch,
      'computer': {
        'name': 'Studio Mac (demo)',
        'state': 'online',
        'unfinished': 1,
      },
      'sessions': sessions,
      'waiting': waiting,
      'groups': const [],
    }, DateTime.now().millisecondsSinceEpoch);
  }

  Map<String, Object?> waitingItem(String id, String what) => {
    'id': id,
    'session': 's1',
    'what': what,
    'preview': 'x',
    'cut': false,
    'risky': false,
    'atComputer': false,
    'secondsLeft': 100,
  };

  Map<String, Object?> session({bool canStop = false}) => {
    'id': 's1',
    'title': 'Fix the login retry',
    'project': 'demo-app',
    'state': 'waiting',
    'pet': {'design': 'cat', 'color': 1},
    'canStop': canStop,
  };

  Map<String, dynamic> twoQuestions() => {
    'kind': 'detail',
    'id': 'q2',
    'what': 'question',
    'text': 'q',
    'cut': false,
    'odd': false,
    'questions': [
      {
        'text': 'Round half up, or half to even?',
        'options': ['Half up', 'Half to even'],
        'multiSelect': false,
      },
      {
        'text': 'Which currencies?',
        'options': ['Euro', 'Yen'],
        'multiSelect': true,
      },
    ],
  };

  test('several questions: every part is shown, the hash covers exactly those, and one answer per part is sent', () async {
    home(waiting: [waitingItem('q2', 'question')], sessions: [session()]);
    final detail = twoQuestions();
    link.detailCache['q2'] = detail;
    final q = await source.question('q2');
    expect(q.parts, hasLength(2));
    expect(q.parts[0].question, 'Round half up, or half to even?');
    expect(q.parts[1].multiSelect, isTrue);
    expect([for (final o in q.parts[1].options) o.label], ['Euro', 'Yen']);
    final expected = await shownHash(
      text: 'q',
      cut: false,
      questions: [
        const ShownQuestion(
          text: 'Round half up, or half to even?',
          options: ['Half up', 'Half to even'],
          multiSelect: false,
        ),
        const ShownQuestion(
          text: 'Which currencies?',
          options: ['Euro', 'Yen'],
          multiSelect: true,
        ),
      ],
    );
    expect(
      q.shownHash,
      expected,
      reason: 'the crate\'s hash over all the questions that were shown',
    );

    final command = Command(
      id: source.newCommandId(),
      kind: CommandKind.answer,
      target: 'q2',
      shown: q.shownHash,
      answers: ['Half up', 'Euro, Yen'],
    );
    expect(await source.confirm('Confirm', command), GateResult.confirmed);
    unawaited(source.send(command));
    final frame = await _until(
      () => socket.sent.where((f) => f['type'] == 'send').skip(1).firstOrNull,
    );
    final plain = await openPacket(
      receiverBoxSecret: desktop.boxSecret,
      senderBoxPublic: phone.boxPublic,
      packet: b64d(frame['body'] as String),
    );
    final body = (jsonDecode(utf8.decode(plain)) as Map)['body'] as String;
    expect(jsonDecode(body)['answers'], [
      'Half up',
      'Euro, Yen',
    ], reason: 'one answer per part, in order');
  });

  test(
    'hidden characters: the computer\'s odd flag reaches the request',
    () async {
      home(waiting: [waitingItem('r1', 'request')], sessions: [session()]);
      link.detailCache['r1'] = {
        'kind': 'detail',
        'id': 'r1',
        'what': 'toolInput',
        'text': r'echo ok \u{202e} # rm -rf /',
        'cut': false,
        'odd': true,
        'questions': const [],
      };
      final r = ((await source.request('r1')) as RequestOpen).request;
      expect(r.odd, isTrue);
      link.detailCache['r2'] = {
        'kind': 'detail',
        'id': 'r2',
        'what': 'toolInput',
        'text': 'ls',
        'cut': false,
        'odd': false,
        'questions': const [],
      };
      home(
        waiting: [waitingItem('r1', 'request'), waitingItem('r2', 'request')],
        sessions: [session()],
      );
      expect(
        ((await source.request('r2')) as RequestOpen).request.odd,
        isFalse,
      );
    },
  );

  test('a plan the computer cut says so', () async {
    home(waiting: [waitingItem('p1', 'plan')], sessions: [session()]);
    link.detailCache['p1'] = {
      'kind': 'detail',
      'id': 'p1',
      'what': 'plan',
      'text': '# Plan',
      'cut': true,
      'odd': false,
      'questions': const [],
    };
    expect((await source.plan('p1')).cut, isTrue);
    link.detailCache['p2'] = {
      'kind': 'detail',
      'id': 'p2',
      'what': 'plan',
      'text': '# Plan',
      'cut': false,
      'odd': false,
      'questions': const [],
    };
    home(
      waiting: [waitingItem('p1', 'plan'), waitingItem('p2', 'plan')],
      sessions: [session()],
    );
    expect((await source.plan('p2')).cut, isFalse);
  });

  /// The computer answers a command with [outcome] (and [why]); returns what the screen gets from "Check status".
  Future<CommandOutcome> outcomeOf(String outcome, {String? why}) async {
    final cid = source.newCommandId();
    final packet = await sealPacket(
      version: versions.current,
      direction: LinkDirection.toPhone,
      stream: LinkStream.event,
      counter: BigInt.from(++counter),
      senderBoxSecret: desktop.boxSecret,
      recipientBoxPublic: phone.boxPublic,
      plaintext: Uint8List.fromList(
        utf8.encode(
          jsonEncode({
            'kind': 'result',
            'cid': cid,
            'outcome': outcome,
            'why': ?why,
          }),
        ),
      ),
    );
    socket.push({
      'type': 'msg',
      'pid': 'p$counter',
      'from': desktop.deviceId,
      'body': b64(packet),
    });
    return source.checkStatus(cid);
  }

  test('why a request was handed back: a restart, a settings change, or an answer at the computer', () async {
    expect(
      (await outcomeOf('released', why: 'restart')).why,
      ReleasedWhy.restarted,
    );
    expect(
      (await outcomeOf('released', why: 'settings')).why,
      ReleasedWhy.settings,
    );
    expect(
      (await outcomeOf('released', why: 'something else')).why,
      ReleasedWhy.answeredAtComputer,
    );
    expect((await outcomeOf('released')).kind, OutcomeKind.released);
  });

  test('Stop is offered from the computer\'s own flag, not inferred from the rights', () async {
    home(
      waiting: [waitingItem('r1', 'request')],
      sessions: [session(canStop: true)],
    );
    expect((await source.session('s1')).stopAllowed, isTrue);
    home(
      waiting: [waitingItem('r1', 'request')],
      sessions: [session(canStop: false)],
    );
    expect(
      (await source.session('s1')).stopAllowed,
      isFalse,
      reason: 'all rights, a held request, but the computer says no',
    );
    home(waiting: const [], sessions: [session(canStop: true)]);
    expect(
      (await source.session('s1')).stopAllowed,
      isFalse,
      reason: 'nothing is held: no request to deny',
    );
  });
}

Future<T> _until<T>(T? Function() f) async {
  final end = DateTime.now().add(const Duration(seconds: 5));
  while (DateTime.now().isBefore(end)) {
    final v = f();
    if (v != null) return v;
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
  throw TimeoutException('timed out');
}
