// The app lock and the cover. A fake for the platform's Face ID check; the lifecycle is driven by the test binding.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/link/link_providers.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/lock/app_lock.dart';
import 'package:phone/lock/own_prompt.dart';
import 'package:phone/pets/pet_view.dart';
import 'package:phone/lock/secure_scope.dart';
import 'package:phone/src/rust/api/messages.dart';
import 'package:flutter/rendering.dart';
import 'package:phone/theme/theme.dart';
import 'package:phone/theme/tokens.dart';

import 'support/memory_secure_keys.dart';

PairedComputer computer() => PairedComputer(
  id: 'c',
  name: 'Studio Mac (demo)',
  relay: 'wss://relay.example',
  phoneName: 'p',
  desktopBoxPublic: Uint8List(32),
  rights: const PhoneRights(
    answer: true,
    plan: true,
    question: true,
    start: true,
    stop: true,
    activity: true,
  ),
  recvState: '',
  eventReservation: BigInt.zero,
);

class _Lang extends Lang {
  @override
  String build() => 'en';
}

void main() {
  for (final dark in [false, true]) {
    testWidgets(
      'the cover hides what is behind it (pixels, ${dark ? 'dark' : 'light'})',
      (tester) async {
        final key = GlobalKey();
        tester.view
          ..physicalSize = const Size(200, 200)
          ..devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        await tester.pumpWidget(
          ProviderScope(
            child: MaterialApp(
              theme: bobixTheme(dark ? Brightness.dark : Brightness.light),
              home: RepaintBoundary(
                key: key,
                child: Stack(
                  children: const [
                    // Content that would show through a see-through cover: a solid block over the whole page.
                    Positioned.fill(
                      child: ColoredBox(color: Color(0xFFFF0000)),
                    ),
                    Positioned.fill(child: CoverScreen()),
                  ],
                ),
              ),
            ),
          ),
        );
        await tester.pump();
        final bg = (dark ? BobixTokens.dark : BobixTokens.light).bg;
        final pixel = await tester.runAsync(() async {
          final boundary =
              key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
          final image = await boundary.toImage();
          final data = (await image.toByteData())!;
          // A corner, away from the "Bobix" text in the middle.
          return ui.Color.fromARGB(
            data.getUint8(3),
            data.getUint8(0),
            data.getUint8(1),
            data.getUint8(2),
          );
        });
        expect(pixel, isNotNull);
        expect(
          pixel!.toARGB32(),
          bg.toARGB32(),
          reason: 'the red block must not show',
        );
      },
    );
  }

  late Directory tmp;
  late MemorySecureKeys keys;
  setUp(() {
    tmp = Directory.systemTemp.createTempSync('bobix-lock-');
    keys = MemorySecureKeys();
  });
  tearDown(() => tmp.deleteSync(recursive: true));

  Future<void> pump(
    WidgetTester tester, {
    bool paired = true,
    bool? requireUnlock,
  }) async {
    if (requireUnlock != null) {
      File('${tmp.path}/app-settings.json')
          .writeAsStringSync(jsonEncode({'requireUnlock': requireUnlock}));
    }
    final overrides = <Override>[
      langProvider.overrideWith(_Lang.new),
      secureKeysProvider.overrideWithValue(keys),
      supportDirectoryProvider.overrideWith((ref) async => tmp),
      pairedComputerProvider.overrideWithValue(paired ? computer() : null),
    ];
    await tester.pumpWidget(
      ProviderScope(
        key: UniqueKey(),
        overrides: overrides,
        child: MaterialApp(
          theme: bobixTheme(Brightness.light),
          builder: (context, child) => AppLock(child: child!),
          home: const Scaffold(body: Text('the app content')),
        ),
      ),
    );
    await settle(tester);
  }

  testWidgets(
    'locked at start: the content is not even built, and Face ID is asked with its reason',
    (tester) async {
      keys.authAnswer = AppAuth.cancelled;
      await pump(tester);
      expect(find.text('the app content'), findsNothing);
      expect(find.text('Bobix is locked'), findsOneWidget);
      expect(keys.authReasons, ['to open Bobix']);
    },
  );

  testWidgets(
    'the lock screen shows a random pet at work, what is the matter and the way in',
    (tester) async {
      keys.authAnswer = AppAuth.cancelled;
      await pump(tester);
      expect(find.byType(PetView), findsOneWidget);
      // A pet picked at random, at work: nothing about a session, and a different one each time the lock shows.
      expect(tester.widget<PetView>(find.byType(PetView)).state, 'working');
      expect(tester.widget<PetView>(find.byType(PetView)).size, 96);
      expect(find.text('Bobix'), findsOneWidget);
      expect(find.text('Bobix is locked'), findsOneWidget);
      expect(find.text('Unlock to see your sessions.'), findsOneWidget);
      expect(find.textContaining('Unlock with'), findsOneWidget);
    },
  );

  testWidgets('the check passes: the content shows', (tester) async {
    await pump(tester);
    expect(find.text('the app content'), findsOneWidget);
    expect(find.text('Bobix is locked'), findsNothing);
    expect(keys.authReasons, hasLength(1));
  });

  testWidgets('cancelled: still locked, and the button asks again', (
    tester,
  ) async {
    keys.authAnswer = AppAuth.cancelled;
    await pump(tester);
    expect(find.text('the app content'), findsNothing);
    keys.authAnswer = AppAuth.ok;
    await tester.tap(find.textContaining('Unlock with'));
    await settle(tester);
    expect(find.text('the app content'), findsOneWidget);
    expect(keys.authReasons, hasLength(2));
  });

  testWidgets('a check that failed says so and stays locked', (tester) async {
    keys.authAnswer = AppAuth.failed;
    await pump(tester);
    expect(find.text('That did not work. Try again.'), findsOneWidget);
    expect(find.text('the app content'), findsNothing);
  });

  testWidgets('a phone with nothing to ask with cannot lock: the app opens', (
    tester,
  ) async {
    keys.authAnswer = AppAuth.unavailable;
    await pump(tester);
    expect(find.text('the app content'), findsOneWidget);
  });

  testWidgets('switched off in Settings: no lock, and nothing is asked', (
    tester,
  ) async {
    await pump(tester, requireUnlock: false);
    expect(find.text('the app content'), findsOneWidget);
    expect(keys.authReasons, isEmpty);
  });

  testWidgets('no computer paired: nothing to lock', (tester) async {
    await pump(tester, paired: false);
    expect(find.text('the app content'), findsOneWidget);
    expect(keys.authReasons, isEmpty);
  });

  testWidgets(
    'going to the background covers the app and locks it; coming back asks again before any content',
    (tester) async {
      await pump(tester);
      expect(find.text('the app content'), findsOneWidget);
      keys.authAnswer = AppAuth.cancelled;
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await tester.pump();
      expect(
        find.byType(CoverScreen),
        findsOneWidget,
        reason: 'the app switcher takes its picture now',
      );
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
      await tester.pump();
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
      await tester.pump();
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await settle(tester);
      expect(find.byType(CoverScreen), findsNothing);
      expect(
        find.text('the app content'),
        findsNothing,
        reason: 'locked until the check passes',
      );
      expect(keys.authReasons, hasLength(2));
      keys.authAnswer = AppAuth.ok;
      await tester.tap(find.textContaining('Unlock with'));
      await settle(tester);
      expect(find.text('the app content'), findsOneWidget);
    },
  );

  testWidgets(
    'no cover while the app\'s own prompt is up; going to the background still locks',
    (tester) async {
      await pump(tester);
      final prompt = Completer<void>();
      final asked = duringOwnPrompt(() => prompt.future);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await tester.pump();
      expect(find.byType(CoverScreen), findsNothing);
      // Really in the background (frames stop there): still locked when the app comes back, prompt or not.
      keys.authAnswer = AppAuth.cancelled;
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await settle(tester);
      expect(find.text('Bobix is locked'), findsOneWidget);
      prompt.complete();
      await asked;
      expect(ownPromptActive, isFalse);
    },
  );

  testWidgets(
    'only inactive (the Face ID sheet itself, Control Center): covered, but no new check on return',
    (tester) async {
      await pump(tester);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await tester.pump();
      expect(find.byType(CoverScreen), findsOneWidget);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await settle(tester);
      expect(find.byType(CoverScreen), findsNothing);
      expect(find.text('the app content'), findsOneWidget);
      expect(keys.authReasons, hasLength(1));
    },
  );

  testWidgets(
    'a page the person pushed survives a lock and an unlock, with what they typed',
    (tester) async {
      await pump(tester);
      final nav = tester.state<NavigatorState>(find.byType(Navigator));
      nav.push(
        MaterialPageRoute<void>(
          builder: (_) => const Scaffold(body: TextField(key: Key('draft'))),
        ),
      );
      await settle(tester);
      await tester.enterText(find.byKey(const Key('draft')), 'half an answer');
      keys.authAnswer = AppAuth.cancelled;
      for (final s in [
        AppLifecycleState.inactive,
        AppLifecycleState.hidden,
        AppLifecycleState.paused,
        AppLifecycleState.hidden,
        AppLifecycleState.inactive,
        AppLifecycleState.resumed,
      ]) {
        tester.binding.handleAppLifecycleStateChanged(s);
        await tester.pump();
      }
      await settle(tester);
      expect(find.text('Bobix is locked'), findsOneWidget);
      expect(
        find.byKey(const Key('draft')),
        findsNothing,
        reason: 'mounted but not painted',
      );
      expect(
        find.byKey(const Key('draft'), skipOffstage: false),
        findsOneWidget,
      );
      keys.authAnswer = AppAuth.ok;
      await tester.tap(find.textContaining('Unlock with'));
      await settle(tester);
      expect(find.byKey(const Key('draft')), findsOneWidget);
      expect(find.text('half an answer'), findsOneWidget);
    },
  );

  testWidgets(
    'an ok that arrives after the app went to the background does not unlock it',
    (tester) async {
      keys.authHold = Completer<AppAuth>();
      await pump(tester);
      expect(keys.authReasons, hasLength(1), reason: 'the check is up');
      for (final s in [
        AppLifecycleState.inactive,
        AppLifecycleState.hidden,
        AppLifecycleState.paused,
      ]) {
        tester.binding.handleAppLifecycleStateChanged(s);
        await tester.pump();
      }
      keys.authHold!.complete(AppAuth.ok);
      await settle(tester);
      keys.authHold = null;
      keys.authAnswer = AppAuth.cancelled;
      for (final s in [
        AppLifecycleState.hidden,
        AppLifecycleState.inactive,
        AppLifecycleState.resumed,
      ]) {
        tester.binding.handleAppLifecycleStateChanged(s);
        await tester.pump();
      }
      await settle(tester);
      expect(find.text('the app content'), findsNothing);
      expect(find.text('Bobix is locked'), findsOneWidget);
      expect(
        keys.authReasons,
        hasLength(2),
        reason: 'the next visit asks again',
      );
    },
  );

  testWidgets('the lock screen reads in Vietnamese too', (tester) async {
    keys.authAnswer = AppAuth.cancelled;
    tester.view.physicalSize = const Size(400, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          langProvider.overrideWith(() => _Vi()),
          secureKeysProvider.overrideWithValue(keys),
          supportDirectoryProvider.overrideWith((ref) async => tmp),
          pairedComputerProvider.overrideWithValue(computer()),
        ],
        child: MaterialApp(
          theme: bobixTheme(Brightness.light),
          builder: (context, child) => AppLock(child: child!),
          home: const SizedBox(),
        ),
      ),
    );
    await settle(tester);
    expect(find.text('Bobix đang khoá'), findsOneWidget);
  });

  group('secure window', () {
    final calls = <bool>[];
    setUp(() {
      calls.clear();
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(
            const MethodChannel('dev.bobix.phone/window'),
            (call) async {
              calls.add((call.arguments as Map)['secure'] as bool);
              return null;
            },
          );
    });
    tearDown(
      () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(
            const MethodChannel('dev.bobix.phone/window'),
            null,
          ),
    );

    testWidgets(
      'a detail screen marks the window secure; leaving the last one clears it',
      (tester) async {
        await tester.pumpWidget(
          const MaterialApp(home: SecureScope(child: Text('a request'))),
        );
        expect(calls, [true]);
        await tester.pumpWidget(
          const MaterialApp(
            home: SecureScope(child: SecureScope(child: Text('a plan'))),
          ),
        );
        expect(calls.last, isTrue);
        await tester.pumpWidget(
          const MaterialApp(home: SecureScope(child: Text('a request'))),
        );
        expect(calls.last, isTrue, reason: 'one scope is still open');
        await tester.pumpWidget(const MaterialApp(home: Text('home')));
        expect(calls.last, isFalse);
      },
    );
  });
}

class _Vi extends Lang {
  @override
  String build() => 'vi';
}

Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 12; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await tester.pump();
  }
}
