import 'dart:typed_data';

/// The three keys of the app, behind one interface.
///
/// - `auth` and `box` are secrets the Rust core makes: kept on this device only, readable after the first unlock, no
///   biometrics, out of backups. [putSecret], [secret] and [deleteSecret] hold them under those names.
/// - `sign` is made in secure hardware (the Secure Enclave on iOS), cannot be exported, asks for the enrolled
///   biometrics on every [sign] and has no passcode fallback. The app only ever gets signatures and the public key.
///
/// A key from an earlier install never comes back: the platform erases every key on the first launch after an install.
abstract interface class SecureKeys {
  static const auth = 'auth';
  static const box = 'box';

  Future<void> putSecret(String name, Uint8List data);
  Future<Uint8List?> secret(String name);
  Future<void> deleteSecret(String name);

  /// What the biometrics the signing key needs are like on this phone. Pairing checks it first.
  Future<Biometry> biometry();

  /// Makes the signing key, replacing any earlier one, and returns its public key (X9.63, 65 bytes).
  Future<Uint8List> createSigningKey();

  /// The public key of the signing key, or null when there is none. Never asks for biometrics.
  Future<Uint8List?> signingPublicKey();

  /// Signs [data] (ECDSA P-256 over SHA-256, X9.62 DER). Asks for Face ID or the fingerprint; [reason] is the line the
  /// system shows. Never throws for what a person does: the result says what happened.
  Future<SignResult> sign(Uint8List data, {required String reason});

  Future<void> deleteSigningKey();

  /// Removes every key (disconnecting, or a `revoked` from the computer).
  Future<void> eraseAll();
}

/// Whether Face ID or the fingerprint can be used: [available], [lockedOut] (enrolled, locked after too many failed
/// matches: the passcode unlocks them) or [none] (not enrolled, or turned off for this app).
enum Biometry { available, lockedOut, none }

sealed class SignResult {
  const SignResult();
}

class Signed extends SignResult {
  const Signed(this.signature);
  final Uint8List signature;
}

/// The person cancelled the biometric prompt. The key is untouched.
class SignCancelled extends SignResult {
  const SignCancelled();
}

/// Too many failed matches: the person can unlock with the passcode and try again. The key is fine; do not unpair.
class SignLockedOut extends SignResult {
  const SignLockedOut();
}

/// Biometrics cannot be used right now (none enrolled, or turned off for this app in Settings). The key is not lost.
class BiometryUnavailable extends SignResult {
  const BiometryUnavailable();
}

/// The key is really gone or unusable (the enrolled biometrics changed, the passcode was removed): the phone must pair
/// again. Reported only after the platform checked that it is so; every temporary state has a result of its own.
class KeyInvalidated extends SignResult {
  const KeyInvalidated();
}

class SignFailed extends SignResult {
  const SignFailed(this.message);
  final String message;
}

/// The platform could not do what was asked (not a person's choice): a Keychain error, a missing channel.
class SecureKeysException implements Exception {
  const SecureKeysException(this.message);
  final String message;

  @override
  String toString() => 'SecureKeysException: $message';
}
