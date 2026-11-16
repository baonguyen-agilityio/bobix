// A long (Vietnamese) header title may wrap to a second line between the Back and Log pills; an ellipsis is only the backstop after the last allowed line, and the header grows with the text.
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/action_widgets.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/theme/theme.dart';
import 'package:phone/widgets/detail_scaffold.dart';

class _Vi extends Lang {
  @override
  String build() => 'vi';
}

class _En extends Lang {
  @override
  String build() => 'en';
}

void main() {
  for (final width in [390.0, 440.0]) {
    for (final (name, page) in <(String, Widget)>[
      (
        'ActionPage with Back and Log',
        ActionPage(
          title: 'Kế hoạch chờ duyệt',
          onLog: () {},
          children: const [],
        ),
      ),
      (
        'DetailScaffold with Back',
        const DetailScaffold(title: 'Quyền của điện thoại này', children: []),
      ),
    ]) {
      testWidgets('$name: the title is not cut at $width pt', (tester) async {
        tester.view
          ..physicalSize = Size(width, 844)
          ..devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        await tester.pumpWidget(
          ProviderScope(
            overrides: [langProvider.overrideWith(_Vi.new)],
            child: MaterialApp(theme: bobixTheme(Brightness.light), home: page),
          ),
        );
        await tester.pump();
        final title = find.byWidgetPredicate(
          (w) =>
              w is Text &&
              (w.data == 'Kế hoạch chờ duyệt' ||
                  w.data == 'Quyền của điện thoại này'),
        );
        expect(title, findsOneWidget);
        // The test font is far wider than a real one, so what is checked is the rule: two lines allowed, ellipsis only as a backstop.
        final text = tester.widget<Text>(title);
        expect(text.maxLines, 2);
        expect(text.overflow, TextOverflow.ellipsis);
        expect(
          tester.renderObject<RenderParagraph>(title).size.height,
          lessThan(52),
        );
        expect(tester.takeException(), isNull);
      });
    }
  }

  for (final scale in [1.3, 1.6]) {
    for (final title in ['Kế hoạch chờ duyệt', 'Plan waiting for approval']) {
      testWidgets(
        'text scale $scale, "$title": the header grows, nothing is clipped',
        (tester) async {
          tester.view
            ..physicalSize = const Size(390, 844)
            ..devicePixelRatio = 1;
          addTearDown(tester.view.reset);
          await tester.pumpWidget(
            ProviderScope(
              // English pill labels: the test font is one em wide per letter, so the long Vietnamese pill labels would
              // overflow for a reason that has nothing to do with the title.
              overrides: [langProvider.overrideWith(_En.new)],
              child: MaterialApp(
                theme: bobixTheme(Brightness.light),
                builder: (context, child) => MediaQuery(
                  data: MediaQuery.of(context)
                      .copyWith(textScaler: TextScaler.linear(scale)),
                  child: child!,
                ),
                home: ActionPage(
                  title: title,
                  onLog: () {},
                  children: const [],
                ),
              ),
            ),
          );
          await tester.pump();
          final text = find.text(title);
          final paragraph = tester.renderObject<RenderParagraph>(text);
          final header = find
              .ancestor(of: text, matching: find.byType(Row))
              .first;
          // The row that holds the title grew with it (never clipped), and ellipsis is only the backstop.
          expect(
            tester.getSize(header).height,
            greaterThanOrEqualTo(paragraph.size.height),
          );
          expect(tester.getSize(header).height, greaterThanOrEqualTo(52));
          expect(tester.widget<Text>(text).maxLines, 2);
          expect(tester.widget<Text>(text).overflow, TextOverflow.ellipsis);
          expect(tester.takeException(), isNull);
        },
      );
    }
  }
}
