import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/keys/secure_keys.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('dev.bobix.phone/secure_keys');
  final asked = <String>[];

  PlatformSecureKeys keysAnswering(Object? answer) {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
          if (call.method == 'authenticate') {
            asked.add((call.arguments as Map)['reason'] as String);
          }
          if (answer is PlatformException) throw answer;
          return call.method == 'open' ? null : answer;
        });
    return PlatformSecureKeys(channel: channel);
  }

  tearDown(() {
    asked.clear();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null);
  });

  test('the platform\'s words become the four answers of the app lock, and the reason is passed', () async {
    for (final (word, expected) in [
      ('ok', AppAuth.ok),
      ('cancelled', AppAuth.cancelled),
      ('unavailable', AppAuth.unavailable),
      ('failed', AppAuth.failed),
      ('something new', AppAuth.failed),
    ]) {
      expect(
        await keysAnswering(word).authenticate(reason: 'to open Bobix'),
        expected,
        reason: word,
      );
    }
    expect(asked, everyElement('to open Bobix'));
  });

  test('a platform that errors is a failed check, never an open app', () async {
    expect(
      await keysAnswering(PlatformException(code: 'key_error', message: 'x'))
          .authenticate(reason: 'r'),
      AppAuth.failed,
    );
  });
}
