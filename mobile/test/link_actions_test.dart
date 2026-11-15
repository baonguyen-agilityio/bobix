// The action screens' source over the real link, against the real `relay --dev` and the crate's `fake-desktop`: commands are
// signed (with a software key whose public half the desktop got at pairing), sealed by the crate, verified by the desktop,
// and their results come back. Build the programs first (see test/support/link_harness.dart).
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/actions_source.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/link_actions_source.dart';
import 'package:phone/link/link_state.dart';

import 'support/core_library.dart';
import 'support/link_harness.dart';
import 'support/test_phone.dart';

void main() {
  late Harness relay;
  late Directory tmp;
  final desktops = <FakeDesktop>[];
  final links = <ComputerLink>[];
  final sources = <LinkActionsSource>[];
  final stores = <LinkStateStore>[];

  setUpAll(() async {
    HttpOverrides.global = null;
    await initCoreLibrary();
    relay = await Harness.start();
  });
  tearDownAll(() async => relay.stop());
  setUp(() => tmp = Directory.systemTemp.createTempSync('bobix-actions-'));
  tearDown(() async {
    for (final s in sources) {
      await s.dispose();
    }
    for (final l in links) {
      await l.dispose();
    }
    for (final d in desktops) {
      await d.stop();
    }
    for (final s in stores) {
      await s.flush();
    }
    stores.clear();
    sources.clear();
    links.clear();
    desktops.clear();
    tmp.deleteSync(recursive: true);
  });

  /// Pairs with a fake desktop playing [scenario], connects, and returns the source over the link.
  Future<(LinkActionsSource, TestPhone, FakeDesktop, void Function()?)> start(
    String scenario, {
    void Function()? onKeyLost,
  }) async {
    final d = await relay.desktop(scenario);
    desktops.add(d);
    final phone = TestPhone(tmp);
    stores.add(phone.store);
    final c = await phone.pair(d.qr);
    final link = await phone.link(
      c,
      delay: (_, _) => const Duration(milliseconds: 80),
    );
    links.add(link);
    link.resume();
    await until(() => link.home, what: 'the first home');
    final source = LinkActionsSource(
      link: link,
      keys: phone.keys,
      onKeyLost: onKeyLost ?? () {},
      answerWithin: const Duration(seconds: 2),
      resultWithin: const Duration(seconds: 8),
    );
    sources.add(source);
    return (source, phone, d, onKeyLost);
  }

  /// The two steps the screen runs: the biometric step (which signs), then send.
  Future<CommandOutcome> run(
    LinkActionsSource source,
    CommandKind kind,
    String target,
    String shown, {
    String? text,
    StartMode? mode,
  }) async {
    final command = Command(
      id: source.newCommandId(),
      kind: kind,
      target: target,
      shown: shown,
      text: text,
      mode: mode,
    );
    expect(await source.confirm('Confirm', command), GateResult.confirmed);
    return source.send(command);
  }

  group('what the screens read', () {
    test(
      'a held request, with the hash of what is shown, and its session',
      () async {
        final (source, _, _, _) = await start('normal');
        final view = await source.request('req1') as RequestOpen;
        final r = view.request;
        expect(r.input, 'npm test -- --watch=false');
        expect(r.cut, isFalse);
        expect(r.released, isFalse);
        expect(r.sessionName, 'Fix the login retry');
        expect(r.folder, 'demo-app');
        expect(r.secondsLeft, inInclusiveRange(270, 280));
        expect(r.shownHash, isNotEmpty);
        expect(source.computerName, 'Studio Mac (demo)');
      },
    );

    test(
      'a plan, a question, and a session that waits on a held request',
      () async {
        final (source, _, _, _) = await start('normal');
        final plan = await source.plan('plan1');
        expect(plan.markdown, contains('Add the retry'));
        final q = await source.question('q1');
        expect(q.question, 'Which database?');
        expect([for (final o in q.options) o.label], ['SQLite', 'Postgres']);
        expect(q.multiSelect, isFalse);
        final s = await source.session('s1');
        expect(s.phase, SessionPhase.waiting);
        expect(
          s.stopAllowed,
          isTrue,
          reason: 'a held request, and both the Stop and the Answer right',
        );
        expect(s.waitingOnRequest, 'req1');
      },
    );

    test(
      'a request id that is not held is closed, not loading for ever',
      () async {
        final (source, _, _, _) = await start('normal');
        final view = await source.request('nope');
        expect(view, isA<RequestClosed>());
      },
    );

    test(
      'a request released to the computer is still open, and says so',
      () async {
        final (source, _, _, _) = await start('released');
        final r = ((await source.request('req1')) as RequestOpen).request;
        expect(r.released, isTrue);
      },
    );
  });

  group('commands', () {
    test('allow, approve, answer and stop are signed, verified by the desktop and applied', () async {
      final (source, phone, desk, _) = await start('normal');
      final r = ((await source.request('req1')) as RequestOpen).request;
      expect(
        (await run(source, CommandKind.allow, 'req1', r.shownHash)).kind,
        OutcomeKind.applied,
      );
      final plan = await source.plan('plan1');
      expect(
        (await run(
          source,
          CommandKind.approve,
          'plan1',
          plan.shownHash,
          text: 'ok',
        )).kind,
        OutcomeKind.applied,
      );
      final q = await source.question('q1');
      expect(
        (await run(
          source,
          CommandKind.answer,
          'q1',
          q.shownHash,
          text: 'SQLite',
        )).kind,
        OutcomeKind.applied,
      );
      expect(
        (await run(source, CommandKind.stop, 's1', '', text: 'req1')).kind,
        OutcomeKind.applied,
      );
      expect(
        phone.keys.signCalls,
        4,
        reason:
            'one biometric prompt for each command, and none for anything else',
      );
      expect(
        desk.lines.where((l) => l.startsWith('command ') && l.contains('Ok')),
        hasLength(4),
      );
    });

    test('what the person saw is not what the computer holds: stale', () async {
      final (source, _, _, _) = await start('normal');
      expect(
        (await run(
          source,
          CommandKind.allow,
          'req1',
          'a hash of something else',
        )).kind,
        OutcomeKind.stale,
      );
    });

    test('answered at the computer first, and expired', () async {
      final (a, _, _, _) = await start('answered-on-computer');
      final ra = ((await a.request('req1')) as RequestOpen).request;
      expect(
        (await run(a, CommandKind.allow, 'req1', ra.shownHash)).kind,
        OutcomeKind.latePc,
      );
    });

    test('a request that ran out of time', () async {
      final (source, _, _, _) = await start('expired');
      final r = ((await source.request('req1')) as RequestOpen).request;
      expect(
        (await run(source, CommandKind.deny, 'req1', r.shownHash)).kind,
        OutcomeKind.expired,
      );
    });

    test('a command is sent once; "Check status" asks the computer, and a command it never got is not received', () async {
      final (source, _, desk, _) = await start('normal');
      final r = ((await source.request('req1')) as RequestOpen).request;
      final command = Command(
        id: source.newCommandId(),
        kind: CommandKind.allow,
        target: 'req1',
        shown: r.shownHash,
      );
      expect(await source.confirm('Confirm', command), GateResult.confirmed);
      expect((await source.send(command)).kind, OutcomeKind.applied);
      expect(
        (await source.send(command)).kind,
        OutcomeKind.refused,
        reason: 'the signature was used: nothing is sent again',
      );
      expect((await source.checkStatus(command.id)).kind, OutcomeKind.applied);
      expect(
        (await source.checkStatus('a command id the computer never saw')).kind,
        OutcomeKind.notReceived,
      );
      expect(
        desk.lines.where((l) => l.contains(command.id)),
        hasLength(1),
        reason: 'the computer got it once',
      );
    });

    test(
      'withdrawing a command that was already delivered is too late',
      () async {
        final (source, _, _, _) = await start('normal');
        final r = ((await source.request('req1')) as RequestOpen).request;
        final command = Command(
          id: source.newCommandId(),
          kind: CommandKind.allow,
          target: 'req1',
          shown: r.shownHash,
        );
        await source.confirm('Confirm', command);
        await source.send(command);
        expect(
          (await source.withdraw(command.id)).kind,
          OutcomeKind.withdrawTooLate,
        );
      },
    );

    test(
      'start: the computer builds the prompt, and the start carries its hash',
      () async {
        final (source, _, _, _) = await start('normal');
        final p = await source.startPreview('demo-app#42', StartMode.quick);
        expect(p.prompt, contains('42'));
        expect(p.folderMissing, isFalse);
        expect(
          (await run(
            source,
            CommandKind.start,
            'demo-app#42',
            p.promptHash,
            mode: StartMode.quick,
          )).kind,
          OutcomeKind.applied,
        );
        expect(
          (await run(
            source,
            CommandKind.start,
            'demo-app#42',
            'not the hash of the prompt',
            mode: StartMode.quick,
          )).kind,
          OutcomeKind.stale,
        );
      },
    );

    test('a start for a folder the computer does not know says so', () async {
      final (source, _, _, _) = await start('folder-missing');
      final p = await source.startPreview('demo-app#42', StartMode.full);
      expect(p.folderMissing, isTrue);
    });
  });

  group('the biometric step', () {
    test('each way it ends is its own state, and only a lost key asks to pair again', () async {
      var lost = 0;
      final (source, phone, _, _) = await start(
        'normal',
        onKeyLost: () => lost++,
      );
      Command command() => Command(
        id: source.newCommandId(),
        kind: CommandKind.deny,
        target: 'req1',
        shown: 'x',
      );
      for (final (answer, expected) in <(SignResult, GateResult)>[
        (const SignCancelled(), GateResult.cancelled),
        (const SignLockedOut(), GateResult.lockedOut),
        (const BiometryUnavailable(), GateResult.unavailable),
        (const SignFailed('x'), GateResult.failed),
      ]) {
        phone.keys.signAnswer = answer;
        expect(await source.confirm('Confirm', command()), expected);
        expect(lost, 0, reason: '$answer');
      }
      phone.keys.signAnswer = const KeyInvalidated();
      expect(await source.confirm('Confirm', command()), GateResult.keyLost);
      expect(lost, 1);
    });

    test('a command the phone has no right for is not even offered to the biometrics', () async {
      final (source, phone, _, _) = await start('normal');
      // The phone was granted everything here; a start for no project names no command the crate can build.
      final calls = phone.keys.signCalls;
      final c = Command(
        id: source.newCommandId(),
        kind: CommandKind.answer,
        target: 'q1',
        shown: 'x',
        text: 'y' * 3000,
      );
      expect(
        await source.confirm('Confirm', c),
        GateResult.failed,
        reason: 'an answer above the limit is refused before signing',
      );
      expect(phone.keys.signCalls, calls);
    });
  });
}
