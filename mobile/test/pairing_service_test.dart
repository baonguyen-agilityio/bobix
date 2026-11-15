// What a pairing does to the keys and the stored state when it does not succeed, or must not start.
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/config/flavor.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/link/pairing_service.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/src/rust/api/messages.dart';

import 'support/core_library.dart';
import 'support/memory_secure_keys.dart';

String key(int fill) => b64(Uint8List.fromList(List.filled(32, fill)));

String code() =>
    'bobix://pair?r=${Uri.encodeComponent('ws://127.0.0.1:1')}&a=${key(7)}&b=${key(9)}&t=${key(5)}';

const rights = PhoneRights(
  answer: true,
  plan: true,
  question: true,
  start: true,
  stop: true,
  activity: true,
);

void main() {
  late Directory tmp;
  late MemorySecureKeys keys;
  late LinkStateStore store;
  setUpAll(initCoreLibrary);
  setUp(() {
    tmp = Directory.systemTemp.createTempSync('bobix-pairing-');
    keys = MemorySecureKeys();
    store = LinkStateStore(tmp);
  });
  tearDown(() => tmp.deleteSync(recursive: true));

  PairingService service() => PairingService(
    keys: keys,
    store: store,
    connect: (_) async => throw const SocketException('refused'),
    flavor: Flavor.dev,
  );

  Future<Object?> failure() => service()
      .start(PairRequest(uri: code(), phoneName: 'p', rights: rights))
      .result
      .then<Object?>((_) => null, onError: (Object e) => e);

  test('a failed attempt removes the signing key it made', () async {
    final e = await failure();
    expect((e! as PairException).failure, PairFailure.relayUnreachable);
    expect(keys.hasSigningKey, isFalse);
  });

  test('a failed attempt keeps a signing key it did not make', () async {
    await keys.createSigningKey();
    final e = await failure();
    expect((e! as PairException).failure, PairFailure.relayUnreachable);
    expect(
      keys.hasSigningKey,
      isTrue,
      reason: 'the key was there before: not this attempt\'s to delete',
    );
  });

  test('pairing never starts over stored state or stored keys', () async {
    File(
      '${tmp.path}/link-state.json',
    ).writeAsStringSync('anything: even an unreadable file may be a pairing');
    await keys.createSigningKey();
    var e = await failure();
    expect((e! as PairException).failure, PairFailure.alreadyPaired);
    expect(keys.hasSigningKey, isTrue);
    expect(
      File('${tmp.path}/link-state.json').readAsStringSync(),
      contains('anything'),
    );

    File('${tmp.path}/link-state.json').deleteSync();
    await keys.putSecret(SecureKeys.auth, Uint8List(32));
    e = await failure();
    expect((e! as PairException).failure, PairFailure.alreadyPaired);
    expect(
      keys.secrets[SecureKeys.auth],
      Uint8List(32),
      reason: 'its keys are not replaced',
    );
  });

  test(
    'keys that cannot be read for now: no pairing, and nothing changes',
    () async {
      keys.failReads = true;
      final e = await failure();
      expect((e! as PairException).failure, PairFailure.keyFailed);
      expect(keys.hasSigningKey, isFalse);
    },
  );
}
