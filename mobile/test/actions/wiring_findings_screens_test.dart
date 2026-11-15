import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/plan_screen.dart';
import 'package:phone/features/actions/question_screen.dart';
import 'package:phone/features/actions/request_screen.dart';

import 'support.dart';

void main() {
  testWidgets(
    'a request the computer says has hidden characters gets the warning and the danger band',
    (tester) async {
      await pumpScreen(tester, const RequestScreen(id: 'req-odd'));
      expect(find.text(tr('en', 'rq.hid')), findsWidgets);
      expect(find.text(tr('en', 'rq.hid.d')), findsOneWidget);
    },
  );

  testWidgets(
    'a cut plan says so and cannot be approved; it can still be sent back',
    (tester) async {
      await pumpScreen(tester, const PlanScreen(id: 'plan-cut'));
      expect(find.text(tr('en', 'pl.cut')), findsOneWidget);
      expect(off(tester, tr('en', 'pl.approve')), isTrue);
      expect(off(tester, tr('en', 'pl.back')), isFalse);
      await pumpScreen(tester, const PlanScreen(id: 'plan-short'));
      expect(find.text(tr('en', 'pl.cut')), findsNothing);
      expect(off(tester, tr('en', 'pl.approve')), isFalse);
    },
  );

  testWidgets(
    'every part of a question is shown, and Send needs an answer to each, one per part',
    (tester) async {
      final src = await pumpScreen(tester, const QuestionScreen(id: 'q-two'));
      expect(find.text('Round half up, or half to even?'), findsOneWidget);
      expect(find.text('Which currencies?'), findsOneWidget);
      expect(find.text(tr('en', 'qs.part', {'n': 1, 'm': 2})), findsOneWidget);
      expect(find.text(tr('en', 'qs.part', {'n': 2, 'm': 2})), findsOneWidget);
      final send = tr('en', 'qs.send');
      expect(off(tester, send), isTrue);
      await tester.tap(find.text('Half up'));
      await tester.pump();
      expect(
        off(tester, send),
        isTrue,
        reason: 'the second part has no answer yet',
      );
      await tester.ensureVisible(find.text('Euro'));
      await tester.tap(find.text('Euro'));
      await tester.tap(find.text('Yen'));
      await tester.pump();
      expect(off(tester, send), isFalse);
      await tester.tap(button(send));
      await settle(tester);
      expect(src.sent.single.answers, ['Half up', 'Euro, Yen']);
    },
  );
}
