// A long (Vietnamese) header title may wrap to a second line between the Back and Log pills; it is never cut with an ellipsis.
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
        // The test font is far wider than a real one, so what is checked is the rule: two lines allowed, no ellipsis.
        final text = tester.widget<Text>(title);
        expect(text.maxLines, 2);
        expect(text.overflow, isNull);
        expect(
          tester.renderObject<RenderParagraph>(title).size.height,
          lessThan(52),
        );
        expect(tester.takeException(), isNull);
      });
    }
  }
}
