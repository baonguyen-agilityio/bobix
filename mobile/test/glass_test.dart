import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/theme/theme.dart';
import 'package:phone/widgets/glass.dart';

Widget host(Widget child) => MaterialApp(
  theme: bobixTheme(Brightness.light),
  home: Scaffold(body: Center(child: child)),
);

void main() {
  tearDown(() => Glass.blurEnabled = true);

  testWidgets('a surface that floats over content blurs what is behind it', (
    tester,
  ) async {
    await tester.pumpWidget(
      host(const Glass(blur: true, child: SizedBox(width: 80, height: 40))),
    );
    final filter = tester.widget<BackdropFilter>(find.byType(BackdropFilter));
    expect(filter.filter, ImageFilter.blur(sigmaX: 20, sigmaY: 20));
    expect(
      find.descendant(
        of: find.byType(ClipRRect),
        matching: find.byType(BackdropFilter),
      ),
      findsOneWidget,
    );
  });

  testWidgets('every other surface is flat: no blur layer', (tester) async {
    await tester.pumpWidget(
      host(const Glass(child: SizedBox(width: 80, height: 40))),
    );
    expect(find.byType(BackdropFilter), findsNothing);
  });

  testWidgets('one switch turns every blur off', (tester) async {
    Glass.blurEnabled = false;
    await tester.pumpWidget(
      host(const Glass(blur: true, child: SizedBox(width: 80, height: 40))),
    );
    expect(find.byType(BackdropFilter), findsNothing);
    expect(find.byType(Glass), findsOneWidget);
  });
}
