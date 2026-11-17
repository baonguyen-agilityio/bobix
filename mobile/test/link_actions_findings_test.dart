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
import 'package:phone/data/models.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/link_actions_source.dart';
import 'package:phone/link/link_phone_data.dart';
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

  test('a plan with files: the files are read from the detail and the hash covers them', () async {
    home(waiting: [waitingItem('pf', 'plan')], sessions: [session()]);
    final file = {
      'name': 'phase-01.md',
      'path': 'plans/x/phase-01.md',
      'bytes': 31,
      'text': '## Phase 1\nDo the first thing.',
      'sent': true,
    };
    link.detailCache['pf'] = {
      'kind': 'detail',
      'id': 'pf',
      'what': 'plan',
      'text': '# Plan',
      'cut': false,
      'odd': false,
      'files': [
        file,
        {
          ...file,
          'name': 'phase-02.md',
          'path': 'plans/x/phase-02.md',
          'bytes': 52430,
          'text': '',
          'sent': false,
        },
      ],
    };
    final p = await source.plan('pf');
    expect(
      [for (final f in p.files) (f.name, f.folder, f.sent, f.cut)],
      [
        ('phase-01.md', 'plans/x/', true, false),
        ('phase-02.md', 'plans/x/', false, false),
      ],
    );
    expect(p.filesPartial, isTrue);
    ShownFile shown(Map<String, Object?> f) => ShownFile(
      name: f['name'] as String,
      path: f['path'] as String,
      bytes: BigInt.from(f['bytes'] as int),
      text: f['text'] as String,
      cut: false,
      sent: f['sent'] as bool,
    );
    final both = [
      for (final f
          in (link.detailCache['pf']!['files'] as List)
              .cast<Map<String, Object?>>())
        shown(f),
    ];
    expect(
      p.shownHash,
      await shownHash(
        text: '# Plan',
        cut: false,
        questions: const [],
        files: both,
        omitted: 0,
      ),
    );
    expect(
      p.shownHash,
      isNot(
        await shownHash(
          text: '# Plan',
          cut: false,
          questions: const [],
          files: both.take(1).toList(),
          omitted: 0,
        ),
      ),
      reason: 'every file is in the hash',
    );
    expect(
      p.shownHash,
      isNot(
        await shownHash(
          text: '# Plan',
          cut: false,
          questions: const [],
          files: const [],
          omitted: 0,
        ),
      ),
    );
  });

  test('a plan that names files the computer could not show: the count is read, blocks Approve and is in the hash', () async {
    home(waiting: [waitingItem('po', 'plan')], sessions: [session()]);
    Map<String, Object?> detail(int omitted) => {
      'kind': 'detail',
      'id': 'po',
      'what': 'plan',
      'text': '# Plan',
      'cut': false,
      'odd': false,
      'files': const [],
      if (omitted > 0) 'omitted': omitted,
    };
    link.detailCache['po'] = detail(0);
    final whole = await source.plan('po');
    expect((whole.omitted, whole.filesPartial), (0, false));
    link.detailCache['po'] = detail(3);
    final more = await source.plan('po');
    expect((more.omitted, more.filesPartial), (3, true));
    expect(
      more.shownHash,
      await shownHash(
        text: '# Plan',
        cut: false,
        questions: const [],
        files: const [],
        omitted: 3,
      ),
    );
    expect(more.shownHash, isNot(whole.shownHash));
  });

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
      files: const [],
      omitted: 0,
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

  /// `source.issues()` while the computer answers with [reply].
  Future<StartList> issuesAnswered(Map<String, Object?> reply) async {
    final sentBefore = socket.sent.where((f) => f['type'] == 'send').length;
    final list = source.issues();
    await _until(
      () => socket.sent.where((f) => f['type'] == 'send').length > sentBefore
          ? true
          : null,
    );
    final packet = await sealPacket(
      version: versions.current,
      direction: LinkDirection.toPhone,
      stream: LinkStream.event,
      counter: BigInt.from(++counter),
      senderBoxSecret: desktop.boxSecret,
      recipientBoxPublic: phone.boxPublic,
      plaintext: Uint8List.fromList(utf8.encode(jsonEncode(reply))),
    );
    socket.push({
      'type': 'msg',
      'pid': 'p$counter',
      'from': desktop.deviceId,
      'body': b64(packet),
    });
    return list;
  }

  test('the issue list: repositories with their issues, a repo with none, and why it may be short', () async {
    final list = await issuesAnswered({
      'kind': 'issues',
      'repos': [
        {
          'path': 'acme/web',
          'host': 'github.com',
          'cloned': true,
          'issues': [
            {'iid': 7, 'title': 'Fix login', 'hasSession': true},
            {'iid': 8, 'title': 'Second'},
          ],
        },
        {'path': 'acme/docs', 'host': 'gitlab.com', 'cloned': false},
      ],
      'error': 'read',
    });
    expect(list.error, StartListError.read);
    expect(
      [for (final r in list.repos) (r.path, r.host, r.cloned)],
      [('acme/web', 'github.com', true), ('acme/docs', 'gitlab.com', false)],
    );
    expect(list.repos[1].issues, isEmpty);
    final first = list.repos[0].issues.first;
    expect(
      (first.id, first.number, first.title, first.hasSession),
      ('acme/web#7', 7, 'Fix login', true),
    );
    expect(list.repos[0].issues.last.hasSession, isFalse);
  });

  test(
    'a computer that refuses the issue list says this phone has no Start right',
    () async {
      final list = await issuesAnswered({
        'kind': 'error',
        'code': 'right',
        'who': 'desktop',
      });
      expect(list.error, StartListError.noRight);
      expect(list.repos, isEmpty);
    },
  );

  /// A sealed message from the computer, as the relay hands it over.
  Future<void> fromComputer(Map<String, Object?> json) async {
    final packet = await sealPacket(
      version: versions.current,
      direction: LinkDirection.toPhone,
      stream: LinkStream.event,
      counter: BigInt.from(++counter),
      senderBoxSecret: desktop.boxSecret,
      recipientBoxPublic: phone.boxPublic,
      plaintext: Uint8List.fromList(utf8.encode(jsonEncode(json))),
    );
    socket.push({
      'type': 'msg',
      'pid': 'p$counter',
      'from': desktop.deviceId,
      'body': b64(packet),
    });
  }

  /// What this phone sealed for the computer in its last `send` frames, opened.
  Future<List<Map<String, dynamic>>> toComputer() async => [
    for (final f in socket.sent.where((f) => f['type'] == 'send'))
      jsonDecode(
        utf8.decode(
          await openPacket(
            receiverBoxSecret: desktop.boxSecret,
            senderBoxPublic: phone.boxPublic,
            packet: b64d(f['body'] as String),
          ),
        ),
      ) as Map<String, dynamic>,
  ];

  test('a plan that changed: Approve is stale, the plan is fetched again, and the next Approve uses the new hash', () async {
    home(waiting: [waitingItem('pc', 'plan')], sessions: [session()]);
    Map<String, Object?> detail(String text) => {
      'kind': 'detail',
      'id': 'pc',
      'what': 'plan',
      'text': text,
      'shown': '',
      'cut': false,
      'odd': false,
    };
    link.detailCache['pc'] = detail('# Plan, first version');
    final first = await source.plan('pc');

    Future<CommandOutcome> approve(String shown, String outcome) async {
      final command = Command(
        id: source.newCommandId(),
        kind: CommandKind.approve,
        target: 'pc',
        shown: shown,
      );
      expect(await source.confirm('Confirm', command), GateResult.confirmed);
      final sent = socket.sent.length;
      final out = source.send(command);
      await _until(() => socket.sent.length > sent ? true : null);
      await fromComputer({
        'kind': 'result',
        'cid': command.id,
        'outcome': outcome,
      });
      return out;
    }

    expect((await approve(first.shownHash, 'stale')).kind, OutcomeKind.stale);
    expect(
      link.answeredHere,
      isNot(contains('pc')),
      reason: 'the computer did not take that answer',
    );

    // Opening the plan again asks the computer for it instead of showing the old copy.
    final asked = socket.sent.length;
    final again = source.plan('pc');
    await _until(() => socket.sent.length > asked ? true : null);
    final request = (await toComputer()).last;
    expect(request['kind'], isNot('command'));
    await fromComputer(detail('# Plan, second version'));
    final second = await again;
    expect(second.markdown, '# Plan, second version');
    expect(second.shownHash, isNot(first.shownHash));

    expect((await approve(second.shownHash, 'ok')).kind, OutcomeKind.applied);
    expect(link.answeredHere, contains('pc'));
  });

  test('a stale result that comes after the phone stopped waiting still drops the old plan and the answer', () async {
    final late = LinkActionsSource(
      link: link,
      keys: keys,
      onKeyLost: () {},
      answerWithin: const Duration(milliseconds: 500),
      resultWithin: const Duration(milliseconds: 100),
    );
    addTearDown(late.dispose);
    home(waiting: [waitingItem('pl', 'plan')], sessions: [session()]);
    link.detailCache['pl'] = {
      'kind': 'detail',
      'id': 'pl',
      'what': 'plan',
      'text': '# Plan',
      'shown': '',
      'cut': false,
      'odd': false,
    };
    final plan = await late.plan('pl');
    final command = Command(
      id: late.newCommandId(),
      kind: CommandKind.approve,
      target: 'pl',
      shown: plan.shownHash,
    );
    expect(await late.confirm('Confirm', command), GateResult.confirmed);
    expect(
      (await late.send(command)).kind,
      OutcomeKind.queued,
      reason: 'no result in time: shown as held',
    );
    expect(link.answeredHere, contains('pl'));
    await fromComputer({
      'kind': 'result',
      'cid': command.id,
      'outcome': 'stale',
    });
    await _until(() => link.detailCache.containsKey('pl') ? null : true);
    expect(link.answeredHere, isNot(contains('pl')));
    expect((await late.checkStatus(command.id)).kind, OutcomeKind.stale);
  });

  /// The computer says it closed [ref]; resolves once the phone has taken it in.
  Future<void> closedNotice(String ref, Map<String, Object?>? by) async {
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
            'kind': 'notice',
            'what': 'closed',
            'ref': ref,
            'session': 's1',
            'title': '',
            'body': '',
            'by': ?by,
          }),
        ),
      ),
    );
    final notice = link.closedNotices.firstWhere((n) => n.ref == ref);
    socket.push({
      'type': 'msg',
      'pid': 'p$counter',
      'from': desktop.deviceId,
      'body': b64(packet),
    });
    await notice.timeout(const Duration(seconds: 5));
  }

  /// This phone confirms and sends an Allow for [id]; the computer's result is not waited for.
  Future<void> allowFromThisPhone(String id) async {
    final command = Command(
      id: source.newCommandId(),
      kind: CommandKind.allow,
      target: id,
      shown: 'h',
    );
    expect(await source.confirm('Confirm', command), GateResult.confirmed);
    unawaited(source.send(command));
    await _until(() => link.answeredHere.contains(id) ? true : null);
  }

  test(
    'an item this phone answered is never shown as closed by the computer',
    () async {
      home(waiting: [waitingItem('r1', 'request')], sessions: [session()]);
      await allowFromThisPhone('r1');
      home(sessions: [session()]);
      final view = await source.request('r1') as RequestClosed;
      expect(view.by, ClosedBy.thisPhone);
    },
  );

  test('the computer says who closed an item: itself, another phone, this phone, or time', () async {
    home(sessions: [session()]);
    await closedNotice('c1', {'kind': 'computer'});
    await closedNotice('c2', {'kind': 'phone', 'phone': 'another-phone-id'});
    await closedNotice('c3', {'kind': 'phone', 'phone': phone.deviceId});
    await closedNotice('c4', {'kind': 'expired'});
    await closedNotice('c5', null);
    // the computer names no id to a phone that did not answer: it was a phone, not this one
    await closedNotice('c6', {'kind': 'phone', 'phone': ''});
    Future<ClosedBy> by(String id) async =>
        (await source.request(id) as RequestClosed).by;
    expect(await by('c1'), ClosedBy.computer);
    expect(await by('c2'), ClosedBy.otherPhone);
    expect(await by('c3'), ClosedBy.thisPhone);
    expect(await by('c4'), ClosedBy.expired);
    expect(await by('c6'), ClosedBy.otherPhone);
    expect(
      await by('c5'),
      ClosedBy.computer,
      reason: 'an older computer: the old guess',
    );
  });

  test('Home: a closed question keeps its kind, and this phone\'s own answer is not at the computer', () async {
    final data = LinkPhoneData(link);
    Future<Session> row() async =>
        (await data.watchHome().first).sessions.single;
    home(waiting: [waitingItem('q1', 'question')], sessions: [session()]);
    expect((await row()).needsYou, isTrue);
    await closedNotice('q1', {'kind': 'computer'});
    home(sessions: [session()]);
    var s = await row();
    expect(s.answeredAtComputer, isTrue);
    expect(
      s.waitingKind,
      WaitingKind.question,
      reason: 'not hard-coded to a request',
    );

    home(waiting: [waitingItem('r2', 'request')], sessions: [session()]);
    await row();
    await allowFromThisPhone('r2');
    home(sessions: [session()]);
    s = await row();
    expect(
      s.answeredAtComputer,
      isFalse,
      reason: 'this phone sent that answer',
    );

    home(waiting: [waitingItem('r3', 'request')], sessions: [session()]);
    await row();
    home(sessions: [session()]);
    expect(
      (await row()).answeredAtComputer,
      isTrue,
      reason: 'an older computer: the old guess',
    );
    await closedNotice('r3', {'kind': 'phone', 'phone': 'another-phone-id'});
    expect(
      (await row()).answeredAtComputer,
      isFalse,
      reason: 'the notice corrects the guess',
    );
  });

  test('a session waiting at the computer carries why, and is not one that needs this phone', () async {
    home(
      sessions: [
        {...session(), 'state': 'atComputer', 'atComputerWhy': 'tooLarge'},
      ],
    );
    final s = (await LinkPhoneData(link).watchHome().first).sessions.single;
    expect(
      (s.needsYou, s.answerAtComputer, s.answersHere, s.atComputerWhy),
      (true, true, false, 'tooLarge'),
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
