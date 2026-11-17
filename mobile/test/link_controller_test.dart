// Which screen the app opens on, from what is stored: a read that fails for now is never "the pairing is gone".
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/app.dart';
import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/actions_source.dart';
import 'package:phone/features/actions/request_screen.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/link/link_providers.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/link/pairing_controller.dart';
import 'package:phone/log/activity_log.dart';
import 'package:phone/push/push_providers.dart';
import 'package:phone/screens/pair_again_screen.dart';
import 'package:phone/src/rust/api/core.dart';
import 'package:phone/src/rust/api/messages.dart';
import 'package:phone/widgets/glass_backdrop.dart';

import 'support/core_library.dart';
import 'support/fake_push_platform.dart';
import 'support/memory_secure_keys.dart';
import 'support/scripted_socket.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory tmp;
  late MemorySecureKeys keys;
  late DeviceIdentity identity;

  setUpAll(initCoreLibrary);

  setUp(() async {
    tmp = Directory.systemTemp.createTempSync('bobix-controller-');
    keys = MemorySecureKeys();
    identity = await generateIdentity();
  });
  tearDown(() => tmp.deleteSync(recursive: true));

  Future<void> storePairing() async {
    await LinkStateStore(tmp).save(
      PairedComputer(
        id: 'c',
        name: 'Studio Mac (demo)',
        relay: 'ws://127.0.0.1:1',
        phoneName: 'p',
        desktopBoxPublic: Uint8List(32)..[0] = 9,
        rights: const PhoneRights(
          answer: true,
          plan: false,
          question: false,
          start: false,
          stop: false,
          activity: false,
        ),
        recvState: await recvStateNew(),
        eventReservation: BigInt.zero,
      ),
    );
    await keys.putSecret(SecureKeys.auth, identity.authSeed);
    await keys.putSecret(SecureKeys.box, identity.boxSecret);
  }

  final push = FakePushPlatform();

  ProviderContainer container() {
    push.calls.clear();
    final c = ProviderContainer(
      overrides: [
        pushPlatformProvider.overrideWithValue(push),
        supportDirectoryProvider.overrideWith((ref) async => tmp),
        secureKeysProvider.overrideWithValue(keys),
        relayConnectorProvider.overrideWithValue(
          (url) async => throw const SocketException('refused'),
        ),
      ],
    );
    addTearDown(c.dispose);
    return c;
  }

  Future<AppPhase> phaseOf(ProviderContainer c) =>
      c.read(linkControllerProvider.future);

  testWidgets('the signing key lost under an open request leads to pair again, with no error screen', (tester) async {
    await tester.runAsync(storePairing);
    File('${tmp.path}/app-settings.json').writeAsStringSync('{"requireUnlock": false}');
    keys.signAnswer = const KeyInvalidated();
    final c = ProviderContainer(
      overrides: [
        pushPlatformProvider.overrideWithValue(push),
        supportDirectoryProvider.overrideWith((ref) async => tmp),
        secureKeysProvider.overrideWithValue(keys),
        relayConnectorProvider.overrideWithValue(
          (url) async => throw const SocketException('refused'),
        ),
        linkPhoneData,
        ...linkActionOverrides,
      ],
    );
    expect(await tester.runAsync(() => c.read(linkControllerProvider.future)), isA<Paired>());
    await tester.pumpWidget(
      UncontrolledProviderScope(container: c, child: const BobixApp(home: RootGate())),
    );
    for (var i = 0; i < 4; i++) {
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 50)));
      await tester.pump(const Duration(milliseconds: 300));
    }
    tester.state<NavigatorState>(find.byType(Navigator).first).push(pageRoute<void>(const RequestScreen(id: 'r1')));
    for (var i = 0; i < 4; i++) {
      await tester.pump(const Duration(milliseconds: 300));
    }
    expect(find.byType(RequestScreen), findsOneWidget);

    final gate = c.read(biometricGateProvider);
    final result = await tester.runAsync(
      () => gate.confirm('Confirm', const Command(id: 'c1', kind: CommandKind.allow, target: 'r1', shown: 'h')),
    );
    expect(result, GateResult.keyLost);
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 300));
    }
    expect(tester.takeException(), isNull);
    expect(find.byType(RequestScreen), findsNothing);
    expect(find.byType(PairAgainScreen), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
    await tester.pump(const Duration(minutes: 5)); // the link's timers end with it
  });

  test('nothing stored: not paired', () async {
    expect(await phaseOf(container()), isA<Unpaired>());
  });

  test('a keychain read that fails for now keeps the pairing, and a retry opens it', () async {
    await storePairing();
    keys.failReads = true;
    final c = container();
    expect(
      await phaseOf(c),
      isA<Unavailable>(),
      reason: 'locked: not "keys lost"',
    );
    expect(keys.erased, isFalse);
    expect(
      await LinkStateStore(tmp).exists(),
      isTrue,
      reason: 'the pairing is not erased',
    );

    keys.failReads = false;
    c.read(linkControllerProvider.notifier).retry();
    final phase = await phaseOf(c);
    expect(phase, isA<Paired>());
    await (phase as Paired).link.dispose();
  });

  test(
    'keys that are definitely not there: pair again, and the rest is erased',
    () async {
      await storePairing();
      keys.secrets.clear();
      final phase = await phaseOf(container());
      expect(phase, isA<PairAgain>());
      expect(keys.erased, isTrue);
      expect(await LinkStateStore(tmp).exists(), isFalse);
    },
  );

  test(
    'a stored state that cannot be read for now is not "no pairing"',
    () async {
      // A directory where the file should be: reading it fails, as a locked file or a full disk would.
      Directory('${tmp.path}/link-state.json').createSync();
      expect(await phaseOf(container()), isA<Unavailable>());
      expect(keys.erased, isFalse);
    },
  );

  test('a stored state that is not valid at all: pair again', () async {
    File('${tmp.path}/link-state.json').writeAsStringSync('{ not json');
    expect(await phaseOf(container()), isA<PairAgain>());
  });

  test(
    'forget erases everything and lands on the welcome screen, no scanner',
    () async {
      await storePairing();
      final c = container();
      await phaseOf(c);
      await c.read(linkControllerProvider.notifier).forgetComputer();
      expect(c.read(linkControllerProvider).value, isA<Unpaired>());
      expect(keys.erased, isTrue);
      expect(await LinkStateStore(tmp).exists(), isFalse);
      expect(c.read(pairingControllerProvider).step, PairStep.welcome);
    },
  );

  test('the notification extension learns the computer at every start, and forgets it on Erase', () async {
    await storePairing();
    final c = container();
    final phase = await phaseOf(c);
    expect(phase, isA<Paired>());
    expect(push.calls.where((x) => x.startsWith('setComputer')), ['setComputer c 32 1-1']);
    await c.read(linkControllerProvider.notifier).forgetComputer();
    expect(push.calls.where((x) => x.startsWith('removeComputer')), ['removeComputer c']);
  });

  test('a pairing that just ended tells the extension the new computer', () async {
    final c = container();
    await phaseOf(c);
    await storePairing();
    final stored = (await LinkStateStore(tmp).load())!;
    await c.read(linkControllerProvider.notifier).paired(stored);
    expect(push.calls.where((x) => x.startsWith('setComputer')), ['setComputer c 32 1-1']);
    final phase = c.read(linkControllerProvider).value;
    if (phase is Paired) await phase.link.dispose();
  });

  test('a grant the relay refuses puts one line in the log, with the id the grant named', () async {
    await storePairing();
    final c = ProviderContainer(
      overrides: [
        pushPlatformProvider.overrideWithValue(push),
        supportDirectoryProvider.overrideWith((ref) async => tmp),
        secureKeysProvider.overrideWithValue(keys),
        relayConnectorProvider.overrideWithValue(
          (url) async => ScriptedSocket()
            ..push({'type': 'challenge', 'nonce': 'n', 'relayId': 'opaque-id'})
            ..push({'type': 'error', 'code': 'grant', 'who': 'phone', 'peer': 'me'}),
        ),
      ],
    );
    addTearDown(c.dispose);
    final phase = await phaseOf(c);
    for (var i = 0; i < 100 && c.read(activityLogProvider).isEmpty; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 20));
    }
    final log = c.read(activityLogProvider);
    expect(log.map((e) => (e.kind, e.detail)), [(LogKind.pushRefused, 'opaque-id')]);
    await (phase as Paired).link.dispose();
  });

  test('erase and pair again erases, then opens the scanner', () async {
    await storePairing();
    final c = container();
    await phaseOf(c);
    await c.read(linkControllerProvider.notifier).eraseAndPairAgain();
    expect(c.read(linkControllerProvider).value, isA<Unpaired>());
    expect(await LinkStateStore(tmp).exists(), isFalse);
    expect(c.read(pairingControllerProvider).step, PairStep.scan);
  });

  test('after the signing key is lost, erase and pair again leaves nothing that refuses the next pairing', () async {
    await storePairing();
    final c = container();
    final link = (await phaseOf(c) as Paired).link;
    link.computer.pending.add('late-command');
    c.read(linkControllerProvider.notifier).keyLost();
    expect(c.read(linkControllerProvider).value, isA<PairAgain>());
    await c.read(linkControllerProvider.notifier).eraseAndPairAgain();
    // the old link's late work (a result that arrives, the app coming back) writes nothing and connects nowhere
    await link.forget('late-command');
    link.resume();
    await Future<void>.delayed(const Duration(milliseconds: 50));
    expect(link.connected, isFalse);
    expect(await LinkStateStore(tmp).exists(), isFalse, reason: 'a new pairing checks this file');
    expect(await keys.secret(SecureKeys.auth), isNull, reason: 'and this key');
    expect(c.read(pairingControllerProvider).step, PairStep.scan);
  });

  LogEntry old(String session) => LogEntry(
    at: DateTime(2026, 11, 13, 20, 35),
    kind: LogKind.allow,
    sessionId: session,
    project: 'p',
    detail: 'npm test',
  );

  test('disconnect clears the log of that computer and leaves one disconnected line', () async {
    await storePairing();
    final c = container();
    await phaseOf(c);
    c.read(activityLogProvider.notifier)
      ..record(old('s1'))
      ..record(old('s2'));
    await c.read(linkControllerProvider.notifier).disconnect();
    final log = c.read(activityLogProvider);
    expect(log, hasLength(1));
    expect(log.single.kind, LogKind.disconnect);
    expect(log.single.detail, 'Studio Mac (demo)');
    expect(c.read(taskLogProvider('s1')), isEmpty);
    expect(c.read(appLogProvider), hasLength(1));
  });

  test('erase and pair again does the same', () async {
    await storePairing();
    final c = container();
    await phaseOf(c);
    c.read(activityLogProvider.notifier).record(old('s1'));
    await c.read(linkControllerProvider.notifier).eraseAndPairAgain();
    expect(c.read(activityLogProvider).map((e) => e.kind), [
      LogKind.disconnect,
    ]);
  });

  test('a pairing that just ended well writes one paired line', () async {
    final c = container();
    await phaseOf(c);
    await storePairing();
    final stored = (await LinkStateStore(tmp).load())!;
    await c.read(linkControllerProvider.notifier).paired(stored);
    final log = c.read(activityLogProvider);
    expect(log.map((e) => e.kind), [LogKind.pair]);
    expect(log.single.detail, 'Studio Mac (demo)');
    final phase = c.read(linkControllerProvider).value;
    if (phase is Paired) await phase.link.dispose();
  });
}
