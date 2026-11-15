import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/pets/pet_view.dart';
import 'package:phone/theme/tokens.dart';
import 'package:phone/widgets/glass.dart';
import 'package:phone/widgets/session_row.dart';

import 'app_shell_test.dart' show app, tallScreen;

/// The colours of the halo layers right now.
List<double> haloColors(WidgetTester tester) => [
  for (final w in tester.widgetList<ColorFiltered>(
    find.descendant(
      of: find.byType(PetView),
      matching: find.byType(ColorFiltered),
    ),
  ))
    // The filter's colour is the only thing a halo layer carries.
    _colorOf(w),
];

// The filter prints as ColorFilter.mode(Color(alpha: 0.35, red: ..), BlendMode.srcIn); its alpha is what breathes.
double _colorOf(ColorFiltered w) => double.parse(
  RegExp(r'alpha: ([0-9.]+)').firstMatch(w.colorFilter.toString())!.group(1)!,
);

void main() {
  for (final b in Brightness.values) {
    group('design v2 (${b.name})', () {
      setUp(() {});

      testWidgets('Start is solid accent with the on-accent icon', (
        tester,
      ) async {
        tester.platformDispatcher.platformBrightnessTestValue = b;
        addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);
        await tester.pumpWidget(app());
        await tester.pump();
        final tokens = Theme.of(tester.element(find.byType(Scaffold).first))
            .extension<BobixTokens>()!;
        final start = find.bySemanticsLabel('Start');
        final box = tester.widget<DecoratedBox>(
          find.descendant(of: start, matching: find.byType(DecoratedBox)).first,
        );
        final d = box.decoration as BoxDecoration;
        expect(d.color, tokens.accentSolid);
        expect(
          tester
              .widgetList<DecoratedBox>(
                find.descendant(of: start, matching: find.byType(DecoratedBox)),
              )
              .any((b) => (b.decoration as BoxDecoration).gradient != null),
          isTrue,
          reason: 'the glass sheen',
        );
        expect(d.boxShadow, isNotEmpty);
        final icon = tester.widget<Icon>(
          find.descendant(of: start, matching: find.byType(Icon)),
        );
        expect(icon.color, tokens.onAccent);
        // Light icon on orange in light mode, dark icon in dark mode.
        expect(icon.color!.computeLuminance() > .5, b == Brightness.light);
      });

      testWidgets(
        'a Needs-you card has the warm tint and no accent outline; its pet has the halo',
        (tester) async {
          tallScreen(tester);
          tester.platformDispatcher.platformBrightnessTestValue = b;
          addTearDown(
            tester.platformDispatcher.clearPlatformBrightnessTestValue,
          );
          await tester.pumpWidget(app());
          await tester.pump();
          final rows = tester.widgetList<SessionRow>(find.byType(SessionRow));
          final need = rows.where((r) => r.needsYou).toList();
          expect(need, isNotEmpty);
          for (final r in rows) {
            final glass = tester.widget<Glass>(
              find
                  .descendant(
                    of: find.byWidget(r),
                    matching: find.byType(Glass),
                  )
                  .first,
            );
            expect(glass.border, isNull, reason: 'no outline on any card');
            expect(glass.tint != null, r.needsYou, reason: 'warm tint');
            final pet = tester.widget<PetView>(
              find
                  .descendant(
                    of: find.byWidget(r),
                    matching: find.byType(PetView),
                  )
                  .first,
            );
            expect(pet.halo, r.needsYou);
          }
        },
      );
    });
  }

  testWidgets(
    'the halo breathes over 3.2 s, and is still under reduced motion',
    (tester) async {
      tallScreen(tester);
      await tester.pumpWidget(app());
      await tester.pump();
      final rest = haloColors(tester);
      expect(rest, isNotEmpty);
      await tester.pump(PetView.haloCycle ~/ 2);
      final full = haloColors(tester);
      expect(full, isNot(rest), reason: 'the halo changed over half a cycle');
      await tester.pump(PetView.haloCycle ~/ 2);
      expect(haloColors(tester), rest, reason: 'back where it started');

      tester.platformDispatcher.accessibilityFeaturesTestValue =
          const FakeAccessibilityFeatures(disableAnimations: true);
      addTearDown(
        tester.platformDispatcher.clearAccessibilityFeaturesTestValue,
      );
      await tester.pumpWidget(app());
      await tester.pump();
      final still = haloColors(tester);
      await tester.pump(const Duration(seconds: 2));
      expect(haloColors(tester), still);
    },
  );
}
