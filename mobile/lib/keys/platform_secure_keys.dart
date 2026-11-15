import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'secure_keys.dart';

/// Run with `--dart-define=BOBIX_SOFTWARE_KEY=true` on the iOS simulator, which cannot make a Secure Enclave key with
/// biometric access control. The native side honours it in debug builds only and refuses it in a release build.
const _softwareKey = bool.fromEnvironment('BOBIX_SOFTWARE_KEY');

/// [SecureKeys] over the platform channel `dev.bobix.phone/secure_keys` (Swift on iOS).
class PlatformSecureKeys implements SecureKeys {
  PlatformSecureKeys({bool software = _softwareKey, MethodChannel? channel})
    : _software = software,
      _channel = channel ?? const MethodChannel('dev.bobix.phone/secure_keys') {
    assert(!(kReleaseMode && software), 'the software key must never ship');
  }

  final bool _software;
  final MethodChannel _channel;
  Future<void>? _open;

  Future<T?> _call<T>(
    String method, [
    Map<String, Object?> args = const {},
  ]) async {
    // The first call opens the store; a failed opening is tried again on the next call.
    _open ??= _channel
        .invokeMethod<void>('open', {'software': _software})
        .catchError((Object e) {
          _open = null;
          throw e;
        });
    try {
      await _open;
      return await _channel.invokeMethod<T>(method, args);
    } on PlatformException catch (e) {
      throw SecureKeysException(e.message ?? e.code);
    } on MissingPluginException {
      throw const SecureKeysException(
        'the secure_keys channel is not available on this platform',
      );
    }
  }

  @override
  Future<void> putSecret(String name, Uint8List data) =>
      _call<void>('putSecret', {'name': name, 'data': data});

  @override
  Future<Uint8List?> secret(String name) =>
      _call<Uint8List>('secret', {'name': name});

  @override
  Future<void> deleteSecret(String name) =>
      _call<void>('deleteSecret', {'name': name});

  @override
  Future<Uint8List> createSigningKey() async =>
      await _call<Uint8List>('createSigningKey') ??
      (throw const SecureKeysException('no public key returned'));

  @override
  Future<Uint8List?> signingPublicKey() => _call<Uint8List>('signingPublicKey');

  @override
  Future<SignResult> sign(Uint8List data, {required String reason}) async {
    final Map<Object?, Object?>? out;
    try {
      out = await _call<Map<Object?, Object?>>('sign', {
        'data': data,
        'reason': reason,
      });
    } on SecureKeysException catch (e) {
      return SignFailed(e.message);
    }
    return switch (out?['result']) {
      'signed' => Signed(out!['signature']! as Uint8List),
      'cancelled' => const SignCancelled(),
      'lockedOut' => const SignLockedOut(),
      'unavailable' => const BiometryUnavailable(),
      'invalidated' => const KeyInvalidated(),
      _ => SignFailed('${out?['message'] ?? 'unknown'}'),
    };
  }

  @override
  Future<void> deleteSigningKey() => _call<void>('deleteSigningKey');

  @override
  Future<void> eraseAll() => _call<void>('eraseAll');
}

final secureKeysProvider = Provider<SecureKeys>((ref) => PlatformSecureKeys());
