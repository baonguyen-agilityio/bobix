import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/lock/app_lock.dart';
import 'package:phone/pets/creature.dart';
import 'package:phone/pets/pet_view.dart';
import 'package:phone/pets/random_pet.dart';
import 'package:phone/theme/theme.dart';

void main() {
  Widget host(Widget child) => MaterialApp(
    theme: bobixTheme(Brightness.light),
    home: Scaffold(body: child),
  );

  testWidgets('a random pet is any design and colour, at work', (tester) async {
    final seen = <String>{};
    for (var seed = 0; seed < 12; seed++) {
      await tester.pumpWidget(
        host(RandomPet(key: UniqueKey(), random: Random(seed))),
      );
      final pet = tester.widget<PetView>(find.byType(PetView));
      expect(designs, contains(pet.design));
      expect(pet.color, inInclusiveRange(0, petColorCount - 1));
      expect(pet.state, 'working');
      seen.add('${pet.design}/${pet.color}');
    }
    expect(seen.length, greaterThan(3), reason: 'not the same pet every time');
  });

  testWidgets('the splash writes its name in the app font, not the fallback', (
    tester,
  ) async {
    // No Scaffold above it, as in the gate while the app starts.
    await tester.pumpWidget(
      MaterialApp(
        theme: bobixTheme(Brightness.light),
        home: const PetsSplash(),
      ),
    );
    final style = DefaultTextStyle.of(tester.element(find.text('Bobix'))).style;
    final text = tester.widget<Text>(find.text('Bobix'));
    final resolved = style.merge(text.style);
    expect(resolved.debugLabel ?? '', isNot(contains('fallback')));
    expect(resolved.fontFamily, isNot('monospace'));
  });

  testWidgets('the splash and the cover show pets and no data', (tester) async {
    await tester.pumpWidget(host(const PetsSplash()));
    expect(find.byType(PetView), findsNWidgets(3));
    expect(find.text('Bobix'), findsOneWidget);
    await tester.pumpWidget(host(const CoverScreen()));
    expect(find.byType(PetView), findsOneWidget);
  });
}
