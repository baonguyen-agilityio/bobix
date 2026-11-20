import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/app.dart';
import 'package:phone/data/models.dart';
import 'package:phone/data/phone_data.dart';
import 'package:phone/features/actions/actions_source.dart';
import 'package:phone/log/activity_log.dart';
import 'package:phone/pets/pet_view.dart';
import 'package:phone/screens/task_log_screen.dart';
import 'package:phone/widgets/glass_tab_bar.dart';
import 'package:phone/widgets/session_row.dart';

import 'support/actions/fixture_actions_source.dart';
import 'support/fixture_phone_data.dart';

import 'package:phone/l10n/l10n.dart';
import 'package:phone/l10n/strings.g.dart';

class _Lang extends Lang {
  _Lang(this.value);
  final String value;
  @override
  String build() => value;
}

/// Home data with exactly these sessions.
class _Sessions implements PhoneData {
  const _Sessions(this.sessions);
  final List<Session> sessions;

  @override
  Stream<HomeData> watchHome() => Stream.value(
    HomeData(
      computer: 'Studio Mac',
      conn: ConnState.ok,
      lastContact: '21:14',
      sessions: sessions,
    ),
  );
}

const _asks = <Session>[
  Session(
    id: 'a1',
    name: 'Tidy the cache',
    project: 'shop',
    stage: Stage.doing,
    needsYou: true,
    waitingId: 'r1',
    waitingKind: WaitingKind.request,
    tool: 'Bash',
  ),
  Session(
    id: 'a2',
    name: 'Rotate the certificates',
    project: 'infra',
    stage: Stage.doing,
    needsYou: true,
    waitingId: 'p1',
    waitingKind: WaitingKind.plan,
  ),
  Session(
    id: 'a3',
    name: 'Add the onboarding screens',
    project: 'app',
    stage: Stage.doing,
    needsYou: true,
    waitingId: 'q1',
    waitingKind: WaitingKind.question,
  ),
];

Widget app({
  String lang = 'en',
  ConnState conn = ConnState.ok,
  PhoneData? data,
  ActionsSource? source,
}) => ProviderScope(
  key: UniqueKey(),
  overrides: [
    langProvider.overrideWith(() => _Lang(lang)),
    phoneDataProvider.overrideWithValue(
      data ?? FixturePhoneData(connection: conn),
    ),
    if (source != null) actionsSourceProvider.overrideWithValue(source),
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

/// Whether the radio card whose label starts with [label] is picked: read from the `Semantics` it declares, which is
/// the same on every Flutter version (the semantics matchers and flags are not).
bool checkedOf(WidgetTester tester, String label) => tester
    .widget<Semantics>(
      find
          .byWidgetPredicate(
            (w) =>
                w is Semantics &&
                (w.properties.label?.startsWith(label) ?? false) &&
                w.properties.checked != null,
          )
          .first,
    )
    .properties
    .checked!;

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

  testWidgets('a tab that is not shown stops its animations', (tester) async {
    tallScreen(tester);
    await tester.pumpWidget(app());
    await tester.pump();
    final onHome = PetView.stepping;
    expect(onHome, greaterThan(3), reason: 'the pets of Home are running');
    await tester.tap(find.bySemanticsLabel('Settings'));
    await tester.pump(const Duration(milliseconds: 400));
    // Settings has no pet: nothing may keep ticking for the tabs behind it.
    final onSettings = PetView.stepping;
    expect(onSettings, lessThan(onHome ~/ 2));
  });

  testWidgets('pets stop stepping while the app is not in front, and go on when it is', (tester) async {
    tallScreen(tester);
    await tester.pumpWidget(app());
    await tester.pump();
    final running = PetView.stepping;
    expect(running, greaterThan(0));
    for (final st in [AppLifecycleState.inactive, AppLifecycleState.hidden, AppLifecycleState.paused]) {
      tester.binding.handleAppLifecycleStateChanged(st);
    }
    await tester.pump();
    expect(PetView.stepping, 0);
    for (final st in [AppLifecycleState.hidden, AppLifecycleState.inactive, AppLifecycleState.resumed]) {
      tester.binding.handleAppLifecycleStateChanged(st);
    }
    await tester.pump();
    expect(PetView.stepping, running);
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
      find.text('Tap a pet. The ones with an orange “!” need you.'),
      findsOneWidget,
    );
    expect(find.text('Update the sign-up email'), findsOneWidget);
  });

  testWidgets('Home says what each waiting item asks for by its kind', (
    tester,
  ) async {
    tallScreen(tester);
    await tester.pumpWidget(app(data: const _Sessions(_asks)));
    await tester.pump();
    expect(find.text('Bash wants to run'), findsOneWidget);
    expect(find.text('Plan ready to approve'), findsOneWidget);
    expect(find.text('Claude has a question'), findsOneWidget);
  });

  testWidgets(
    'a session asked at the computer is not counted as needing the phone, and says why when opened',
    (tester) async {
      tallScreen(tester);
      await tester.pumpWidget(
        app(
          data: const _Sessions([
            Session(
              id: 'ses-at-computer',
              name: 'Rename the settings page',
              project: 'shop',
              stage: Stage.doing,
              needsYou: true,
              answerAtComputer: true,
              atComputerWhy: 'tooLarge',
            ),
          ]),
          source: FixtureActionsSource(),
        ),
      );
      await tester.pump();
      expect(find.text('Needs you'), findsNothing);
      expect(find.text('Nothing needs you'), findsOneWidget);
      expect(find.text('At the computer'), findsOneWidget);
      expect(find.text('Answer it at the computer'), findsOneWidget);
      expect(
        find.descendant(of: find.byType(GlassTabBar), matching: find.text('1')),
        findsNothing,
        reason: 'no count on the Home tab',
      );
      await tester.tap(find.text('Rename the settings page'));
      await tester.pump(const Duration(seconds: 1));
      await tester.pump(const Duration(seconds: 1));
      expect(find.textContaining('Answer it there.'), findsOneWidget);
      expect(
        find.textContaining('The question is too large for the phone.'),
        findsOneWidget,
      );
    },
  );

  testWidgets('a task card ends with its log count, which opens that log', (
    tester,
  ) async {
    tallScreen(tester);
    await tester.pumpWidget(app(source: FixtureActionsSource()));
    await tester.pump();
    final container = ProviderScope.containerOf(
      tester.element(find.byType(AppShell)),
    );
    container
        .read(activityLogProvider.notifier)
        .record(
          LogEntry(at: DateTime(2026), kind: LogKind.allow, sessionId: 's1'),
        );
    await tester.pump();
    final pill = find.bySemanticsLabel('Log of Add a dark mode switch');
    expect(find.descendant(of: pill, matching: find.text('1')), findsOneWidget);
    expect(
      find.descendant(
        of: find.byType(LogPill).first,
        matching: find.byIcon(Icons.format_list_bulleted),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byType(SessionRow).first,
        matching: find.byIcon(Icons.chevron_right),
      ),
      findsOneWidget,
    );
    final size = tester.getSize(find.byType(LogPill).first);
    expect(size.width >= 44 && size.height >= 44, isTrue);
    await tester.tap(find.byType(LogPill).first);
    await settle(tester);
    expect(find.byType(TaskLogScreen), findsOneWidget);
    expect(find.text('Task log'), findsOneWidget);
  });

  testWidgets(
    '"To do" lists the issues, and a tap opens Start with it picked',
    (tester) async {
      tallScreen(tester);
      final semantics = tester.ensureSemantics();
      await tester.pumpWidget(app(source: FixtureActionsSource()));
      await tester.pump();
      await tester.pump();
      expect(find.text('To do'), findsOneWidget);
      final tickets = find.byIcon(Icons.confirmation_number_outlined);
      expect(tickets, findsNWidgets(4));
      await tester.tap(tickets.at(1));
      await tester.pump();
      expect(find.text('Start Full'), findsOneWidget);
      expect(checkedOf(tester, 'Update the sign-up email, #35'), isTrue);
      semantics.dispose();
    },
  );

  testWidgets('Pets says per card what the pet is doing', (tester) async {
    tallScreen(tester);
    await tester.pumpWidget(
      app(
        data: _Sessions([
          ..._asks,
          ...const [
            Session(
              id: 'd1',
              name: 'Fix the cart',
              project: 'shop',
              stage: Stage.doing,
              tool: 'Edit',
            ),
            Session(
              id: 'd2',
              name: 'Update the docs',
              project: 'blog',
              stage: Stage.done,
            ),
            Session(
              id: 'd3',
              name: 'Move the files',
              project: 'wiki',
              stage: Stage.stuck,
            ),
          ],
        ]),
      ),
    );
    await tester.pump();
    await tester.tap(find.bySemanticsLabel('Pets'));
    await tester.pump();
    for (final chip in [
      'Needs your approval',
      'Plan ready to approve',
      'Claude has a question',
      'Now Edit',
      'Done!',
      'Finished 3 h ago with no pull request',
    ]) {
      expect(find.text(chip), findsOneWidget, reason: chip);
    }
    // The project is the card's title and the session its subtitle.
    expect(find.text('infra'), findsOneWidget);
    expect(find.text('Rotate the certificates'), findsOneWidget);
  });

  testWidgets(
    'Start picks the first issue and shows Full and Quick as radio cards',
    (tester) async {
      tallScreen(tester);
      final semantics = tester.ensureSemantics();
      await tester.pumpWidget(app(source: FixtureActionsSource()));
      await tester.pump();
      await tester.pump();
      await tester.tap(find.bySemanticsLabel('Start'));
      await tester.pump();
      bool checked(String label) => checkedOf(tester, label);
      expect(checked('Fix the cart total rounding, #38'), isTrue);
      expect(checked('Full, '), isTrue);
      expect(checked('Quick, '), isFalse);
      expect(find.text('Start Full'), findsOneWidget);
      await tester.tap(find.text('Quick'));
      await tester.pump();
      expect(checked('Quick, '), isTrue);
      expect(find.text('Start Quick'), findsOneWidget);
      expect(find.text('Start Full'), findsNothing);
      expect(find.text('What happens on Studio Mac'), findsOneWidget);
      expect(find.textContaining('Asks for'), findsOneWidget);
      semantics.dispose();
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
    'only the floating surfaces blur: the tab bar and the computer chip (Start is solid)',
    (tester) async {
      await tester.pumpWidget(app());
      await tester.pump();
      expect(find.byType(BackdropFilter), findsNWidgets(2));
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
