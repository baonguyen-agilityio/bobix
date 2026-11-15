// Runs the real Swift key store on the iOS simulator, through the platform channel (the software signing key: the
// simulator cannot make a Secure Enclave key with biometrics):
//   flutter test integration_test/secure_keys_test.dart -d <simulator>
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/keys/secure_keys.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('secrets and the signing key work through the channel', (
    tester,
  ) async {
    final keys = PlatformSecureKeys(software: true);
    await keys.eraseAll();
    expect(await keys.secret(SecureKeys.auth), isNull);

    await keys.putSecret(SecureKeys.auth, Uint8List.fromList([1, 2, 3]));
    expect(await keys.secret(SecureKeys.auth), [1, 2, 3]);

    expect(await keys.signingPublicKey(), isNull);
    final publicKey = await keys.createSigningKey();
    expect(publicKey.length, 65);
    expect(await keys.signingPublicKey(), publicKey);

    final result = await keys.sign(
      Uint8List.fromList([7, 7, 7]),
      reason: 'Confirm this command',
    );
    expect(result, isA<Signed>());
    expect(
      (result as Signed).signature.length,
      inInclusiveRange(68, 72),
      reason: 'an X9.62 DER ECDSA signature',
    );

    await keys.deleteSigningKey();
    expect(await keys.sign(Uint8List(1), reason: 'x'), isA<KeyInvalidated>());

    await keys.eraseAll();
    expect(await keys.secret(SecureKeys.auth), isNull);
  });
}
