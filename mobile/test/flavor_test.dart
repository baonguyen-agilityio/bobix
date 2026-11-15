import 'package:flutter_test/flutter_test.dart';
import 'package:phone/config/flavor.dart';

import 'support/core_library.dart';

void main() {
  setUpAll(initCoreLibrary);

  test('prod refuses a plain connection, even to this computer; dev allows it only there', () async {
    const plainLocal = [
      'ws://localhost:8787',
      'ws://127.0.0.1:8787',
      'ws://[::1]:8787',
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
        reason: 'dev $url: plain only to localhost',
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
