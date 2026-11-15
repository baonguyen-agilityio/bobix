import 'dart:typed_data';

import 'package:phone/keys/secure_keys.dart';

/// The generator point of P-256 (SEC1, uncompressed): a valid public key for a pairing's `hello`. The tests never sign.
final _publicKey = Uint8List.fromList([
  0x04, 0x6B, 0x17, 0xD1, 0xF2, 0xE1, 0x2C, 0x42, 0x47, 0xF8, 0xBC, 0xE6, //
  0xE5, 0x63, 0xA4, 0x40, 0xF2, 0x77, 0x03, 0x7D, 0x81, 0x2D, 0xEB, 0x33,
  0xA0, 0xF4, 0xA1, 0x39, 0x45, 0xD8, 0x98, 0xC2, 0x96, 0x4F, 0xE3, 0x42,
  0xE2, 0xFE, 0x1A, 0x7F, 0x9B, 0x8E, 0xE7, 0xEB, 0x4A, 0x7C, 0x0F, 0x9E,
  0x16, 0x2B, 0xCE, 0x33, 0x57, 0x6B, 0x31, 0x5E, 0xCE, 0xCB, 0xB6, 0x40,
  0x68, 0x37, 0xBF, 0x51, 0xF5,
]);

/// [SecureKeys] in memory, for tests of everything that does not need the secure hardware.
class MemorySecureKeys implements SecureKeys {
  MemorySecureKeys({this.biometrics = true});

  /// What [strongBiometrics] answers.
  bool biometrics;
  final secrets = <String, Uint8List>{};
  bool hasSigningKey = false;
  int signCalls = 0;
  bool erased = false;

  @override
  Future<void> putSecret(String name, Uint8List data) async =>
      secrets[name] = data;

  @override
  Future<Uint8List?> secret(String name) async => secrets[name];

  @override
  Future<void> deleteSecret(String name) async => secrets.remove(name);

  @override
  Future<bool> strongBiometrics() async => biometrics;

  @override
  Future<Uint8List> createSigningKey() async {
    hasSigningKey = true;
    return _publicKey;
  }

  @override
  Future<Uint8List?> signingPublicKey() async =>
      hasSigningKey ? _publicKey : null;

  @override
  Future<SignResult> sign(Uint8List data, {required String reason}) async {
    signCalls++;
    return const SignCancelled();
  }

  @override
  Future<void> deleteSigningKey() async => hasSigningKey = false;

  @override
  Future<void> eraseAll() async {
    secrets.clear();
    hasSigningKey = false;
    erased = true;
  }
}
