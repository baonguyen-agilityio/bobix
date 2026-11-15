// Settings, the computers sheet on Home, and the disconnect sheet. Everything here is made up.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/data/models.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/push/push_providers.dart';
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/link_providers.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/screens/computers_sheet.dart';
import 'package:phone/screens/settings_screen.dart';
import 'package:phone/src/rust/api/messages.dart';
import 'package:phone/theme/theme.dart';

import 'support/fake_push_platform.dart';
import 'support/memory_secure_keys.dart';

class _Lang extends Lang {
  _Lang(this.value);
  final String value;
  @override
  String build() => value;
}

class _FakeLink extends LinkController {
  int disconnected = 0;

  /// When set, [disconnect] waits for it; when [failWith] is set it throws that.
  Completer<void>? hold;
  Object? failWith;
  @override
  Future<AppPhase> build() async => const Unpaired();
  @override
  Future<void> disconnect() async {
    disconnected++;
    await hold?.future;
    if (failWith != null) throw failWith!;
  }
}

PairedComputer computer({PhoneRights? rights, int? pairedAt}) => PairedComputer(
  id: 'c',
  name: 'Studio Mac (demo)',
  relay: 'wss://relay.example',
  phoneName: 'p',
  desktopBoxPublic: Uint8List(32),
  rights:
      rights ??
      const PhoneRights(
        answer: true,
        plan: true,
        question: false,
        start: false,
        stop: false,
        activity: true,
      ),
  recvState: '',
  eventReservation: BigInt.zero,
  pairedAt: pairedAt ?? DateTime(2026, 11, 11).millisecondsSinceEpoch,
);

/// Files and the Rust core answer in real time: alternate real time with the test's own frames.
Future<void> realWait(WidgetTester tester) async {
  for (var i = 0; i < 24; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 30)),
    );
    await tester.pump();
  }
}

void main() {
  late Directory tmp;
  late MemorySecureKeys keys;
  setUp(() {
    tmp = Directory.systemTemp.createTempSync('bobix-settings-');
    keys = MemorySecureKeys();
  });
  tearDown(() => tmp.deleteSync(recursive: true));

  Future<_FakeLink> pump(
    WidgetTester tester, {
    PairedComputer? paired,
    String lang = 'en',
    ConnState conn = ConnState.ok,
    Widget screen = const SettingsScreen(),
    Brightness brightness = Brightness.light,
  }) async {
    tester.view.physicalSize = const Size(400, 2600);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final link = _FakeLink();
    final overrides = <Override>[
      langProvider.overrideWith(() => _Lang(lang)),
      supportDirectoryProvider.overrideWith((ref) async => tmp),
      pairedComputerProvider.overrideWithValue(paired),
      linkStatusProvider.overrideWith(
        (ref) => Stream.value(LinkStatus(conn: conn)),
      ),
      linkControllerProvider.overrideWith(() => link),
      secureKeysProvider.overrideWithValue(keys),
      pushPlatformProvider.overrideWithValue(FakePushPlatform()),
    ];
    await tester.pumpWidget(
      ProviderScope(
        key: UniqueKey(),
        overrides: overrides,
        child: MaterialApp(
          theme: bobixTheme(brightness),
          home: Scaffold(body: SafeArea(child: screen)),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
    return link;
  }

  testWidgets('the paired computer and how it is; the rights are one row', (
    tester,
  ) async {
    await pump(tester, paired: computer());
    expect(find.text('Computers'), findsOneWidget);
    expect(find.text('Studio Mac (demo)'), findsOneWidget);
    expect(find.text('connected 11 Nov · online'), findsOneWidget);
    expect(find.text('Connect another computer'), findsOneWidget);
    expect(find.text('Permissions'), findsOneWidget);
    expect(find.text('What this phone may do'), findsOneWidget);
    expect(find.text('3 of 6 allowed on Studio Mac (demo)'), findsOneWidget);
    // The six rights are not listed in Settings any more.
    expect(find.byIcon(Icons.check), findsNothing);
    expect(find.text('Start tickets'), findsNothing);
  });

  Future<void> openPermissions(WidgetTester tester) async {
    await tester.tap(find.text('What this phone may do'));
    await tester.pumpAndSettle();
  }

  testWidgets(
    'the permissions screen: allowed, not allowed, always, and read only',
    (tester) async {
      await pump(tester, paired: computer());
      await openPermissions(tester);
      // The computer, this phone's name and the count.
      expect(find.text('Studio Mac (demo)'), findsOneWidget);
      expect(find.text('p on this computer'), findsOneWidget);
      expect(find.text('3/6'), findsOneWidget);
      // Granted: answer, plan, activity. Not granted: question, start, stop.
      expect(find.text('Allowed'), findsOneWidget);
      expect(find.text('Not allowed'), findsOneWidget);
      expect(find.byIcon(Icons.check), findsNWidgets(3));
      expect(find.byIcon(Icons.close), findsNWidgets(3));
      for (final (title, d) in [
        ('Allow or deny requests', 'Allow or deny one request at a time'),
        ('Approve or send back plans', 'With an optional note'),
        ('Answer questions', 'Pick an option or type an answer'),
        ('Start tickets', 'Full or Quick, in a folder that exists'),
        (
          'Stop a waiting session',
          'Interrupts the turn; the session stays open',
        ),
        (
          'See what sessions are doing',
          'The last lines of each session, read only',
        ),
      ]) {
        expect(find.text(title), findsOneWidget);
        expect(find.text(d), findsOneWidget);
      }
      // What always holds, and where the rights are changed.
      expect(find.text('Always, whatever is allowed'), findsOneWidget);
      expect(find.textContaining('before each command'), findsOneWidget);
      expect(find.text('No “Always allow”'), findsOneWidget);
      expect(find.textContaining('Bobix → Settings → Phones'), findsOneWidget);
      // Read only: nothing here can be switched.
      expect(find.byType(Switch), findsNothing);
      expect(find.byType(Checkbox), findsNothing);
    },
  );

  testWidgets('everything allowed: no "Not allowed" part', (tester) async {
    await pump(
      tester,
      paired: computer(
        rights: const PhoneRights(
          answer: true,
          plan: true,
          question: true,
          start: true,
          stop: true,
          activity: true,
        ),
      ),
    );
    expect(find.text('6 of 6 allowed on Studio Mac (demo)'), findsOneWidget);
    await openPermissions(tester);
    expect(find.text('6/6'), findsOneWidget);
    expect(find.text('Not allowed'), findsNothing);
    expect(find.byIcon(Icons.close), findsNothing);
  });

  testWidgets('nothing allowed: says so, and lists all six as not allowed', (
    tester,
  ) async {
    await pump(
      tester,
      paired: computer(
        rights: const PhoneRights(
          answer: false,
          plan: false,
          question: false,
          start: false,
          stop: false,
          activity: false,
        ),
      ),
    );
    expect(find.text('0 of 6 allowed on Studio Mac (demo)'), findsOneWidget);
    await openPermissions(tester);
    expect(
      find.text('Nothing yet. Turn permissions on at the computer.'),
      findsOneWidget,
    );
    expect(find.byIcon(Icons.check), findsNothing);
    expect(find.byIcon(Icons.close), findsNWidgets(6));
  });

  testWidgets('Vietnamese and dark: the same permissions screen', (
    tester,
  ) async {
    await pump(
      tester,
      paired: computer(),
      lang: 'vi',
      brightness: Brightness.dark,
    );
    expect(find.text('Được 3/6 quyền trên Studio Mac (demo)'), findsOneWidget);
    await tester.tap(find.text('Quyền của điện thoại này'));
    await tester.pumpAndSettle();
    expect(find.text('Được phép'), findsOneWidget);
    expect(find.text('Không được phép'), findsOneWidget);
    expect(find.text('Luôn áp dụng'), findsOneWidget);
    expect(find.text('Bắt đầu ticket'), findsOneWidget);
    expect(
      find.text('Đầy đủ hoặc Nhanh, trong thư mục có sẵn'),
      findsOneWidget,
    );
    expect(find.textContaining('Bobix → Cài đặt → Điện thoại'), findsOneWidget);
  });

  testWidgets('each state of the computer reads its own way', (tester) async {
    for (final (conn, text) in [
      (ConnState.asleep, 'connected 11 Nov · asleep'),
      (ConnState.offline, 'connected 11 Nov · offline'),
      (ConnState.relayDown, 'connected 11 Nov · not reachable'),
      (ConnState.noNetwork, 'connected 11 Nov · not reachable'),
    ]) {
      await pump(tester, paired: computer(), conn: conn);
      expect(find.text(text), findsOneWidget, reason: '$conn');
    }
  });

  testWidgets('not paired: no computers and no rights, but the rest is there', (
    tester,
  ) async {
    await pump(tester);
    expect(find.text('Computers'), findsNothing);
    expect(find.text('Disconnect'), findsNothing);
    expect(find.text('Safety'), findsOneWidget);
    expect(find.text('Send feedback'), findsOneWidget);
  });

  testWidgets('Vietnamese: the same screen in the person\'s words', (
    tester,
  ) async {
    await pump(tester, paired: computer(), lang: 'vi');
    expect(find.text('Máy tính'), findsOneWidget);
    expect(find.text('kết nối 11 thg 11 · đang trực tuyến'), findsOneWidget);
    expect(find.text('Kết nối máy tính khác'), findsOneWidget);
    expect(find.text('Gửi góp ý'), findsOneWidget);
  });

  testWidgets('disconnect asks first; only the Erase button disconnects', (
    tester,
  ) async {
    final link = await pump(tester, paired: computer());
    await tester.tap(find.text('Disconnect'));
    await tester.pumpAndSettle();
    expect(find.text('Disconnect from Studio Mac (demo)?'), findsOneWidget);
    expect(find.textContaining('its keys are erased'), findsOneWidget);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(link.disconnected, 0);
    await tester.tap(find.text('Disconnect'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Disconnect and erase'));
    await tester.pumpAndSettle();
    expect(link.disconnected, 1);
  });

  testWidgets(
    'Disconnect waits for the result: the button is off meanwhile, and the sheet closes once it is done',
    (tester) async {
      final link = await pump(tester, paired: computer());
      link.hold = Completer<void>();
      await tester.tap(find.text('Disconnect'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Disconnect and erase'));
      await tester.pump();
      await tester.tap(find.text('Disconnect and erase'), warnIfMissed: false);
      await tester.tap(find.text('Cancel'), warnIfMissed: false);
      await tester.pump();
      expect(link.disconnected, 1, reason: 'a second press does nothing');
      expect(find.text('Disconnect from Studio Mac (demo)?'), findsOneWidget);
      link.hold!.complete();
      await tester.pumpAndSettle();
      expect(find.text('Disconnect from Studio Mac (demo)?'), findsNothing);
    },
  );

  testWidgets('a key that cannot be erased is said, and the person can retry', (
    tester,
  ) async {
    final link = await pump(tester, paired: computer());
    link.failWith = StateError('keychain refused');
    await tester.tap(find.text('Disconnect'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Disconnect and erase'));
    await tester.pumpAndSettle();
    expect(find.text('Could not erase the keys. Try again.'), findsOneWidget);
    link.failWith = null;
    await tester.tap(find.text('Disconnect and erase'));
    await tester.pumpAndSettle();
    expect(link.disconnected, 2);
    expect(find.text('Disconnect from Studio Mac (demo)?'), findsNothing);
  });

  testWidgets('the Safety group has the Hide details switch', (tester) async {
    await pump(tester, paired: computer());
    expect(find.text('Hide details'), findsOneWidget);
  });

  testWidgets(
    'turning the lock off asks first: a cancelled check leaves it on, a passed one switches it off',
    (tester) async {
      await pump(tester, paired: computer());
      keys.authAnswer = AppAuth.cancelled;
      await tester.tap(find.byKey(const Key('lock-switch')));
      await tester.pump();
      await realWait(tester);
      expect(keys.authReasons, ['to turn the lock off']);
      expect(
        tester.widget<Switch>(find.byKey(const Key('lock-switch'))).value,
        isTrue,
      );
      keys.authAnswer = AppAuth.ok;
      await tester.tap(find.byKey(const Key('lock-switch')));
      await tester.pump();
      await realWait(tester);
      expect(
        tester.widget<Switch>(find.byKey(const Key('lock-switch'))).value,
        isFalse,
      );
      // Turning it back on asks nothing.
      await tester.tap(find.byKey(const Key('lock-switch')));
      await tester.pump();
      await realWait(tester);
      expect(keys.authReasons, hasLength(2));
      expect(
        tester.widget<Switch>(find.byKey(const Key('lock-switch'))).value,
        isTrue,
      );
    },
  );

  testWidgets(
    'the lock is on by default, and the choice is kept in the settings file',
    (tester) async {
      await pump(tester, paired: computer());
      expect(
        tester.widget<Switch>(find.byKey(const Key('lock-switch'))).value,
        isTrue,
      );
      await tester.tap(find.byKey(const Key('lock-switch')));
      await tester.pump();
      await realWait(tester);
      expect(
        tester.widget<Switch>(find.byKey(const Key('lock-switch'))).value,
        isFalse,
      );
      final saved = jsonDecode(
        File('${tmp.path}/app-settings.json').readAsStringSync(),
      ) as Map;
      expect(saved['requireUnlock'], isFalse);
      expect(
        File('${tmp.path}/app-settings.json').readAsStringSync(),
        isNot(contains('Studio')),
        reason: 'nothing about the computer',
      );
    },
  );

  testWidgets('theme and language are chosen here and kept', (tester) async {
    await pump(tester, paired: computer());
    await tester.tap(find.text('Dark'));
    await tester.pump();
    await tester.tap(find.text('Tiếng Việt'));
    await tester.pump();
    expect(
      find.text('Máy tính'),
      findsOneWidget,
      reason: 'the language changed at once',
    );
    await realWait(tester);
    final saved = jsonDecode(
      File('${tmp.path}/app-settings.json').readAsStringSync(),
    ) as Map;
    expect(saved['theme'], 'dark');
    expect(saved['language'], 'vi');
  });

  testWidgets('the settings file brings the choices back', (tester) async {
    File('${tmp.path}/app-settings.json').writeAsStringSync(
      jsonEncode({'theme': 'light', 'language': 'vi', 'requireUnlock': false}),
    );
    await pump(tester, paired: computer());
    await realWait(tester);
    expect(find.text('Máy tính'), findsOneWidget);
    expect(
      tester.widget<Switch>(find.byKey(const Key('lock-switch'))).value,
      isFalse,
    );
  });

  testWidgets(
    'a settings file that cannot be read gives the defaults, with the lock on',
    (tester) async {
      File('${tmp.path}/app-settings.json').writeAsStringSync('{ nope');
      await pump(tester, paired: computer());
      expect(
        tester.widget<Switch>(find.byKey(const Key('lock-switch'))).value,
        isTrue,
      );
    },
  );

  testWidgets(
    'the computers sheet lists the computer and how many wait; one at a time',
    (tester) async {
      await pump(
        tester,
        screen: Builder(
          builder: (context) => TextButton(
            onPressed: () => showComputersSheet(
              context,
              computer: 'Studio Mac (demo)',
              waiting: 2,
            ),
            child: const Text('open'),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      expect(find.text('Choose a computer'), findsOneWidget);
      expect(find.text('Studio Mac (demo)'), findsOneWidget);
      expect(find.text('2 need you'), findsOneWidget);
      expect(find.text('Only one computer is connected.'), findsOneWidget);
      await tester.tap(find.text('Close'));
      await tester.pumpAndSettle();
      expect(find.text('Choose a computer'), findsNothing);
    },
  );

  testWidgets('the largest font overflows nothing, in both languages', (
    tester,
  ) async {
    tester.platformDispatcher.textScaleFactorTestValue = 2.0;
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    for (final lang in ['en', 'vi']) {
      await pump(tester, paired: computer(), lang: lang);
      expect(tester.takeException(), isNull, reason: lang);
      await pump(tester, paired: computer(), lang: lang);
      await tester.tap(
        find.text(
          lang == 'en' ? 'What this phone may do' : 'Quyền của điện thoại này',
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: 'permissions $lang');
    }
  });
}
