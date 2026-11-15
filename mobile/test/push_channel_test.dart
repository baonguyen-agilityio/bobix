
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/push/push_platform.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('dev.bobix.phone/push');
  late List<MethodCall> calls;
  Object? Function(MethodCall) answer = (_) => null;

  setUp(() {
    calls = [];
    answer = (_) => null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
          calls.add(call);
          return answer(call);
        });
  });
  tearDown(
    () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null),
  );

  test('the calls carry the names and arguments Swift reads', () async {
    final p = ChannelPushPlatform();
    answer = (c) => switch (c.method) {
      'permissionStatus' => 'denied',
      'requestPermission' => true,
      'token' => 'aa',
      'environment' => 'sandbox',
      'grant' => 'g',
      'hideDetails' => true,
      _ => null,
    };
    expect(await p.permissionStatus(), PushPermission.denied);
    expect(await p.requestPermission(), isTrue);
    expect(await p.token(), 'aa');
    expect(await p.environment(), 'sandbox');
    expect(await p.grant(token: 't', relayId: 'r', expiresMs: 5), 'g');
    expect(calls.last.arguments, {'token': 't', 'relayId': 'r', 'expiresMs': 5});
    await p.setComputer(id: 'c', boxPublic: Uint8List(32), versionMin: 1, versionMax: 2);
    expect(calls.last.method, 'setComputer');
    expect((calls.last.arguments as Map)['versionMax'], 2);
    await p.removeComputer('c');
    expect(calls.last.arguments, {'id': 'c'});
    expect(await p.hideDetails(), isTrue);
    await p.setHideDetails(true);
    expect(calls.last.arguments, {'hide': true});
  });

  test('an opened packet and a refused one come back as what they are', () async {
    final p = ChannelPushPlatform();
    answer = (_) => {
      'computer': 'c',
      'stream': 'event',
      'counter': 7,
      'plaintext': Uint8List.fromList([1, 2]),
    };
    final ok = await p.openPacket(Uint8List(3)) as PacketOpened;
    expect((ok.computerId, ok.stream, ok.counter), ('c', 'event', 7));
    expect(ok.plaintext.toList(), [1, 2]);
    answer = (_) => {'error': 'replayed'};
    expect((await p.openPacket(Uint8List(3)) as PacketRefused).why, 'replayed');
  });

  test('a token and a tap from the system reach the streams, and the cold-start tap is taken once', () async {
    final p = ChannelPushPlatform();
    final tokens = <String>[];
    final taps = <PushTap>[];
    p.tokens.listen(tokens.add);
    p.taps.listen(taps.add);
    Future<void> fromNative(String method, Object? args) async =>
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.handlePlatformMessage(
          channel.name,
          channel.codec.encodeMethodCall(MethodCall(method, args)),
          (_) {},
        );
    await fromNative('token', 'beef');
    await fromNative('tap', {'computer': 'c', 'ref': 'r', 'kind': 'closed'});
    await fromNative('tap', {'computer': 'c', 'ref': 'r', 'kind': 'anything'});
    await Future<void>.delayed(Duration.zero);
    expect(tokens, ['beef']);
    expect(taps.single, const PushTap(computerId: 'c', ref: 'r', kind: 'closed'));
    answer = (_) => {'computer': 'c', 'ref': 'r', 'kind': 'waiting'};
    expect(await p.initialTap(), const PushTap(computerId: 'c', ref: 'r', kind: 'waiting'));
    answer = (_) => null;
    expect(await p.initialTap(), isNull);
  });
}
