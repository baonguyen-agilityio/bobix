import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/app.dart';
import 'package:phone/fixtures/fixtures.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/l10n/strings.g.dart';
import 'package:phone/screens/home_screen.dart';

class _Conn extends Conn {
  _Conn(this.value);
  final ConnState value;
  @override
  ConnState build() => value;
}

class _Lang extends Lang {
  _Lang(this.value);
  final String value;
  @override
  String build() => value;
}

Widget app({String lang = 'en', ConnState conn = ConnState.ok}) =>
    ProviderScope(
      key: UniqueKey(),
      overrides: [
        langProvider.overrideWith(() => _Lang(lang)),
        connProvider.overrideWith(() => _Conn(conn)),
      ],
      child: const BobixApp(),
    );

void main() {
  testWidgets('Home shows the made-up sessions grouped, and the tabs switch', (
    tester,
  ) async {
    await tester.pumpWidget(app());
    expect(find.text('Needs you'), findsOneWidget);
    expect(find.text('Add a dark mode switch'), findsOneWidget);
    expect(find.text('Bash wants to run'), findsOneWidget);
    expect(find.text('Pull request #57 waits for a review'), findsOneWidget);
    expect(find.text(fixtureComputer), findsOneWidget);

    await tester.tap(find.bySemanticsLabel('Settings'));
    await tester.pump();
    expect(
      find.text('Computers, safety and language settings come with pairing.'),
      findsOneWidget,
    );
  });

  testWidgets('Vietnamese strings are used when the language is vi', (
    tester,
  ) async {
    await tester.pumpWidget(app(lang: 'vi'));
    expect(find.text('Cần bạn'), findsOneWidget);
    expect(find.text('Bash muốn chạy'), findsOneWidget);
  });

  testWidgets(
    'each connection state shows its banner and the "last data" line',
    (tester) async {
      const titles = {
        ConnState.noNetwork: 'No network on this phone',
        ConnState.relayDown: 'Can’t reach the Bobix relay',
        ConnState.asleep: '$fixtureComputer is asleep',
        ConnState.offline: '$fixtureComputer is offline',
      };
      tester.view.physicalSize = const Size(400, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      for (final e in titles.entries) {
        await tester.pumpWidget(app(conn: e.key));
        expect(find.text(e.value), findsOneWidget, reason: e.key.name);
        expect(
          find.text('Showing what the computer sent at 21:02.'),
          findsOneWidget,
        );
      }
      await tester.pumpWidget(app());
      expect(
        find.text('Showing what the computer sent at 21:02.'),
        findsNothing,
      );
    },
  );

  test('every key exists in both languages with the same {names}', () {
    final names = RegExp(r'\{(\w+)\}');
    String vars(String s) =>
        (names.allMatches(s).map((m) => m[1]!).toList()..sort()).join(',');
    final en = phoneStrings['en']!, vi = phoneStrings['vi']!;
    expect(vi.keys.toSet(), en.keys.toSet());
    for (final k in en.keys) {
      expect(vars(vi[k]!), vars(en[k]!), reason: k);
    }
  });

  testWidgets('touch targets are at least 44 points', (tester) async {
    await tester.pumpWidget(app());
    for (final label in ['Home', 'Pets', 'Settings', 'Start']) {
      final size = tester.getSize(find.bySemanticsLabel(label));
      expect(
        size.width >= 44 && size.height >= 44,
        isTrue,
        reason: '$label ${size.width}x${size.height}',
      );
    }
  });

  testWidgets('the largest system font size does not overflow Home', (
    tester,
  ) async {
    tester.platformDispatcher.textScaleFactorTestValue = 2.0;
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    await tester.pumpWidget(app(lang: 'vi', conn: ConnState.asleep));
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'pets animate by frame, and stand still when the system asks for reduced motion',
    (tester) async {
      await tester.pumpWidget(app());
      await tester.pump(const Duration(milliseconds: 400));
      expect(
        tester.binding.hasScheduledFrame,
        isTrue,
        reason: 'a pet is animating',
      );

      tester.platformDispatcher.accessibilityFeaturesTestValue =
          const FakeAccessibilityFeatures(disableAnimations: true);
      addTearDown(
        tester.platformDispatcher.clearAccessibilityFeaturesTestValue,
      );
      await tester.pumpWidget(app());
      await tester.pumpAndSettle();
    },
  );

  testWidgets(
    'only the floating surfaces blur: the tab bar, Start and the computer chip',
    (tester) async {
      await tester.pumpWidget(app());
      await tester.pump();
      expect(find.byType(BackdropFilter), findsNWidgets(3));
    },
  );
}
