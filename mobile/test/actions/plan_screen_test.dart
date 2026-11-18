import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/plan_file_screen.dart';
import 'package:phone/features/actions/plan_screen.dart';
import 'package:phone/data/models.dart';

import 'support.dart';

/// A taller screen, so the whole list of files is built (a list builds only what is near the screen).
Future<void> tall(WidgetTester tester) async {
  tester.view.physicalSize = const Size(390 * 3, 1500 * 3);
  await tester.pump();
}

String allText(WidgetTester tester) => tester
    .widgetList<RichText>(find.byType(RichText))
    .map((w) => w.text.toPlainText())
    .join('\n');

void main() {
  for (final lang in langs) {
    for (final b in brightnesses) {
      group('plan screen ($lang, ${b.name})', () {
        testWidgets('short plan: shown whole, with the auto-mode note', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            const PlanScreen(id: 'plan-short'),
            lang: lang,
            brightness: b,
          );
          expect(
            find.textContaining('roundCents', findRichText: true),
            findsWidgets,
          );
          expect(find.text(tr(lang, 'pl.auto')), findsOneWidget);
          expect(button(tr(lang, 'pl.approve')), findsOneWidget);
          expect(button(tr(lang, 'pl.back')), findsOneWidget);
        });

        testWidgets('long plan: the whole plan is inline, nothing to tap', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            const PlanScreen(id: 'plan-long'),
            lang: lang,
            brightness: b,
          );
          expect(find.byType(TextButton), findsNothing);
          expect(
            find.textContaining(
              'No downtime window is needed.',
              findRichText: true,
            ),
            findsWidgets,
          );
        });
      });
    }
  }

  group('plan content', () {
    testWidgets('an image address makes no request and shows its alt text', (
      tester,
    ) async {
      await pumpScreen(tester, const PlanScreen(id: 'plan-long'));
      expect(find.byType(Image), findsNothing);
      expect(allText(tester), contains('[image: the timing chart]'));
    });

    testWidgets('links are plain text and nothing is tappable', (tester) async {
      await pumpScreen(tester, const PlanScreen(id: 'plan-long'));
      final text = allText(tester);
      expect(text, contains('the design note (https://example.test/design)'));
      expect(text, contains('https://example.test/raw'));
      final recognizers = <GestureRecognizer>[];
      for (final w in tester.widgetList<RichText>(find.byType(RichText))) {
        w.text.visitChildren((s) {
          if (s is TextSpan && s.recognizer != null) {
            recognizers.add(s.recognizer!);
          }
          return true;
        });
      }
      expect(recognizers, isEmpty);
    });

    testWidgets('lists, code blocks and tables render', (tester) async {
      await pumpScreen(tester, const PlanScreen(id: 'plan-long'));
      final text = allText(tester);
      expect(text, contains('npm run build'));
      expect(find.byType(Table), findsOneWidget);
      expect(text, contains('db/migrations/12.sql'));
    });
  });

  group('a plan with several files', () {
    for (final lang in langs) {
      testWidgets('lists each file and opens one read only ($lang)', (
        tester,
      ) async {
        final src = await pumpScreen(
          tester,
          const PlanScreen(id: 'plan-files'),
          lang: lang,
        );
        await tall(tester);
        expect(find.text(tr(lang, 'pf.sec', {'n': 3})), findsOneWidget);
        for (final n in [
          'phase-01-link-messages.md',
          'phase-02-desktop-issue-list.md',
          'phase-03-phone-start-screen.md',
        ]) {
          expect(find.text(n), findsOneWidget);
        }
        expect(
          find.text(tr(lang, 'pf.ok', {'n': 3})),
          findsOneWidget,
          reason: 'what Approve covers',
        );
        expect(off(tester, tr(lang, 'pl.approve')), isFalse);
        expect(find.text(tr(lang, 'pf.cut')), findsNothing);
        await tester.ensureVisible(find.text('phase-02-desktop-issue-list.md'));
        await tester.tap(find.text('phase-02-desktop-issue-list.md'));
        await tester.pumpAndSettle();
        expect(find.byType(PlanFileScreen), findsOneWidget);
        expect(
          find.text('phase-02-desktop-issue-list.md'),
          findsWidgets,
          reason: 'the title and the card',
        );
        expect(
          find.textContaining('plans/261117-start-repos/', findRichText: true),
          findsOneWidget,
        );
        expect(
          allText(tester),
          contains('No repository ticked means an empty'),
        );
        expect(
          find.text(
            tr(lang, 'pf.ro', {'c': src.computerName}),
            findRichText: true,
          ),
          findsOneWidget,
        );
        expect(
          find.text('2,7 KB'),
          lang == 'vi' ? findsOneWidget : findsNothing,
        );
        expect(
          find.text('2.7 KB'),
          lang == 'en' ? findsOneWidget : findsNothing,
        );
        expect(find.byType(ActionChip), findsNothing);
        expect(
          button(tr(lang, 'pl.approve')),
          findsNothing,
          reason: 'no action bar',
        );
      });
    }

    testWidgets('a long file is clamped until the whole file is asked for', (
      tester,
    ) async {
      await pumpScreen(tester, const PlanScreen(id: 'plan-files'));
      await tall(tester);
      await tester.ensureVisible(find.text('phase-03-phone-start-screen.md'));
      await tester.tap(find.text('phase-03-phone-start-screen.md'));
      await tester.pumpAndSettle();
      final lines = 16;
      expect(find.text(tr('en', 'pf.all', {'n': lines})), findsOneWidget);
      final whole = find.text(tr('en', 'pf.all', {'n': lines}));
      final closedAt = tester.getTopLeft(whole).dy;
      await tester.tap(whole);
      await tester.pumpAndSettle();
      final less = find.text(tr('en', 'pl.less'));
      expect(less, findsOneWidget);
      expect(
        tester.getTopLeft(less).dy,
        greaterThan(closedAt),
        reason: 'the rest of the file is in the page now',
      );
    });

    testWidgets(
      'a file too large to send is listed, cannot be opened, and blocks Approve',
      (tester) async {
        final src = await pumpScreen(
          tester,
          const PlanScreen(id: 'plan-files-big'),
        );
        await tall(tester);
        expect(find.text(tr('en', 'pf.bad.h')), findsOneWidget);
        expect(find.text(tr('en', 'pf.unsent')), findsOneWidget);
        expect(
          find.text(tr('en', 'pf.big', {'c': src.computerName})),
          findsOneWidget,
        );
        expect(find.text('51.2 KB'), findsOneWidget);
        expect(find.text(tr('en', 'pf.bad.s')), findsOneWidget);
        expect(find.text(tr('en', 'pf.ok', {'n': 3})), findsNothing);
        expect(off(tester, tr('en', 'pl.approve')), isTrue);
        expect(
          off(tester, tr('en', 'pl.back')),
          isFalse,
          reason: 'sending back still works',
        );
        await tester.ensureVisible(find.text('phase-02-desktop-issue-list.md'));
        await tester.tap(
          find.text('phase-02-desktop-issue-list.md'),
          warnIfMissed: false,
        );
        await tester.pump(const Duration(milliseconds: 400));
        expect(
          find.byType(PlanFileScreen),
          findsNothing,
          reason: 'a file that was not sent has nothing to open',
        );
        await tester.tap(button(tr('en', 'pl.approve')), warnIfMissed: false);
        await settle(tester);
        expect(src.sent, isEmpty);
      },
    );

    testWidgets(
      'files the plan names that are not shown at all are counted, and block Approve',
      (tester) async {
        final src = await pumpScreen(
          tester,
          const PlanScreen(id: 'plan-files-more'),
        );
        await tall(tester);
        expect(find.text(tr('en', 'pf.more', {'n': 2})), findsOneWidget);
        expect(
          find.text(tr('en', 'pf.more.d', {'c': src.computerName})),
          findsOneWidget,
        );
        expect(find.text(tr('en', 'pf.more.s', {'n': 2})), findsOneWidget);
        expect(find.text(tr('en', 'pf.ok', {'n': 3})), findsNothing);
        expect(off(tester, tr('en', 'pl.approve')), isTrue);
        expect(off(tester, tr('en', 'pl.back')), isFalse);
        await tester.tap(button(tr('en', 'pl.approve')), warnIfMissed: false);
        await settle(tester);
        expect(src.sent, isEmpty);
      },
    );

    testWidgets(
      'a shortened file says so in the list and on its own screen, and blocks Approve',
      (tester) async {
        await pumpScreen(tester, const PlanScreen(id: 'plan-files-cut'));
        await tall(tester);
        expect(find.text(tr('en', 'pf.cut')), findsOneWidget);
        expect(off(tester, tr('en', 'pl.approve')), isTrue);
        await tester.ensureVisible(find.text('phase-03-phone-start-screen.md'));
        await tester.tap(find.text('phase-03-phone-start-screen.md'));
        await tester.pumpAndSettle();
        expect(
          find.text(tr('en', 'pf.cut')),
          findsNWidgets(2),
          reason: 'the badge and the banner',
        );
        expect(
          find.text(
            tr('en', 'pf.cutn', {'c': 'Studio Mac'}),
            findRichText: true,
          ),
          findsOneWidget,
        );
      },
    );
  });

  group('plan commands', () {
    testWidgets('Approve: confirm, send once with the plan hash', (
      tester,
    ) async {
      final src = await pumpScreen(tester, const PlanScreen(id: 'plan-short'));
      await tester.tap(button('Approve plan'));
      await settle(tester);
      expect(src.sent, hasLength(1));
      expect(src.sent.single.kind, CommandKind.approve);
      expect(src.sent.single.shown, 'h-plan-plan-short');
      expect(find.text(tr('en', 'res.ok.approve.t')), findsOneWidget);
    });

    testWidgets('Send back with a note sends the note unchanged', (
      tester,
    ) async {
      final src = await pumpScreen(tester, const PlanScreen(id: 'plan-short'));
      await tester.tap(button('Send back'));
      await tester.pumpAndSettle();
      expect(find.byType(PlanNoteScreen), findsOneWidget);
      await tester.tap(find.text(tr('en', 'pn.c3')));
      await tester.enterText(find.byType(TextField), 'Giữ API cũ, thêm test');
      await tester.pump();
      await tester.tap(button(tr('en', 'pn.send')));
      await settle(tester);
      expect(src.sent.single.kind, CommandKind.back);
      expect(src.sent.single.text, 'Giữ API cũ, thêm test');
      expect(find.text(tr('en', 'res.ok.back.t')), findsOneWidget);
    });

    testWidgets('suggestion chips are 44 pt capsules and add their text', (
      tester,
    ) async {
      await pumpScreen(tester, const PlanScreen(id: 'plan-short'));
      await tester.tap(button('Send back'));
      await tester.pumpAndSettle();
      expect(find.byType(ActionChip), findsNothing);
      final chip = find.ancestor(
        of: find.text(tr('en', 'pn.c1')),
        matching: find.byType(InkWell),
      );
      expect(tester.getSize(chip.first).height, greaterThanOrEqualTo(44));
      await tester.tap(find.text(tr('en', 'pn.c1')));
      await tester.pump();
      final field = tester.widget<TextField>(find.byType(TextField));
      expect(field.controller!.text, tr('en', 'pn.c1'));
    });

    testWidgets('the note may be empty, as at the computer', (tester) async {
      final src = await pumpScreen(tester, const PlanScreen(id: 'plan-short'));
      await tester.tap(button('Send back'));
      await tester.pumpAndSettle();
      expect(off(tester, tr('en', 'pn.send')), isFalse);
      await tester.tap(button(tr('en', 'pn.send')));
      await settle(tester);
      expect(src.sent.single.text, '');
    });

    testWidgets('Approve is off while sending is off', (tester) async {
      await pumpScreen(
        tester,
        const PlanScreen(id: 'plan-short'),
        conn: ConnState.noNetwork,
      );
      expect(off(tester, 'Approve plan'), isTrue);
      expect(off(tester, 'Send back'), isTrue);
    });
  });
}
