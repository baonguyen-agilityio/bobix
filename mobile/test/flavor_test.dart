import 'package:flutter_test/flutter_test.dart';
import 'package:phone/config/flavor.dart';
import 'package:phone/src/rust/api/core.dart';

import 'support/core_library.dart';

void main() {
  setUpAll(initCoreLibrary);

  test('prod refuses a plain connection, even to this computer; dev allows it only on this computer and the home network', () async {
    const plainLocal = [
      'ws://localhost:8787',
      'ws://127.0.0.1:8787',
      'ws://[::1]:8787',
      'ws://192.168.1.20:8787',
      'ws://10.0.0.5:8787',
      'ws://172.16.4.2:8787',
      'ws://[fd12:3456::1]:8787',
    ];
    for (final url in plainLocal) {
      expect(
        await relayAllowed(url, Flavor.prod),
        isFalse,
        reason: 'prod $url',
      );
      expect(await relayAllowed(url, Flavor.dev), isTrue, reason: 'dev $url');
    }
    for (final url in [
      'ws://relay.example',
      'ws://relay.example:8787',
      'ws://203.0.113.5:8787',
      'ws://192.168.1.20.evil.example:8787',
      'ws://192.168.1.20@evil.example',
      'ws://mac.local:8787',
      'ws://localhost@evil.example',
      'http://relay.example',
    ]) {
      expect(
        await relayAllowed(url, Flavor.prod),
        isFalse,
        reason: 'prod $url',
      );
      expect(
        await relayAllowed(url, Flavor.dev),
        isFalse,
        reason: 'dev $url: plain only to this computer or a private address',
      );
    }
  });

  test(
    'both flavors accept a TLS relay, and refuse what the shared crate refuses',
    () async {
      for (final f in Flavor.values) {
        expect(
          await relayAllowed('wss://relay.example/ws', f),
          isTrue,
          reason: f.name,
        );
        expect(
          await relayAllowed('WSS://Relay.Example:443/', f),
          isTrue,
          reason: '${f.name}: case and default port',
        );
        for (final bad in [
          'wss://a@b',
          'wss://relay.example:99999',
          'wss://',
          'wss://relay.exa\tmple',
          'wss://RÉLAY.example',
          'not a url',
          '',
        ]) {
          expect(
            await relayAllowed(bad, f),
            isFalse,
            reason: '${f.name} "$bad"',
          );
        }
      }
    },
  );

  test('a pairing code for a relay on the home network is read, and only dev may use it', () async {
    // The crate's test-vector keys with the relay address replaced by a private-network one.
    const keys =
        '&a=iojj3XQJ8ZX9UtstPLpdcspnCb8dlBIb83SIAbQPb1w&b=zo060cy2M-x7cMF4FKXHbs0CloUFDTRHRboFhw5YfVk&t=BgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgY';
    final info = await parsePairingCode(
      uri: 'bobix://pair?r=ws%3A%2F%2F192.168.1.20%3A8787$keys',
    );
    expect(info.relay, 'ws://192.168.1.20:8787');
    expect(await relayAllowed(info.relay, Flavor.dev), isTrue);
    expect(await relayAllowed(info.relay, Flavor.prod), isFalse);
    await expectLater(
      parsePairingCode(
        uri: 'bobix://pair?r=ws%3A%2F%2F203.0.113.5%3A8787$keys',
      ),
      throwsA(CoreError.insecureRelay),
    );
  });

  test('a build without the flag, or with an unknown one, is prod', () {
    expect(flavorOf('dev'), Flavor.dev);
    expect(flavorOf('prod'), Flavor.prod);
    expect(flavorOf(''), Flavor.prod);
    expect(flavorOf('DEV'), Flavor.prod);
    expect(flavorOf('staging'), Flavor.prod);
    expect(
      const String.fromEnvironment('BOBIX_FLAVOR', defaultValue: 'prod'),
      'prod',
      reason: 'the default of a plain test run',
    );
  });
}
