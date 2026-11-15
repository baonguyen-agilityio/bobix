// Which screen the app opens on, from what is stored: a read that fails for now is never "the pairing is gone".
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/link/link_providers.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/src/rust/api/core.dart';
import 'package:phone/src/rust/api/messages.dart';

import 'support/core_library.dart';
import 'support/memory_secure_keys.dart';

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

  ProviderContainer container() {
    final c = ProviderContainer(
      overrides: [
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
}
