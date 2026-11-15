import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/fixture_actions_source.dart';
import 'package:phone/features/actions/request_screen.dart';
import 'package:phone/features/actions/result_screen.dart';
import 'package:phone/fixtures/fixtures.dart';

import 'support.dart';

const _cmd = Command(
  id: 'c1',
  kind: CommandKind.allow,
  target: 'req-bash',
  shown: 'h',
);

Widget screen(CommandOutcome o, [Command c = _cmd]) =>
    ResultScreen(command: c, first: Future.value(o));

void main() {
  // Each result state: its outcome and the title key it must show.
  final states = <(CommandOutcome, String, Map<String, Object>)>[
    (const CommandOutcome(OutcomeKind.applied), 'res.ok.allow.t', {}),
    (const CommandOutcome(OutcomeKind.queued), 'res.queued.t', {}),
    (const CommandOutcome(OutcomeKind.latePc), 'res.late-pc.t', {}),
    (
      const CommandOutcome(OutcomeKind.latePhone, phone: 'Anna’s iPhone'),
      'res.late-phone.t',
      {},
    ),
    (const CommandOutcome(OutcomeKind.expired), 'res.expired.t', {}),
    (
      const CommandOutcome(
        OutcomeKind.released,
        why: ReleasedWhy.answeredAtComputer,
      ),
      'res.released.t',
      {},
    ),
    (const CommandOutcome(OutcomeKind.stale), 'res.stale.t', {}),
    (const CommandOutcome(OutcomeKind.clock), 'res.clk.t', {}),
    (
      const CommandOutcome(OutcomeKind.refused, reason: 'no such request'),
      'res.refused.t',
      {'c': fixtureComputer},
    ),
    (const CommandOutcome(OutcomeKind.notReceived), 'res.notrecv.t', {}),
    (const CommandOutcome(OutcomeKind.withdrawn), 'res.wd.t', {}),
    (const CommandOutcome(OutcomeKind.withdrawTooLate), 'res.wd.late.t', {}),
    (const CommandOutcome(OutcomeKind.sessionEnded), 'res.ended.t', {}),
  ];

  for (final lang in langs) {
    for (final b in brightnesses) {
      group('result states ($lang, ${b.name})', () {
        for (final st in states) {
          testWidgets(st.$1.kind.name, (tester) async {
            await pumpScreen(tester, screen(st.$1), lang: lang, brightness: b);
            expect(find.text(tr(lang, st.$2, st.$3)), findsOneWidget);
            expect(button(tr(lang, 'a.home')), findsOneWidget);
          });
        }

        testWidgets('applied lists what was answered, to whom, and the log', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            screen(const CommandOutcome(OutcomeKind.applied)),
            lang: lang,
            brightness: b,
          );
          expect(find.text(tr(lang, 'res.what.allow')), findsOneWidget);
          expect(
            find.text(tr(lang, 'res.logged', {'c': fixtureComputer})),
            findsOneWidget,
          );
        });
      });
    }
  }

  group('result behaviour', () {
    testWidgets('says "sending" until the result comes', (tester) async {
      await pumpScreen(
        tester,
        ResultScreen(
          command: _cmd,
          first: Future.delayed(
            const Duration(seconds: 1),
            () => const CommandOutcome(OutcomeKind.applied),
          ),
        ),
      );
      expect(find.text(tr('en', 'res.sending')), findsOneWidget);
      await tester.pump(const Duration(seconds: 2));
      expect(find.text(tr('en', 'res.ok.allow.t')), findsOneWidget);
    });

    testWidgets('a held command: Check status asks, and never sends again', (
      tester,
    ) async {
      final src = FixtureActionsSource(
        forced: const CommandOutcome(OutcomeKind.queued),
      );
      await src.send(_cmd);
      await pumpScreen(
        tester,
        screen(const CommandOutcome(OutcomeKind.queued)),
        source: src,
      );
      await tester.tap(button(tr('en', 'res.check')));
      await tester.pump();
      await tester.pump();
      expect(find.text(tr('en', 'res.check.r')), findsOneWidget);
      expect(find.text(tr('en', 'res.queued.t')), findsOneWidget);
      expect(src.sent, hasLength(1));
    });

    testWidgets('a lost result: Check status says "not received"', (
      tester,
    ) async {
      final src = FixtureActionsSource(
        checked: const CommandOutcome(OutcomeKind.notReceived),
      );
      await pumpScreen(
        tester,
        screen(const CommandOutcome(OutcomeKind.queued)),
        source: src,
      );
      await tester.tap(button(tr('en', 'res.check')));
      await tester.pump();
      await tester.pump();
      expect(find.text(tr('en', 'res.notrecv.t')), findsOneWidget);
      expect(button(tr('en', 'res.withdraw')), findsOneWidget);
      expect(src.sent, isEmpty);
    });

    testWidgets('Withdraw in time says withdrawn', (tester) async {
      await pumpScreen(
        tester,
        screen(const CommandOutcome(OutcomeKind.queued)),
      );
      await tester.tap(button(tr('en', 'res.withdraw')));
      await tester.pump();
      await tester.pump();
      expect(find.text(tr('en', 'res.wd.t')), findsOneWidget);
      expect(find.text(tr('en', 'res.withdrawn')), findsOneWidget);
      expect(button(tr('en', 'res.withdraw')), findsNothing);
    });

    testWidgets('Withdraw too late says so', (tester) async {
      await pumpScreen(
        tester,
        screen(const CommandOutcome(OutcomeKind.notReceived)),
        source: FixtureActionsSource(
          withdrawn: const CommandOutcome(OutcomeKind.withdrawTooLate),
        ),
      );
      await tester.tap(button(tr('en', 'res.withdraw')));
      await tester.pump();
      await tester.pump();
      expect(find.text(tr('en', 'res.wd.late.t')), findsOneWidget);
    });

    testWidgets('finished results offer no way to send again', (tester) async {
      await pumpScreen(
        tester,
        screen(const CommandOutcome(OutcomeKind.latePc)),
      );
      expect(button(tr('en', 'res.check')), findsNothing);
      expect(button(tr('en', 'res.withdraw')), findsNothing);
      expect(button(tr('en', 'ask.allow')), findsNothing);
    });

    testWidgets('checking and withdrawing are off while sending is off', (
      tester,
    ) async {
      await pumpScreen(
        tester,
        screen(const CommandOutcome(OutcomeKind.queued)),
        conn: ConnState.relayDown,
      );
      expect(off(tester, tr('en', 'res.check')), isTrue);
      expect(off(tester, tr('en', 'res.withdraw')), isTrue);
    });

    for (final o in [
      (OutcomeKind.latePc, 'res.late-pc.t'),
      (OutcomeKind.latePhone, 'res.late-phone.t'),
      (OutcomeKind.expired, 'res.expired.t'),
      (OutcomeKind.stale, 'res.stale.t'),
      (OutcomeKind.clock, 'res.clk.t'),
    ]) {
      testWidgets('the flow ends in ${o.$1.name} when the computer says so', (
        tester,
      ) async {
        final src = await pumpScreen(
          tester,
          const RequestScreen(id: 'req-bash'),
          source: FixtureActionsSource(
            forced: CommandOutcome(o.$1, phone: 'Anna’s iPhone'),
          ),
        );
        await tester.tap(button('Allow'));
        await settle(tester);
        expect(src.sent, hasLength(1));
        expect(find.text(tr('en', o.$2)), findsOneWidget);
      });
    }
  });
}
