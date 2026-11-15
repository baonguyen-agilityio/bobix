import 'dart:async';

import 'package:flutter/services.dart';

enum PushPermission { notDetermined, denied, authorized }

/// A tap on a notification: which computer, which request (the notice's `ref`) and what kind of notice.
class PushTap {
  const PushTap({
    required this.computerId,
    required this.ref,
    required this.kind,
  });

  /// A tap is input: a notification's data can come from anything that can send this phone a push, so only a
  /// well-formed route is taken. Anything else is null and opens nothing.
  static PushTap? tryParse(Object? raw) {
    if (raw is! Map) return null;
    final c = raw['computer'], r = raw['ref'], k = raw['kind'];
    if (c is! String || r is! String || k is! String) return null;
    if (c.isEmpty || r.isEmpty || c.length > 128 || r.length > 256) return null;
    if (k != 'waiting' && k != 'closed' && k != 'done') return null;
    return PushTap(computerId: c, ref: r, kind: k);
  }

  final String computerId;
  final String ref;

  /// `waiting`, `closed` or `done`.
  final String kind;

  @override
  bool operator ==(Object other) =>
      other is PushTap &&
      other.computerId == computerId &&
      other.ref == ref &&
      other.kind == kind;

  @override
  int get hashCode => Object.hash(computerId, ref, kind);
}

/// What opening a packet through the shared receive state gave.
sealed class OpenedPacket {
  const OpenedPacket();
}

class PacketOpened extends OpenedPacket {
  const PacketOpened({
    required this.computerId,
    required this.stream,
    required this.counter,
    required this.plaintext,
  });
  final String computerId;

  /// `state`, `event` or `activity`.
  final String stream;
  final int counter;
  final Uint8List plaintext;
}

/// Not opened. `replayed` is normal (the notification extension already took it): drop it quietly and never end a
/// pairing because of it.
class PacketRefused extends OpenedPacket {
  const PacketRefused(this.why);

  /// `replayed`, `version`, `unreadable` or `noKey`.
  final String why;
}

/// The push side of the platform: notification permission, APNs registration, the signed grant, taps, and the receive
/// state the app and the notification extension share. Implemented over a platform channel; tests use a fake.
abstract interface class PushPlatform {
  Future<PushPermission> permissionStatus();

  /// Asks the person (the system shows its dialog once); registers with APNs when allowed. Returns whether allowed.
  Future<bool> requestPermission();

  /// Registers with APNs again (the system hands back the same token, or a new one).
  Future<void> register();

  /// The device token as lowercase hex, or null until the system has handed one over.
  Future<String?> token();

  /// Fired when the system hands over a token (first time, or changed).
  Stream<String> get tokens;

  /// `sandbox` or `production`: the APNs environment this build is signed for.
  Future<String> environment();

  /// The grant for [token] at [relayId], signed with the `auth` key without leaving the native side.
  Future<String> grant({
    required String token,
    required String relayId,
    required int expiresMs,
  });

  Stream<PushTap> get taps;

  /// The tap that started the app, once.
  Future<PushTap?> initialTap();

  Future<void> openSettings();

  // The receive state shared with the notification extension.
  Future<void> setComputer({
    required String id,
    required Uint8List boxPublic,
    int versionMin = 1,
    int versionMax = 1,
  });
  Future<void> removeComputer(String id);
  Future<OpenedPacket> openPacket(Uint8List packet);

  Future<bool> hideDetails();
  Future<void> setHideDetails(bool hide);
}

/// [PushPlatform] over the platform channel `dev.bobix.phone/push` (Swift on iOS).
class ChannelPushPlatform implements PushPlatform {
  ChannelPushPlatform({MethodChannel? channel})
    : _channel = channel ?? const MethodChannel('dev.bobix.phone/push') {
    _channel.setMethodCallHandler(_fromNative);
  }

  final MethodChannel _channel;
  final _tokens = StreamController<String>.broadcast();
  final _taps = StreamController<PushTap>.broadcast();

  Future<Object?> _fromNative(MethodCall call) async {
    switch (call.method) {
      case 'token':
        _tokens.add(call.arguments as String);
      case 'tap':
        final t = PushTap.tryParse(call.arguments);
        if (t != null) _taps.add(t);
    }
    return null;
  }

  @override
  Stream<String> get tokens => _tokens.stream;

  @override
  Stream<PushTap> get taps => _taps.stream;

  @override
  Future<PushPermission> permissionStatus() async {
    return switch (await _channel.invokeMethod<String>('permissionStatus')) {
      'authorized' => PushPermission.authorized,
      'denied' => PushPermission.denied,
      _ => PushPermission.notDetermined,
    };
  }

  @override
  Future<bool> requestPermission() async =>
      await _channel.invokeMethod<bool>('requestPermission') ?? false;

  @override
  Future<void> register() => _channel.invokeMethod<void>('register');

  @override
  Future<String?> token() => _channel.invokeMethod<String>('token');

  @override
  Future<String> environment() async =>
      await _channel.invokeMethod<String>('environment') ?? 'production';

  @override
  Future<String> grant({
    required String token,
    required String relayId,
    required int expiresMs,
  }) async => (await _channel.invokeMethod<String>('grant', {
    'token': token,
    'relayId': relayId,
    'expiresMs': expiresMs,
  }))!;

  @override
  Future<PushTap?> initialTap() async {
    return PushTap.tryParse(
      await _channel.invokeMethod<Map<Object?, Object?>>('initialTap'),
    );
  }

  @override
  Future<void> openSettings() => _channel.invokeMethod<void>('openSettings');

  @override
  Future<void> setComputer({
    required String id,
    required Uint8List boxPublic,
    int versionMin = 1,
    int versionMax = 1,
  }) => _channel.invokeMethod<void>('setComputer', {
    'id': id,
    'boxPublic': boxPublic,
    'versionMin': versionMin,
    'versionMax': versionMax,
  });

  @override
  Future<void> removeComputer(String id) =>
      _channel.invokeMethod<void>('removeComputer', {'id': id});

  @override
  Future<OpenedPacket> openPacket(Uint8List packet) async {
    final m = (await _channel.invokeMethod<Map<Object?, Object?>>(
      'openPacket',
      {'packet': packet},
    ))!;
    final error = m['error'];
    if (error != null) return PacketRefused('$error');
    return PacketOpened(
      computerId: m['computer'] as String,
      stream: m['stream'] as String,
      counter: (m['counter'] as num).toInt(),
      plaintext: m['plaintext'] as Uint8List,
    );
  }

  @override
  Future<bool> hideDetails() async =>
      await _channel.invokeMethod<bool>('hideDetails') ?? false;

  @override
  Future<void> setHideDetails(bool hide) =>
      _channel.invokeMethod<void>('setHideDetails', {'hide': hide});
}
