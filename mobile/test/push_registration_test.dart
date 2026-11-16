import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/push/push_platform.dart';
import 'package:phone/push/push_registration.dart';

import 'support/fake_push_platform.dart';

FakePushPlatform platform_() => FakePushPlatform()..currentToken = 'ab12cd';

void main() {
  late FakePushPlatform platform;
  late DateTime clock;
  late PushRegistration reg;
  late List<Map<String, Object?>> sent;

  Future<void> send(Map<String, Object?> f) async => sent.add(f);

  setUp(() {
    platform = FakePushPlatform()..currentToken = 'ab12cd';
    clock = DateTime.utc(2026, 11, 15);
    sent = [];
    reg = PushRegistration(platform, now: () => clock, platform: 'ios');
  });
  tearDown(() => reg.dispose());

  test('a connection registers the token with the build\'s environment and a grant for that relay', () async {
    await reg.connected('relay.example.test', send);
    expect(sent, hasLength(1));
    final f = sent.single;
    expect(f['type'], 'pushToken');
    expect(f['platform'], 'ios');
    expect(f['kind'], 'device');
    expect(f['env'], 'sandbox');
    expect(f['token'], 'ab12cd');
    final ends = clock.add(const Duration(days: 29)).millisecondsSinceEpoch;
    expect(f['grant'], 'grant-ab12cd-relay.example.test-$ends');
    expect(ends - clock.millisecondsSinceEpoch, lessThan(30 * 24 * 3600 * 1000), reason: 'inside what the relay accepts');
  });

  test('a production build registers as production', () async {
    platform.env = 'production';
    await reg.connected('r', send);
    expect(sent.single['env'], 'production');
  });

  test('without a token nothing is sent until the system hands one over, then it is', () async {
    platform.currentToken = null;
    await reg.start();
    await reg.connected('r', send);
    expect(sent, isEmpty);
    platform.tokenController.add('ff00');
    await Future<void>.delayed(Duration.zero);
    await Future<void>.delayed(Duration.zero);
    expect(sent.map((f) => f['token']), ['ff00']);
  });

  test('a changed token registers again for every connected relay', () async {
    await reg.start();
    final other = <Map<String, Object?>>[];
    await reg.connected('a', send);
    await reg.connected('b', (f) async => other.add(f));
    platform.tokenController.add('new1');
    await Future<void>.delayed(Duration.zero);
    await Future<void>.delayed(Duration.zero);
    expect(sent.last['token'], 'new1');
    expect(other.last['token'], 'new1');
  });

  test('a grant is renewed before it ends and not before', () async {
    await reg.connected('r', send);
    clock = clock.add(const Duration(days: 10));
    await reg.renewIfDue();
    expect(sent, hasLength(1), reason: '19 days are left: nothing to do');
    clock = clock.add(const Duration(days: 13));
    await reg.renewIfDue();
    expect(sent, hasLength(2), reason: '6 days are left: renewed');
    expect(platform.grants.last['expiresMs'], clock.add(const Duration(days: 29)).millisecondsSinceEpoch);
  });

  test('a send that failed is tried again by the next renewal and by the next connection', () async {
    var down = true;
    Future<void> flaky(Map<String, Object?> f) async {
      if (down) throw Exception('no network');
      sent.add(f);
    }

    await reg.connected('r', flaky);
    expect(sent, isEmpty);
    down = false;
    await reg.renewIfDue();
    expect(sent, hasLength(1));
  });

  test('a grant that could not be made sends nothing', () async {
    platform.grantError = Exception('no key');
    await reg.connected('r', send);
    expect(sent, isEmpty);
  });

  test('a dropped connection is not used and a new one registers again', () async {
    await reg.connected('r', send);
    reg.disconnected('r');
    clock = clock.add(const Duration(days: 25));
    await reg.renewIfDue();
    expect(sent, hasLength(1));
    await reg.connected('r', send);
    expect(sent, hasLength(2));
  });

  test('a forgotten relay is no longer renewed, the others are', () async {
    final other = <Map<String, Object?>>[];
    await reg.connected('a', send);
    await reg.connected('b', (f) async => other.add(f));
    reg.forget('a');
    clock = clock.add(const Duration(days: 25));
    await reg.renewIfDue();
    expect(sent, hasLength(1));
    expect(other, hasLength(2));
  });

  group('permission', () {
    test('asks once, after pairing, and registers when allowed', () async {
      expect(await reg.requestPermissionAndRegister(), PushPermission.authorized);
      expect(platform.calls, ['requestPermission']);
    });

    test('a refusal leaves everything usable and says so', () async {
      platform.allow = false;
      expect(await reg.requestPermissionAndRegister(), PushPermission.denied);
      expect(await reg.requestPermissionAndRegister(), PushPermission.denied);
      expect(platform.calls, ['requestPermission'], reason: 'the system dialog is not asked for again');
      await reg.openSystemSettings();
      expect(platform.calls.last, 'openSettings');
    });

    test('already allowed: registers with APNs again, no dialog', () async {
      platform.permission = PushPermission.authorized;
      await reg.requestPermissionAndRegister();
      expect(platform.calls, ['register']);
    });

    test('start registers again only when notifications are allowed', () async {
      await reg.start();
      expect(platform.calls, isEmpty);
      platform.permission = PushPermission.authorized;
      await reg.start();
      expect(platform.calls, ['register']);
    });
  });

  test('a tap carries the computer, the request and the kind', () {
    final t = PushTap.tryParse({'computer': 'c', 'ref': 'r1', 'kind': 'waiting'});
    expect(t, const PushTap(computerId: 'c', ref: 'r1', kind: 'waiting'));
  });

  test('a tap that is not a well-formed route opens nothing', () {
    for (final bad in <Object?>[
      null,
      'x',
      {'computer': 'c', 'ref': 'r'},
      {'computer': 'c', 'ref': 'r', 'kind': 'answer'},
      {'computer': '', 'ref': 'r', 'kind': 'waiting'},
      {'computer': 'c', 'ref': 'r' * 300, 'kind': 'waiting'},
      {'computer': 1, 'ref': 'r', 'kind': 'waiting'},
    ]) {
      expect(PushTap.tryParse(bad), isNull, reason: '$bad');
    }
  });

  group('platform', () {
    tearDown(() => debugDefaultTargetPlatformOverride = null);

    test('an Android phone registers as android, an iPhone as ios', () async {
      debugDefaultTargetPlatformOverride = TargetPlatform.android;
      final a = PushRegistration(platform_(), now: () => clock);
      final out = <Map<String, Object?>>[];
      await a.connected('r', (f) async => out.add(f));
      a.dispose();
      expect(out.single['platform'], 'android');
      debugDefaultTargetPlatformOverride = TargetPlatform.iOS;
      final i = PushRegistration(platform_(), now: () => clock);
      await i.connected('r', (f) async => out.add(f));
      i.dispose();
      expect(out.last['platform'], 'ios');
    });

    test('the platform can be given', () async {
      final r = PushRegistration(platform_(), now: () => clock, platform: 'android');
      final out = <Map<String, Object?>>[];
      await r.connected('r', (f) async => out.add(f));
      r.dispose();
      expect(out.single['platform'], 'android');
    });
  });
}
