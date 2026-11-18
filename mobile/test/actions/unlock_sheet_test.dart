// The confirm sheet: shown fully before the biometric is asked, then into the result; failures stay on the sheet.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/actions_source.dart';
import 'package:phone/features/actions/request_screen.dart';

import 'support.dart';

/// Answers each ask with the next result of [script] (the last one repeats) and counts the asks.
class _ScriptedGate implements BiometricGate {
  _ScriptedGate(this.script);
  final List<GateResult> script;
  int asks = 0;

  @override
  Future<GateResult> confirm(String reason, Command command) async =>
      script[asks++ < script.length ? asks - 1 : script.length - 1];
}

/// Lets [n] ms go by in frames of 50 ms (a long frame would make the animation start late).
Future<void> ms(WidgetTester tester, int n) async {
  for (var left = n; left > 0; left -= 50) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

void main() {
  Future<_ScriptedGate> openSheet(
    WidgetTester tester,
    List<GateResult> script,
  ) async {
    final gate = _ScriptedGate(script);
    await pumpScreen(tester, const RequestScreen(id: 'req-bash'), gate: gate);
    await tester.tap(button('Allow'));
    await tester.pump();
    return gate;
  }

  testWidgets(
    'the biometric is asked only after the sheet is fully shown and the reason could be read',
    (tester) async {
      final gate = await openSheet(tester, [GateResult.confirmed]);
      await ms(tester, 300);
      expect(find.text('Confirm it’s you'), findsOneWidget);
      expect(gate.asks, 0, reason: 'still rising');
      await ms(tester, 200);
      expect(gate.asks, 0, reason: 'shown, but the reason is being read');
      await ms(tester, 150);
      expect(gate.asks, 1);
      await settle(tester);
      expect(find.text(tr('en', 'res.ok.allow.t')), findsOneWidget);
    },
  );

  testWidgets(
    'a check that did not work says why, shakes, and Try again asks again',
    (tester) async {
      final gate = await openSheet(tester, [
        GateResult.failed,
        GateResult.confirmed,
      ]);
      await ms(tester, 700);
      expect(gate.asks, 1);
      expect(find.textContaining('did not recognise you'), findsOneWidget);
      expect(find.text('Confirm it’s you'), findsOneWidget);
      await tester.tap(button('Try again'));
      await settle(tester);
      expect(gate.asks, 2);
      expect(find.text(tr('en', 'res.ok.allow.t')), findsOneWidget);
    },
  );

  testWidgets(
    'locked out: the reason is shown without Try again; Cancel sends nothing',
    (tester) async {
      final gate = await openSheet(tester, [GateResult.lockedOut]);
      await ms(tester, 700);
      expect(find.textContaining('Too many tries'), findsOneWidget);
      expect(button('Try again'), findsNothing);
      await tester.tap(button('Cancel'));
      await settle(tester);
      expect(gate.asks, 1);
      expect(find.text('Confirm it’s you'), findsNothing);
      expect(find.text(tr('en', 'un.cancelled')), findsOneWidget);
    },
  );

  for (final reduced in [false, true]) {
    testWidgets(
      'the circle ${reduced ? 'does not shake' : 'shakes'} on a failed check'
      '${reduced ? ' with reduced motion' : ''}',
      (tester) async {
        if (reduced) {
          tester.platformDispatcher.accessibilityFeaturesTestValue =
              const FakeAccessibilityFeatures(disableAnimations: true);
          addTearDown(
            tester.platformDispatcher.clearAccessibilityFeaturesTestValue,
          );
        }
        await openSheet(tester, [GateResult.failed]);
        await ms(tester, 500);
        final glyph = find.byIcon(Icons.fingerprint);
        final rest = tester.getCenter(glyph);
        // The check fails at about 600 ms; the circle is in the middle of its shake 150 ms later.
        await ms(tester, 250);
        final during = tester.getCenter(glyph).dx;
        if (reduced) {
          expect(during, rest.dx);
        } else {
          expect(during, isNot(rest.dx));
        }
      },
    );
  }
}
