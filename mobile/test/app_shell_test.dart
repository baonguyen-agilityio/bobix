import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/app.dart';
import 'package:phone/data/fixture_phone_data.dart';
import 'package:phone/data/models.dart';
import 'package:phone/data/phone_data.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/l10n/strings.g.dart';
import 'package:phone/screens/session_screen.dart';
import 'package:phone/theme/theme.dart';

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
      child: const BobixApp(),
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
    expect(
      find.text('Computers, safety and language settings come with pairing.'),
      findsOneWidget,
    );
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

  testWidgets(
    'a request opens from Home: a long command is cut short until it is opened',
    (tester) async {
      tallScreen(tester);
      await tester.pumpWidget(app());
      await tester.pump();
      await tester.tap(find.text('Add a dark mode switch'));
      await settle(tester);
      expect(find.text('Permission request'), findsOneWidget);
      expect(find.text('Bash wants to run'), findsOneWidget);
      expect(find.text('Expires in 8 min'), findsOneWidget);
      expect(
        find.text('Open the whole command to turn on Allow'),
        findsOneWidget,
      );
      expect(
        find.textContaining('Show the whole command (', findRichText: true),
        findsOneWidget,
      );
      expect(find.text('~/work/shop'), findsOneWidget);

      await tester.tap(find.textContaining('Show the whole command ('));
      await tester.pump();
      expect(find.text('Show less'), findsOneWidget);
      // Both answers stay wide enough to read, even though Deny is the smaller one.
      final deny = tester.getSize(
        find
            .ancestor(of: find.text('Deny'), matching: find.byType(Material))
            .first,
      );
      expect(deny.width, greaterThan(100));
      expect(
        find.text('Open the whole command to turn on Allow'),
        findsNothing,
      );

      await tester.tap(find.bySemanticsLabel('Back'));
      await settle(tester);
      expect(find.text('Home'), findsOneWidget);
    },
  );

  testWidgets('a destructive request says so and puts the safe answer first', (
    tester,
  ) async {
    tallScreen(tester);
    await tester.pumpWidget(app());
    await tester.pump();
    await tester.tap(find.text('Clean the build folders'));
    await settle(tester);
    expect(find.text('Destructive command'), findsOneWidget);
    expect(find.text('Allow anyway'), findsOneWidget);
    expect(
      tester.getTopLeft(find.text('Deny')).dy,
      lessThan(tester.getTopLeft(find.text('Allow anyway')).dy),
    );
    expect(find.textContaining('rm -rf', findRichText: true), findsOneWidget);
  });

  testWidgets(
    'a session the computer does not hold for phones says to answer at the computer',
    (tester) async {
      tallScreen(tester);
      await tester.pumpWidget(app());
      await tester.pump();
      await tester.tap(find.text('Rename the settings page'));
      await settle(tester);
      expect(find.text('Session'), findsOneWidget);
      expect(
        find.text('Answer it at the computer'),
        findsNWidgets(2),
      ); // the badge and the banner
      expect(find.textContaining('went back to Claude'), findsOneWidget);
      expect(find.text('Activity'), findsOneWidget);
    },
  );

  testWidgets('a working session shows its last lines, read only', (
    tester,
  ) async {
    tallScreen(tester);
    await tester.pumpWidget(app());
    await tester.pump();
    await tester.tap(find.text('Fix the cart total rounding'));
    await settle(tester);
    expect(find.text('Now Edit'), findsOneWidget);
    expect(
      find.textContaining(
        '21:13  Edit  src/checkout/form.ts',
        findRichText: true,
      ),
      findsOneWidget,
    );
    expect(
      find.text('The last lines Studio Mac sent, at 21:14. Read only.'),
      findsOneWidget,
    );
  });

  testWidgets(
    'a request answered at the computer first is withdrawn: nothing to confirm',
    (tester) async {
      tallScreen(tester);
      await tester.pumpWidget(app());
      await tester.pump();
      expect(find.text('Closed without you'), findsOneWidget);
      await tester.tap(find.text('Add the order export'));
      await settle(tester);
      expect(
        find.text('Answered at the computer'),
        findsNWidgets(2),
      ); // the badge and the banner title
      expect(
        find.textContaining('answered this request first'),
        findsOneWidget,
      );
      expect(find.text('Deny'), findsNothing);
      expect(find.text('Allow'), findsNothing);
      expect(find.text('Back to Home'), findsOneWidget);
      await tester.tap(find.text('Back to Home'));
      await settle(tester);
      expect(find.text('Home'), findsOneWidget);
    },
  );

  testWidgets('in Vietnamese the withdrawn request says so too', (
    tester,
  ) async {
    tallScreen(tester);
    await tester.pumpWidget(app(lang: 'vi'));
    await tester.pump();
    await tester.tap(find.text('Add the order export'));
    await settle(tester);
    expect(find.text('Đã trả lời trên máy tính'), findsNWidgets(2));
    expect(find.text('Về Trang chủ'), findsOneWidget);
  });

  Widget sessionPage(String id) => ProviderScope(
    key: UniqueKey(),
    overrides: [phoneDataProvider.overrideWithValue(const FixturePhoneData())],
    child: MaterialApp(
      theme: bobixTheme(Brightness.light),
      home: SessionScreen(sessionId: id),
    ),
  );

  testWidgets(
    'Stop is offered only to a session that waits on a held request',
    (tester) async {
      tallScreen(tester);
      await tester.pumpWidget(sessionPage('s1')); // waits, request held
      await settle(tester);
      expect(find.text('Stop'), findsOneWidget);
      for (final id in ['s2', 's7', 's8']) {
        // running turn without a request / answered at the computer / not held for phones
        await tester.pumpWidget(sessionPage(id));
        await settle(tester);
        expect(find.text('Stop'), findsNothing, reason: id);
      }
    },
  );

  testWidgets('Pets lists every session and opens it', (tester) async {
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
    await tester.tap(find.text('Update the sign-up email'));
    await settle(tester);
    expect(find.text('Done!'), findsOneWidget);
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
    'the largest system font size does not overflow Home or the detail screens, in both languages',
    (tester) async {
      tester.platformDispatcher.textScaleFactorTestValue = 2.0;
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      tallScreen(tester);
      for (final lang in ['en', 'vi']) {
        await tester.pumpWidget(app(lang: lang, conn: ConnState.asleep));
        await tester.pump();
        expect(tester.takeException(), isNull, reason: 'home $lang');
        for (final name in [
          'Add a dark mode switch',
          'Clean the build folders',
          'Rename the settings page',
        ]) {
          await tester.tap(find.text(name));
          await settle(tester);
          expect(tester.takeException(), isNull, reason: '$name $lang');
          await tester.tap(
            find.bySemanticsLabel(lang == 'en' ? 'Back' : 'Quay lại'),
          );
          await settle(tester);
        }
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
}
