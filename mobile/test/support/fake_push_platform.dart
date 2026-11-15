import 'dart:async';
import 'dart:typed_data';

import 'package:phone/push/push_platform.dart';

/// A [PushPlatform] that records what the app asked and answers from fields.
class FakePushPlatform implements PushPlatform {
  PushPermission permission = PushPermission.notDetermined;
  bool allow = true;
  String? currentToken;
  String env = 'sandbox';
  bool hide = false;
  final calls = <String>[];
  final grants = <Map<String, Object?>>[];
  final tokenController = StreamController<String>.broadcast();
  final tapController = StreamController<PushTap>.broadcast();
  PushTap? initial;
  Object? grantError;

  @override
  Future<PushPermission> permissionStatus() async => permission;

  @override
  Future<bool> requestPermission() async {
    calls.add('requestPermission');
    permission = allow ? PushPermission.authorized : PushPermission.denied;
    return allow;
  }

  @override
  Future<void> register() async => calls.add('register');

  @override
  Future<String?> token() async => currentToken;

  @override
  Stream<String> get tokens => tokenController.stream;

  @override
  Future<String> environment() async => env;

  @override
  Future<String> grant({
    required String token,
    required String relayId,
    required int expiresMs,
  }) async {
    if (grantError != null) throw grantError!;
    grants.add({'token': token, 'relayId': relayId, 'expiresMs': expiresMs});
    return 'grant-$token-$relayId-$expiresMs';
  }

  @override
  Stream<PushTap> get taps => tapController.stream;

  @override
  Future<PushTap?> initialTap() async {
    final t = initial;
    initial = null;
    return t;
  }

  @override
  Future<void> openSettings() async => calls.add('openSettings');

  @override
  Future<void> clearDelivered() async => calls.add('clearDelivered');

  @override
  Future<void> setComputer({
    required String id,
    required Uint8List boxPublic,
    int versionMin = 1,
    int versionMax = 1,
  }) async => calls.add('setComputer $id ${boxPublic.length} $versionMin-$versionMax');

  @override
  Future<void> removeComputer(String id) async => calls.add('removeComputer $id');

  @override
  Future<OpenedPacket> openPacket(Uint8List packet) async =>
      const PacketRefused('replayed');

  @override
  Future<bool> hideDetails() async => hide;

  @override
  Future<void> setHideDetails(bool h) async => hide = h;
}
