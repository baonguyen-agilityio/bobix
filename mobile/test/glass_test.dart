import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/theme/theme.dart';
import 'package:phone/widgets/floating_sheet.dart';
import 'package:phone/widgets/glass.dart';
import 'package:phone/widgets/glass_tab_bar.dart';

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

  testWidgets(
    'the tab bar pill is inset 3 pt on top, bottom and the outer side',
    (tester) async {
      await tester.pumpWidget(
        host(
          GlassTabBar(
            tabs: const [
              TabSpec(Icons.home, 'Home'),
              TabSpec(Icons.pets, 'Pets'),
              TabSpec(Icons.settings, 'Settings'),
            ],
            start: const TabSpec(Icons.add, 'Start'),
            index: 0,
            startIndex: 3,
            onSelect: (_) {},
          ),
        ),
      );
      final capsule = tester.getRect(find.byKey(const ValueKey('tab-capsule')));
      final first = tester.getRect(find.byKey(const ValueKey('tab-pill-0')));
      final last = tester.getRect(find.byKey(const ValueKey('tab-pill-2')));
      expect(first.top - capsule.top, 3);
      expect(capsule.bottom - first.bottom, 3);
      expect(first.left - capsule.left, 3);
      expect(capsule.right - last.right, 3);
      expect(last.top - capsule.top, 3);
      expect(capsule.bottom - last.bottom, 3);
      for (final label in ['Home', 'Pets', 'Settings']) {
        final size = tester.getSize(find.bySemanticsLabel(label));
        expect(size.height >= 44 && size.width >= 44, isTrue, reason: label);
      }
    },
  );

  testWidgets(
    'a sheet floats 8 pt above the screen edge and pads for the home indicator',
    (tester) async {
      tester.view
        ..physicalSize = const Size(400, 800)
        ..devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        MaterialApp(
          theme: bobixTheme(Brightness.light),
          builder: (context, child) => MediaQuery(
            data: MediaQuery.of(context).copyWith(
              padding: const EdgeInsets.only(top: 47, bottom: 34),
              viewPadding: const EdgeInsets.only(top: 47, bottom: 34),
            ),
            child: child!,
          ),
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () => showModalBottomSheet<void>(
                context: context,
                isScrollControlled: true,
                backgroundColor: Colors.transparent,
                builder: (_) => const FloatingSheet(
                  children: [SizedBox(key: ValueKey('last'), height: 40)],
                ),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      final sheet = tester.getRect(find.byType(Glass));
      expect(800 - sheet.bottom, 8);
      expect(sheet.left, 8);
      expect(400 - sheet.right, 8);
      final last = tester.getRect(find.byKey(const ValueKey('last')));
      expect(sheet.bottom - last.bottom, 34 - 8);
      expect(find.byType(BackdropFilter), findsOneWidget);
    },
  );
}
