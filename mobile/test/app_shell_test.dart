import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/app.dart';
import 'package:phone/data/models.dart';
import 'package:phone/data/phone_data.dart';

import 'support/fixture_phone_data.dart';

import 'package:phone/l10n/l10n.dart';
import 'package:phone/l10n/strings.g.dart';

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
        phoneDataProvider.overrideWithValue(FixturePhoneData(connection: conn)),
      ],
      child: const BobixApp(home: AppShell()),
    );

/// A tall phone so a lazy list builds every row.
void tallScreen(WidgetTester tester) {
  tester.view.physicalSize = const Size(400, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

/// Frames enough for a route to slide in and its providers to answer. (Pets keep animating, so nothing "settles".)
Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 4; i++) {
    await tester.pump(const Duration(milliseconds: 300));
  }
}

void main() {
  testWidgets('Home shows the made-up sessions grouped, and the tabs switch', (
    tester,
  ) async {
    tallScreen(tester);
    await tester.pumpWidget(app());
    await tester.pump();
    expect(find.text('Needs you'), findsOneWidget);
    expect(find.text('Add a dark mode switch'), findsOneWidget);
    expect(find.text('Bash wants to run'), findsNWidgets(2));
    expect(find.text('Answer it at the computer'), findsOneWidget);
    expect(find.text('Pull request #57 waits for a review'), findsOneWidget);
    expect(find.text('Studio Mac'), findsOneWidget);

    await tester.tap(find.bySemanticsLabel('Settings'));
    await tester.pump();
    expect(find.text('Safety'), findsOneWidget);
    expect(find.text('Send feedback'), findsOneWidget);
  });

  testWidgets('Vietnamese strings are used when the language is vi', (
    tester,
  ) async {
    await tester.pumpWidget(app(lang: 'vi'));
    await tester.pump();
    expect(find.text('Cần bạn'), findsOneWidget);
    expect(find.text('Bash muốn chạy'), findsNWidgets(2));
    expect(find.text('Hãy trả lời tại máy tính'), findsOneWidget);
  });

  testWidgets(
    'each connection state shows its banner and the "last data" line',
    (tester) async {
      const titles = {
        ConnState.noNetwork: 'No network on this phone',
        ConnState.relayDown: 'Can’t reach the Bobix relay',
        ConnState.asleep: 'Studio Mac is asleep',
        ConnState.offline: 'Studio Mac is offline',
      };
      tallScreen(tester);
      for (final e in titles.entries) {
        await tester.pumpWidget(app(conn: e.key));
        await tester.pump();
        expect(find.text(e.value), findsOneWidget, reason: e.key.name);
        expect(
          find.text('Showing what the computer sent at 21:02.'),
          findsOneWidget,
        );
      }
      await tester.pumpWidget(app());
      await tester.pump();
      expect(
        find.text('Showing what the computer sent at 21:02.'),
        findsNothing,
      );
    },
  );

  testWidgets('Pets lists every session', (tester) async {
    tallScreen(tester);
    await tester.pumpWidget(app());
    await tester.pump();
    await tester.tap(find.bySemanticsLabel('Pets'));
    await tester.pump();
    expect(
      find.text('Tap a pet. The ones with a coral ring need you.'),
      findsOneWidget,
    );
    expect(find.text('Update the sign-up email'), findsOneWidget);
  });

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
    await tester.pump();
    for (final label in ['Home', 'Pets', 'Settings', 'Start']) {
      final size = tester.getSize(find.bySemanticsLabel(label));
      expect(
        size.width >= 44 && size.height >= 44,
        isTrue,
        reason: '$label ${size.width}x${size.height}',
      );
    }
  });

  testWidgets(
    'the largest system font size does not overflow Home, in both languages',
    (tester) async {
      tester.platformDispatcher.textScaleFactorTestValue = 2.0;
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      tallScreen(tester);
      for (final lang in ['en', 'vi']) {
        await tester.pumpWidget(app(lang: lang, conn: ConnState.asleep));
        await tester.pump();
        expect(tester.takeException(), isNull, reason: 'home $lang');
      }
    },
  );

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
      await settle(tester);
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

  testWidgets('the computer chip on Home opens the computers sheet', (
    tester,
  ) async {
    await tester.pumpWidget(app());
    await tester.pump();
    await tester.tap(find.text('Studio Mac'));
    await settle(tester);
    expect(find.text('Choose a computer'), findsOneWidget);
    expect(find.text('Only one computer is connected.'), findsOneWidget);
    await tester.tap(find.text('Close'));
    await settle(tester);
    expect(find.text('Choose a computer'), findsNothing);
  });
}
