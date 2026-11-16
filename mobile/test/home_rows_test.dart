import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/theme/theme.dart';
import 'package:phone/widgets/session_row.dart';

void main() {
  testWidgets('every task card is as high as the next, whatever its words', (
    tester,
  ) async {
    Widget row(String title, String subtitle, [String meta = 'project #1']) =>
        SessionRow(
          petDesign: 'cat',
          petColor: 1,
          petState: 'working',
          title: title,
          subtitle: subtitle,
          meta: meta,
        );
    await tester.pumpWidget(
      MaterialApp(
        theme: bobixTheme(Brightness.light),
        home: Scaffold(
          body: Column(
            children: [
              row('Short', 'Now Bash'),
              row(
                'Short',
                'Now Bash',
                List.filled(8, 'a-long-project-name').join('-'),
              ),
              row(
                List.filled(30, 'a very long title').join(' '),
                List.filled(30, 'and a long line').join(' '),
              ),
            ],
          ),
        ),
      ),
    );
    final heights = [
      for (final r in tester.widgetList(find.byType(SessionRow)))
        tester.getSize(find.byWidget(r)).height,
    ];
    expect(heights.toSet().length, 1, reason: '$heights');
  });
}
