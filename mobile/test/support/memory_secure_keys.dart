import 'dart:typed_data';

import 'package:pointycastle/export.dart';
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
  MemorySecureKeys({this.biometrics = true, this.lockedOut = false});

  /// What [biometry] answers.
  bool biometrics;
  bool lockedOut;
  final secrets = <String, Uint8List>{};
  bool hasSigningKey = false;
  int signCalls = 0;
  bool erased = false;

  /// When set, reading a secret fails as the Keychain does while the device is locked: "could not read", not "not found".
  bool failReads = false;

  @override
  Future<void> putSecret(String name, Uint8List data) async =>
      secrets[name] = data;

  @override
  Future<Uint8List?> secret(String name) async {
    if (failReads) {
      throw const SecureKeysException('keychain read failed (-25308)');
    }
    return secrets[name];
  }

  @override
  Future<void> deleteSecret(String name) async => secrets.remove(name);

  /// What [authenticate] answers, and the reasons it was asked with.
  AppAuth authAnswer = AppAuth.ok;
  final authReasons = <String>[];

  @override
  Future<AppAuth> authenticate({required String reason}) async {
    authReasons.add(reason);
    return authAnswer;
  }

  @override
  Future<Biometry> biometry() async => lockedOut
      ? Biometry.lockedOut
      : biometrics
      ? Biometry.available
      : Biometry.none;

  @override
  Future<Uint8List> createSigningKey() async {
    hasSigningKey = true;
    return _publicKey;
  }

  @override
  Future<Uint8List?> signingPublicKey() async =>
      hasSigningKey ? _publicKey : null;

  /// What [sign] answers; null signs for real with the private key 1, whose public key is [_publicKey] (the generator
  /// point), so a computer that verifies the signature accepts it.
  SignResult? signAnswer;

  /// The reasons [sign] was asked with, in order.
  final signReasons = <String>[];

  @override
  Future<SignResult> sign(Uint8List data, {required String reason}) async {
    signCalls++;
    signReasons.add(reason);
    return signAnswer ?? Signed(_sign(data));
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

/// ECDSA P-256 over SHA-256 with the private key 1, as an X9.62 DER signature.
Uint8List _sign(Uint8List data) {
  final curve = ECCurve_secp256r1();
  final signer = ECDSASigner(SHA256Digest(), HMac(SHA256Digest(), 64))
    ..init(true, PrivateKeyParameter(ECPrivateKey(BigInt.one, curve)));
  final sig = signer.generateSignature(data) as ECSignature;
  Uint8List integer(BigInt v) {
    var bytes = <int>[];
    var x = v;
    while (x > BigInt.zero) {
      bytes.insert(0, (x & BigInt.from(0xff)).toInt());
      x >>= 8;
    }
    if (bytes.isEmpty || bytes.first & 0x80 != 0) bytes = [0, ...bytes];
    return Uint8List.fromList([0x02, bytes.length, ...bytes]);
  }

  final body = [...integer(sig.r), ...integer(sig.s)];
  return Uint8List.fromList([0x30, body.length, ...body]);
}
