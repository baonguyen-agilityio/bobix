import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/keys/secure_keys.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('dev.bobix.phone/secure_keys');
  final calls = <MethodCall>[];
  Object? Function(MethodCall) reply = (_) => null;

  setUp(() {
    calls.clear();
    reply = (_) => null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
          calls.add(call);
          final r = reply(call);
          if (r is PlatformException) throw r;
          return r;
        });
  });
  tearDown(
    () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null),
  );

  final data = Uint8List.fromList([1, 2, 3]);

  test(
    'the store is opened once, with the software flag only when asked for',
    () async {
      final keys = PlatformSecureKeys(software: false);
      await keys.putSecret(SecureKeys.auth, data);
      await keys.deleteSecret(SecureKeys.box);
      expect(calls.map((c) => c.method), ['open', 'putSecret', 'deleteSecret']);
      expect(calls.first.arguments, {'software': false});
      expect(calls[1].arguments, {'name': 'auth', 'data': data});

      calls.clear();
      await PlatformSecureKeys(software: true).eraseAll();
      expect(calls.first.arguments, {'software': true});
    },
  );

  test('a secret and the signing public key come back as bytes, or null when there is none', () async {
    final keys = PlatformSecureKeys(software: false);
    reply = (c) => c.method == 'secret' ? data : null;
    expect(await keys.secret(SecureKeys.auth), data);
    expect(await keys.signingPublicKey(), isNull);
  });

  test('every outcome of a signature is a typed result, and nothing throws for a person\'s choice', () async {
    final keys = PlatformSecureKeys(software: false);
    final signature = Uint8List.fromList([9, 9]);
    final outcomes = <Map<String, Object?>, void Function(SignResult)>{
      {'result': 'signed', 'signature': signature}: (r) =>
          expect((r as Signed).signature, signature),
      {'result': 'cancelled'}: (r) => expect(r, isA<SignCancelled>()),
      {'result': 'lockedOut'}: (r) => expect(r, isA<SignLockedOut>()),
      {'result': 'unavailable'}: (r) => expect(r, isA<BiometryUnavailable>()),
      {'result': 'invalidated'}: (r) => expect(r, isA<KeyInvalidated>()),
      {'result': 'failed', 'message': 'boom'}: (r) =>
          expect((r as SignFailed).message, 'boom'),
    };
    for (final e in outcomes.entries) {
      reply = (c) => c.method == 'sign' ? e.key : null;
      e.value(await keys.sign(data, reason: 'Confirm'));
    }
    final sent = calls.lastWhere((c) => c.method == 'sign').arguments as Map;
    expect(sent['reason'], 'Confirm');
    expect(sent['data'], data);
  });

  test('a platform error is a SecureKeysException, and a signature that hit one is a failed result', () async {
    final keys = PlatformSecureKeys(software: false);
    reply = (c) => c.method == 'open'
        ? null
        : PlatformException(
            code: 'key_error',
            message: 'keychain write failed (-25299)',
          );
    await expectLater(
      keys.putSecret('auth', data),
      throwsA(
        isA<SecureKeysException>().having(
          (e) => e.message,
          'message',
          contains('-25299'),
        ),
      ),
    );
    expect(await keys.sign(data, reason: 'x'), isA<SignFailed>());
  });

  test('a failed opening is tried again on the next call', () async {
    final keys = PlatformSecureKeys(software: false);
    var opens = 0;
    reply = (c) {
      if (c.method != 'open') return null;
      return ++opens == 1
          ? PlatformException(code: 'open_failed', message: 'no marker')
          : null;
    };
    await expectLater(keys.eraseAll(), throwsA(isA<SecureKeysException>()));
    await keys.eraseAll();
    expect(opens, 2);
  });

  test('a platform without the channel says so', () async {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null);
    await expectLater(
      PlatformSecureKeys(software: false).eraseAll(),
      throwsA(isA<SecureKeysException>()),
    );
  });
}
