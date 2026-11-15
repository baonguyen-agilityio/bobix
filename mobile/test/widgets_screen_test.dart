// The Widgets previews: the hub and its three preview screens. Everything here is made up.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/data/models.dart';
import 'package:phone/data/phone_data.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/screens/settings_screen.dart' show WidgetsScreen;
import 'package:phone/screens/widgets/control_center.dart';
import 'package:phone/screens/widgets/home_screen_widgets.dart';
import 'package:phone/screens/widgets/lock_widgets.dart';
import 'package:phone/theme/theme.dart';

class _Lang extends Lang {
  _Lang(this.value);
  final String value;
  @override
  String build() => value;
}

class _Data implements PhoneData {
  const _Data(this.home);
  final HomeData? home;
  @override
  Stream<HomeData> watchHome() =>
      home == null ? const Stream.empty() : Stream.value(home!);
}

const _sessions = [
  Session(
    id: 's1',
    name: 'Add coupon field to checkout',
    project: 'shop-app',
    stage: Stage.doing,
    needsYou: true,
    waitingMinutes: 2,
    tool: 'Bash',
  ),
  Session(
    id: 's2',
    name: 'Rotate staging certificates',
    project: 'infra',
    stage: Stage.doing,
    needsYou: true,
    waitingKind: WaitingKind.plan,
    waitingMinutes: 6,
  ),
  Session(
    id: 's3',
    name: 'Add gift card balance page',
    project: 'shop-app',
    issue: 151,
    stage: Stage.todo,
  ),
];

const _home = HomeData(
  computer: 'Studio Mac (demo)',
  conn: ConnState.ok,
  lastContact: '21:02',
  sessions: _sessions,
);

Future<void> pump(
  WidgetTester tester, {
  Widget screen = const WidgetsScreen(),
  HomeData? home = _home,
  String lang = 'en',
  Brightness brightness = Brightness.light,
}) async {
  tester.view.physicalSize = const Size(440, 4000);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      key: UniqueKey(),
      overrides: [
        langProvider.overrideWith(() => _Lang(lang)),
        phoneDataProvider.overrideWithValue(_Data(home)),
      ],
      child: MaterialApp(theme: bobixTheme(brightness), home: screen),
    ),
  );
  // The pets animate for ever, so frames are pumped, never settled.
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 100));
}

Future<void> open(WidgetTester tester, String row) async {
  await tester.tap(find.text(row).first);
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 400));
}

/// Finds [what] on the preview screen [screen]: the hub below it stays built.
Finder on(Type screen, Finder what) =>
    find.descendant(of: find.byType(screen), matching: what);

void main() {
  testWidgets('the hub: three rows, the state control and the previews', (
    tester,
  ) async {
    await pump(tester);
    expect(find.text('Widgets'), findsOneWidget);
    for (final t in ['Home Screen', 'Lock Screen', 'Control Center']) {
      expect(find.text(t), findsWidgets);
    }
    expect(
      find.text('Small, medium and large widgets in place'),
      findsOneWidget,
    );
    expect(find.text('Preview state'), findsOneWidget);
    for (final t in ['Normal', 'Nothing', 'Offline']) {
      expect(find.text(t), findsOneWidget);
    }
    expect(find.text('Home Screen widgets'), findsOneWidget);
    expect(find.byType(SmallHomeWidget), findsOneWidget);
    expect(find.byType(MediumHomeWidget), findsOneWidget);
    expect(find.byType(LargeHomeWidget), findsOneWidget);
    expect(find.byType(LiveActivityCard), findsOneWidget);
    expect(find.byType(IslandPreview), findsNWidgets(3));
    // What the app has, drawn: the count, the first session and its ticket.
    expect(find.text('2'), findsWidgets);
    expect(find.text('Add coupon field to checkout'), findsWidgets);
    expect(find.text('shop-app #151'), findsOneWidget);
  });

  testWidgets('the rows are 44 pt or more and the state control is a group', (
    tester,
  ) async {
    await pump(tester);
    expect(
      tester
          .getSize(
            find
                .ancestor(
                  of: find.text('Lock Screen'),
                  matching: find.byType(InkWell),
                )
                .first,
          )
          .height,
      greaterThanOrEqualTo(44),
    );
    expect(
      tester
          .getSize(
            find
                .ancestor(
                  of: find.text('Nothing'),
                  matching: find.byType(InkWell),
                )
                .first,
          )
          .height,
      greaterThanOrEqualTo(44),
    );
    final handle = tester.ensureSemantics();
    expect(
      tester.getSemantics(find.text('Nothing')),
      isSemantics(isButton: true, isSelected: false),
    );
    expect(
      tester.getSemantics(find.text('Normal')),
      isSemantics(isButton: true, isSelected: true),
    );
    handle.dispose();
  });

  testWidgets('the Home Screen row opens its preview, and back returns', (
    tester,
  ) async {
    await pump(tester);
    await open(tester, 'Home Screen');
    expect(find.byType(HomeScreenPreviewScreen), findsOneWidget);
    expect(
      on(HomeScreenPreviewScreen, find.byType(SmallHomeWidget)),
      findsOneWidget,
    );
    expect(
      on(HomeScreenPreviewScreen, find.byType(LargeHomeWidget)),
      findsOneWidget,
    );
    await tester.tap(
      on(HomeScreenPreviewScreen, find.byIcon(Icons.chevron_left)),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(WidgetsScreen), findsOneWidget);
  });

  testWidgets(
    'the Lock Screen row opens widgets, the activity and the island',
    (tester) async {
      await pump(tester);
      await open(tester, 'Lock Screen');
      expect(find.byType(LockWidgetsScreen), findsOneWidget);
      expect(
        on(LockWidgetsScreen, find.byType(LockCircleWidget)),
        findsOneWidget,
      );
      expect(
        on(LockWidgetsScreen, find.byType(LockRectWidget)),
        findsOneWidget,
      );
      expect(
        on(LockWidgetsScreen, find.byType(LiveActivityCard)),
        findsOneWidget,
      );
      expect(
        on(LockWidgetsScreen, find.text('Tap the island to expand it')),
        findsOneWidget,
      );
      expect(
        on(LockWidgetsScreen, find.text('Open to review')),
        findsOneWidget,
      );
      // A tap on the compact island expands it: the button shows twice.
      await tester.tap(on(LockWidgetsScreen, find.byType(IslandPreview)));
      await tester.pump();
      expect(
        on(LockWidgetsScreen, find.text('Open to review')),
        findsNWidgets(2),
      );
    },
  );

  testWidgets('the Lock Screen preview shows the design clock and date', (
    tester,
  ) async {
    await pump(tester);
    await open(tester, 'Lock Screen');
    expect(on(LockWidgetsScreen, find.text('21:14')), findsOneWidget);
    expect(on(LockWidgetsScreen, find.text('Friday 13 November ·')), findsOne);
  });

  testWidgets('the Lock Screen preview says the date in Vietnamese', (
    tester,
  ) async {
    await pump(tester, lang: 'vi');
    await open(tester, 'Màn khoá');
    expect(on(LockWidgetsScreen, find.text('21:14')), findsOneWidget);
    expect(on(LockWidgetsScreen, find.text('Thứ Sáu 13 tháng 11 ·')), findsOne);
  });

  testWidgets('the Home Screen preview has placeholder apps and a dock', (
    tester,
  ) async {
    await pump(tester);
    await open(tester, 'Home Screen');
    for (final n in ['Mail', 'Maps', 'Notes', 'Music']) {
      expect(on(HomeScreenPreviewScreen, find.text(n)), findsOneWidget);
    }
    expect(
      on(HomeScreenPreviewScreen, find.bySemanticsLabel('Bobix')),
      findsWidgets,
    );
  });

  testWidgets('the Control Center row opens the Bobix and Start controls', (
    tester,
  ) async {
    await pump(tester);
    await open(tester, 'Control Center');
    expect(find.byType(ControlCenterScreen), findsOneWidget);
    expect(on(ControlCenterScreen, find.byType(BobixControl)), findsOneWidget);
    expect(on(ControlCenterScreen, find.text('Start')), findsOneWidget);
    expect(on(ControlCenterScreen, find.text('1 tickets')), findsOneWidget);
    expect(
      on(
        ControlCenterScreen,
        find.text('Bobix controls: waiting requests · start a ticket'),
      ),
      findsOneWidget,
    );
  });

  testWidgets('Nothing: no request is drawn and the widgets say so', (
    tester,
  ) async {
    await pump(tester);
    expect(find.text('Add coupon field to checkout'), findsWidgets);
    await tester.tap(find.text('Nothing'));
    await tester.pump();
    expect(find.text('Nothing needs you'), findsWidgets);
    // Only the ticket's title remains of the made-up sessions that wait.
    expect(find.text('Rotate staging certificates'), findsNothing);
  });

  testWidgets('Offline: the widgets say when they were last up to date', (
    tester,
  ) async {
    await pump(tester);
    expect(find.text('Updated 21:02'), findsNothing);
    await tester.tap(find.text('Offline'));
    await tester.pump();
    expect(find.text('Updated 21:02'), findsOneWidget, reason: 'large widget');
    expect(find.text('21:02'), findsNWidgets(2), reason: 'small and medium');
  });

  testWidgets('no data at all: every preview draws its empty look', (
    tester,
  ) async {
    await pump(tester, home: null);
    expect(find.text('Nothing needs you'), findsWidgets);
    expect(find.byType(SmallHomeWidget), findsOneWidget);
    await tester.tap(find.text('Offline'));
    await tester.pump();
    expect(find.text('Computer offline'), findsWidgets);
  });

  testWidgets('Vietnamese titles and rows, and dark mode', (tester) async {
    await pump(tester, lang: 'vi', brightness: Brightness.dark);
    expect(find.text('Widget'), findsOneWidget);
    expect(find.text('Màn hình chính'), findsWidgets);
    expect(find.text('Màn khoá'), findsWidgets);
    expect(find.text('Trạng thái xem thử'), findsOneWidget);
    expect(find.text('Không có việc'), findsOneWidget);
    await open(tester, 'Control Center');
    expect(on(ControlCenterScreen, find.text('Bắt đầu')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
