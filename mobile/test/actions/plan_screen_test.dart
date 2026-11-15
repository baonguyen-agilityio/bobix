import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/plan_screen.dart';
import 'package:phone/fixtures/fixtures.dart';

import 'support.dart';

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
          expect(find.text(tr(lang, 'pl.less')), findsNothing);
        });

        testWidgets('long plan: collapsed, then the whole plan', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            const PlanScreen(id: 'plan-long'),
            lang: lang,
            brightness: b,
          );
          final more = find.textContaining(
            tr(lang, 'pl.all', {'n': 0}).split('0').first,
          );
          expect(more, findsOneWidget);
          await tester.tap(more);
          await tester.pump();
          expect(find.text(tr(lang, 'pl.less')), findsOneWidget);
          expect(allText(tester), contains('No downtime window is needed.'));
        });
      });
    }
  }

  group('plan content', () {
    testWidgets('an image address makes no request and shows its alt text', (
      tester,
    ) async {
      await pumpScreen(tester, const PlanScreen(id: 'plan-long'));
      await tester.tap(find.textContaining('Show the whole plan'));
      await tester.pump();
      expect(find.byType(Image), findsNothing);
      expect(allText(tester), contains('[image: the timing chart]'));
    });

    testWidgets('links are plain text and nothing is tappable', (tester) async {
      await pumpScreen(tester, const PlanScreen(id: 'plan-long'));
      await tester.tap(find.textContaining('Show the whole plan'));
      await tester.pump();
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
      await tester.tap(find.textContaining('Show the whole plan'));
      await tester.pump();
      final text = allText(tester);
      expect(text, contains('npm run build'));
      expect(find.byType(Table), findsOneWidget);
      expect(text, contains('db/migrations/12.sql'));
    });
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
