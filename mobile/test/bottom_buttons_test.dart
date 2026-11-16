// Two buttons side by side in the bottom bar have the same width.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/action_screen_parts.dart';
import 'package:phone/features/actions/action_widgets.dart';
import 'package:phone/theme/theme.dart';

void main() {
  testWidgets('the secondary and the primary button are equally wide', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: bobixTheme(Brightness.light),
        home: Scaffold(
          body: ActionButtons(
            secondary: ActionButton('Deny', onPressed: () {}),
            primary: ActionButton(
              'Allow',
              kind: BtnKind.primary,
              onPressed: () {},
            ),
          ),
        ),
      ),
    );
    final deny = tester.getSize(find.widgetWithText(InkWell, 'Deny')).width;
    final allow = tester.getSize(find.widgetWithText(InkWell, 'Allow')).width;
    expect(deny, closeTo(allow, .5));
  });
}
