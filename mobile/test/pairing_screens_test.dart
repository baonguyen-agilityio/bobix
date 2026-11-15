// The pairing screens and "pair again", with a stand-in for the camera. Everything here is made up.
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/app.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/data/phone_data.dart';
import 'package:phone/features/actions/action_widgets.dart' show GlassActionBar;
import 'package:phone/widgets/detail_scaffold.dart' show PillButton;
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/link_providers.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/link/pairing_controller.dart';
import 'package:phone/link/pairing_service.dart';
import 'package:phone/link/relay_session.dart';
import 'package:phone/screens/pair_again_screen.dart';
import 'package:phone/screens/pairing_screens.dart';
import 'package:phone/src/rust/api/messages.dart';

import 'support/core_library.dart';
import 'support/fixture_phone_data.dart';
import 'support/memory_secure_keys.dart';

String b64Of(int fill) => b64(Uint8List.fromList(List.filled(32, fill)));

/// A well-formed code (nothing is sent): the relay is [relay].
String code(String relay) =>
    'bobix://pair?r=${Uri.encodeComponent(relay)}&a=${b64Of(7)}&b=${b64Of(9)}&t=${b64Of(5)}';

class _Lang extends Lang {
  _Lang(this.value);
  final String value;
  @override
  String build() => value;
}

class _Fixed extends PairingController {
  _Fixed(this.initial);
  final PairingState initial;
  @override
  PairingState build() => initial;
}

class _FakeLink extends LinkController {
  _FakeLink(this.initial);
  final AppPhase initial;
  int erased = 0;
  @override
  Future<AppPhase> build() async => initial;
  @override
  Future<void> eraseAndPairAgain() async => erased++;
}

const rights = PhoneRights(
  answer: true,
  plan: true,
  question: true,
  start: true,
  stop: true,
  activity: true,
);

Widget host({
  String lang = 'en',
  List<Override> overrides = const [],
  Widget home = const PairingFlow(),
}) => ProviderScope(
  key: UniqueKey(),
  overrides: [langProvider.overrideWith(() => _Lang(lang)), ...overrides],
  child: BobixApp(home: home),
);

Override scanner(List<String> texts) =>
    scannerBuilderProvider.overrideWithValue(
      (context, {required onCode, required onDenied}) => Column(
        children: [
          for (final t in texts)
            TextButton(onPressed: () => onCode(t), child: Text('scan $t')),
          TextButton(onPressed: onDenied, child: const Text('deny camera')),
        ],
      ),
    );

/// The Rust core answers on another thread, and every answer needs a turn of the real event loop and then of the test's
/// own: alternate them for a while.
Future<void> realWait(WidgetTester tester) async {
  for (var i = 0; i < 12; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 40)),
    );
    await tester.pump();
  }
}

void main() {
  late Directory tmp;
  setUpAll(initCoreLibrary);
  setUp(() => tmp = Directory.systemTemp.createTempSync('bobix-screens-'));
  tearDown(() => tmp.deleteSync(recursive: true));

  List<Override> base({RelayConnector? connect, MemorySecureKeys? keys}) => [
    supportDirectoryProvider.overrideWith((ref) async => tmp),
    relayConnectorProvider.overrideWithValue(
      connect ??
          (url) async => throw const SocketException('Connection refused'),
    ),
    secureKeysProvider.overrideWithValue(keys ?? MemorySecureKeys()),
  ];

  testWidgets(
    'welcome leads to the scanner; a code that is not Bobix\'s says so; each way out works',
    (tester) async {
      await tester.pumpWidget(
        host(
          overrides: [
            ...base(),
            scanner(['https://example.com/x']),
          ],
        ),
      );
      await tester.pump();
      expect(find.text('Bobix on your phone'), findsOneWidget);
      await tester.tap(find.text('Scan the code'));
      await tester.pump();
      await tester.pump();
      expect(find.text('Scan the code on your computer'), findsOneWidget);
      await tester.tap(find.text('scan https://example.com/x'));
      await realWait(tester);
      expect(find.text('This is not a Bobix code'), findsOneWidget);
      await tester.tap(find.text('Try again'));
      await tester.pump();
      expect(find.text('Scan the code on your computer'), findsOneWidget);
    },
  );

  testWidgets(
    'a relay that is not Bobix\'s is named before the person goes on; rights; refusing leaves nothing',
    (tester) async {
      tester.view.physicalSize = const Size(
        400,
        2400,
      ); // tall, so a lazy list builds every row
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final keys = MemorySecureKeys();
      await tester.pumpWidget(
        host(
          overrides: [
            ...base(keys: keys),
            scanner([code('wss://relay.example/ws')]),
          ],
        ),
      );
      await tester.pump();
      await tester.tap(find.text('Scan the code'));
      await tester.pump();
      await tester.pump();
      await tester.tap(find.text('scan ${code('wss://relay.example/ws')}'));
      await realWait(tester);
      expect(
        find.text('This code uses the relay relay.example'),
        findsOneWidget,
      );
      await tester.tap(find.text('Continue'));
      await tester.pump();
      expect(find.text('Name this phone'), findsOneWidget);
      expect(find.text('What this phone may do'), findsOneWidget);
      for (final r in [
        'Answer permission requests',
        'Answer Claude’s questions',
        'Approve or send back plans',
        'Start tickets',
        'See session activity',
        'Stop a session',
      ]) {
        expect(find.text(r), findsOneWidget);
      }
      // Stop needs the right to answer: turning that off turns Stop off and locks it.
      await tester.tap(find.text('Answer permission requests'));
      await tester.pump();
      expect(
        tester
            .widget<SwitchListTile>(
              find.widgetWithText(SwitchListTile, 'Stop a session'),
            )
            .onChanged,
        isNull,
      );
      expect(find.text('Needs “Allow or deny requests” too.'), findsOneWidget);
      await tester.tap(find.text('Ask the computer to confirm'));
      await realWait(tester);
      // The fake network refuses: its own message, a way out, and no key or state kept.
      expect(find.text('Can’t reach the relay'), findsOneWidget);
      expect(keys.secrets, isEmpty);
      expect(keys.hasSigningKey, isFalse);
      expect(File('${tmp.path}/link-state.json').existsSync(), isFalse);
    },
  );

  testWidgets('an unreadable camera permission offers the system settings', (
    tester,
  ) async {
    await tester.pumpWidget(host(overrides: [...base(), scanner([])]));
    await tester.pump();
    await tester.tap(find.text('Scan the code'));
    await tester.pump();
    await tester.pump();
    await tester.tap(find.text('deny camera'));
    await tester.pump();
    expect(find.text('The camera is off for Bobix'), findsOneWidget);
    expect(find.text('Open Settings'), findsOneWidget);
  });

  testWidgets('every failure has its own words, in English and in Vietnamese', (
    tester,
  ) async {
    const english = {
      PairFailure.notBobix: 'This is not a Bobix code',
      PairFailure.insecureRelay: 'This relay is not secure',
      PairFailure.codeExpired: 'The code expired',
      PairFailure.codeUsed: 'The code was already used',
      PairFailure.relayUnreachable: 'Can’t reach the relay',
      PairFailure.computerSaidNo: 'The computer said no',
      PairFailure.twoPhones: 'Two phones answered this code',
      PairFailure.noStrongBiometrics: 'This phone has no strong biometrics',
      PairFailure.keyFailed: 'This phone could not make its key',
      PairFailure.alreadyPaired: 'This phone is already paired',
      PairFailure.biometryLockedOut: 'Unlock with your passcode first',
      PairFailure.inviteNeeded:
          'This relay needs an invitation for the computer',
    };
    const vietnamese = {
      PairFailure.codeExpired: 'Mã đã hết hạn',
      PairFailure.twoPhones: 'Hai điện thoại cùng trả lời mã này',
      PairFailure.noStrongBiometrics:
          'Điện thoại này chưa có sinh trắc học đủ mạnh',
    };
    for (final e in english.entries) {
      await tester.pumpWidget(
        host(
          overrides: [
            pairingControllerProvider.overrideWith(
              () => _Fixed(PairingState(step: PairStep.failed, failure: e.key)),
            ),
          ],
        ),
      );
      await tester.pump();
      expect(find.text(e.value), findsOneWidget, reason: '${e.key}');
    }
    for (final e in vietnamese.entries) {
      await tester.pumpWidget(
        host(
          lang: 'vi',
          overrides: [
            pairingControllerProvider.overrideWith(
              () => _Fixed(PairingState(step: PairStep.failed, failure: e.key)),
            ),
          ],
        ),
      );
      await tester.pump();
      expect(find.text(e.value), findsOneWidget, reason: 'vi ${e.key}');
    }
    for (final phoneSide in [true, false]) {
      await tester.pumpWidget(
        host(
          overrides: [
            pairingControllerProvider.overrideWith(
              () => _Fixed(
                PairingState(
                  step: PairStep.failed,
                  failure: PairFailure.updateNeeded,
                  phoneMustUpdate: phoneSide,
                ),
              ),
            ),
          ],
        ),
      );
      await tester.pump();
      expect(
        find.text(
          phoneSide
              ? 'Update Bobix on this phone'
              : 'Update Bobix on the computer',
        ),
        findsOneWidget,
      );
    }
  });

  testWidgets(
    'done shows exactly what was granted, and says so when it is less',
    (tester) async {
      final granted = PairedComputer(
        id: 'c',
        name: 'Studio Mac (demo)',
        relay: 'wss://relay.example',
        phoneName: 'p',
        desktopBoxPublic: Uint8List(32),
        rights: const PhoneRights(
          answer: true,
          plan: false,
          question: false,
          start: false,
          stop: false,
          activity: true,
        ),
        recvState: '',
        eventReservation: BigInt.zero,
      );
      await tester.pumpWidget(
        host(
          overrides: [
            pairingControllerProvider.overrideWith(
              () => _Fixed(
                PairingState(
                  step: PairStep.done,
                  computer: granted,
                  asked: rights,
                ),
              ),
            ),
          ],
        ),
      );
      await tester.pump();
      expect(find.text('Connected to Studio Mac (demo)'), findsOneWidget);
      expect(
        find.text('Answer permission requests, See session activity'),
        findsOneWidget,
      );
      expect(find.textContaining('Start tickets'), findsNothing);
      expect(find.text('Open Home'), findsOneWidget);
      expect(
        find.text('The computer gave fewer rights than you asked for.'),
        findsOneWidget,
      );
    },
  );

  testWidgets('waiting for the computer can be cancelled', (tester) async {
    await tester.pumpWidget(
      host(
        overrides: [
          pairingControllerProvider.overrideWith(
            () => _Fixed(const PairingState(step: PairStep.waiting)),
          ),
        ],
      ),
    );
    await tester.pump();
    expect(find.text('Confirm on your computer'), findsOneWidget);
    expect(find.text('Cancel'), findsOneWidget);
  });

  testWidgets('pair again: the relay case and the computer case, and erasing', (
    tester,
  ) async {
    final link = _FakeLink(
      const PairAgain(computer: 'Studio Mac (demo)', removedByComputer: false),
    );
    await tester.pumpWidget(
      host(
        home: const RootGate(),
        overrides: [linkControllerProvider.overrideWith(() => link)],
      ),
    );
    await tester.pump();
    await tester.pump();
    expect(find.text('Pair again'), findsOneWidget);
    expect(
      find.textContaining('may have disconnected this phone'),
      findsOneWidget,
    );
    await tester.tap(find.text('Erase and pair again'));
    expect(link.erased, 1);

    await tester.pumpWidget(
      host(
        home: const PairAgainScreen(
          phase: PairAgain(
            computer: 'Studio Mac (demo)',
            removedByComputer: true,
          ),
        ),
      ),
    );
    await tester.pump();
    expect(find.text('This phone was disconnected'), findsOneWidget);
    expect(find.text('Disconnected on Studio Mac (demo)'), findsOneWidget);
    expect(find.text('Nothing. The log was erased too.'), findsOneWidget);
    expect(find.text('Connect again'), findsOneWidget);
  });

  testWidgets(
    'every pairing step shows the title, step dots and its action in the bottom bar',
    (tester) async {
      final computer = PairedComputer(
        id: 'c',
        name: 'Studio Mac (demo)',
        relay: 'wss://relay.example',
        phoneName: 'p',
        desktopBoxPublic: Uint8List(32),
        rights: rights,
        recvState: '',
        eventReservation: BigInt.zero,
      );
      // The step the dots show, and whether a Back pill is there, for each page.
      final pages = <(PairingState, int?, bool)>[
        (const PairingState(), 1, false),
        (const PairingState(step: PairStep.scan), 2, true),
        (const PairingState(step: PairStep.details), 3, true),
        (const PairingState(step: PairStep.waiting), 3, false),
        (
          PairingState(step: PairStep.done, computer: computer, asked: rights),
          null,
          false,
        ),
      ];
      for (final (st, dots, back) in pages) {
        await tester.pumpWidget(
          host(
            overrides: [
              pairingControllerProvider.overrideWith(() => _Fixed(st)),
              scanner([]),
            ],
          ),
        );
        await tester.pump();
        expect(
          find.text('Connect to your computer'),
          findsOneWidget,
          reason: '${st.step}',
        );
        if (dots == null) {
          expect(find.byKey(const Key('pair-step-dots')), findsNothing);
        } else {
          expect(find.byKey(const Key('pair-step-dots')), findsOneWidget);
          // The step the person is on is the long pill; the others are small.
          for (var i = 1; i <= 3; i++) {
            expect(
              tester.getSize(find.byKey(Key('pair-dot-$i'))).width,
              i == dots ? 26 + 6 : 8 + 6, // with the 3 pt gap on each side
              reason: '${st.step} dot $i',
            );
          }
        }
        expect(find.text('Back'), back ? findsOneWidget : findsNothing);
        // The action sits in the glass bar at the bottom, across the width of the screen.
        // (The scan page has none: the camera is the action.)
        final bar = find.byType(GlassActionBar);
        if (st.step == PairStep.scan) {
          expect(bar, findsNothing);
          continue;
        }
        expect(bar, findsOneWidget);
        final screen = tester.view.physicalSize / tester.view.devicePixelRatio;
        expect(tester.getRect(bar).bottom, screen.height);
        expect(tester.getRect(bar).width, screen.width);
      }
    },
  );

  testWidgets(
    'a phone with no strong biometrics is told before it scans anything',
    (tester) async {
      final keys = MemorySecureKeys(biometrics: false);
      await tester.pumpWidget(
        host(
          overrides: [
            ...base(keys: keys),
            scanner([]),
          ],
        ),
      );
      await tester.pump();
      await tester.tap(find.text('Scan the code'));
      await tester.pump();
      await tester.pump();
      expect(find.text('This phone has no strong biometrics'), findsOneWidget);
      expect(find.text('What to do'), findsOneWidget);
      expect(
        find.text(
          'Open this phone’s Settings and find fingerprint or face unlock.',
        ),
        findsOneWidget,
      );
      expect(find.text('Scan the code on your computer'), findsNothing);
      // Back goes to the welcome page.
      await tester.tap(find.widgetWithText(PillButton, 'Back'));
      await tester.pump();
      expect(find.text('Bobix on your phone'), findsOneWidget);
    },
  );

  testWidgets('the same page when pairing finds out late', (tester) async {
    await tester.pumpWidget(
      host(
        overrides: [
          pairingControllerProvider.overrideWith(
            () => _Fixed(
              const PairingState(
                step: PairStep.failed,
                failure: PairFailure.noStrongBiometrics,
              ),
            ),
          ),
        ],
      ),
    );
    await tester.pump();
    expect(find.text('What to do'), findsOneWidget);
    expect(find.text('Try again'), findsNothing);
  });

  testWidgets(
    'disconnected: the card says why and what is left, with a way back',
    (tester) async {
      final link = _FakeLink(
        const PairAgain(computer: 'Studio Mac (demo)', removedByComputer: true),
      );
      await tester.pumpWidget(
        host(
          home: const RootGate(),
          overrides: [linkControllerProvider.overrideWith(() => link)],
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(find.text('This phone was disconnected'), findsOneWidget);
      expect(find.text('Why'), findsOneWidget);
      expect(find.text('Data left here'), findsOneWidget);
      expect(
        find.text(
          'If this wasn’t you, someone with access to your computer did it.',
        ),
        findsOneWidget,
      );
      await tester.tap(find.text('Connect again'));
      expect(link.erased, 1);
    },
  );

  testWidgets(
    'the largest system font size overflows no pairing screen, in both languages',
    (tester) async {
      tester.view.physicalSize = const Size(400, 800);
      tester.view.devicePixelRatio = 1;
      tester.platformDispatcher.textScaleFactorTestValue = 2.0;
      addTearDown(() {
        tester.view.reset();
        tester.platformDispatcher.clearTextScaleFactorTestValue();
      });
      final computer = PairedComputer(
        id: 'c',
        name: 'Studio Mac (demo)',
        relay: 'wss://relay.example',
        phoneName: 'p',
        desktopBoxPublic: Uint8List(32),
        rights: rights,
        recvState: '',
        eventReservation: BigInt.zero,
      );
      final states = [
        const PairingState(),
        const PairingState(step: PairStep.scan),
        const PairingState(step: PairStep.scan, cameraDenied: true),
        const PairingState(step: PairStep.relay, relayHost: 'relay.example'),
        const PairingState(step: PairStep.details),
        const PairingState(step: PairStep.waiting),
        const PairingState(step: PairStep.noBiometrics),
        PairingState(step: PairStep.done, computer: computer, asked: rights),
        for (final f in PairFailure.values.where(
          (f) => f != PairFailure.cancelled,
        ))
          PairingState(step: PairStep.failed, failure: f),
      ];
      for (final lang in ['en', 'vi']) {
        for (final st in states) {
          await tester.pumpWidget(
            host(
              lang: lang,
              overrides: [
                pairingControllerProvider.overrideWith(() => _Fixed(st)),
              ],
            ),
          );
          await tester.pump();
          expect(
            tester.takeException(),
            isNull,
            reason: '$lang ${st.step} ${st.failure}',
          );
        }
      }
      await tester.pumpWidget(
        host(
          lang: 'vi',
          home: const PairAgainScreen(
            phase: PairAgain(
              computer: 'Studio Mac (demo)',
              removedByComputer: false,
            ),
          ),
        ),
      );
      await tester.pump();
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('a followed relay move is shown on the app, in both languages', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(400, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    for (final (lang, title) in [
      ('en', 'Now connected through relay.example'),
      ('vi', 'Giờ kết nối qua relay.example'),
    ]) {
      await tester.pumpWidget(
        host(
          lang: lang,
          home: const AppShell(),
          overrides: [
            phoneDataProvider.overrideWithValue(const FixturePhoneData()),
            linkStatusProvider.overrideWith(
              (ref) =>
                  Stream.value(const LinkStatus(movedHost: 'relay.example')),
            ),
          ],
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(find.text(title), findsOneWidget, reason: lang);
    }
  });
}
