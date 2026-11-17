import 'dart:async';

import 'package:flutter/foundation.dart';

import 'push_platform.dart';

/// Sends one relay frame (its JSON) over the connection the pairing code owns.
typedef FrameSink = Future<void> Function(Map<String, Object?> frame);

/// Registers this phone's push token with the relays it is connected to, each time with a grant signed by the `auth`
/// key (no biometrics). The relay and Apple see only the token and the grant; a notice is sealed.
///
/// A grant is made at every connection and renewed while the app runs, a week before it ends. It is renewed only while
/// the app runs: after 30 days without opening it the relay stops pushing, and the computer says so in Settings.
/// Nothing here is logged: a token is a credential.
class PushRegistration {
  PushRegistration(
    this._platform, {
    DateTime Function()? now,
    String? platform,
    this.validity = const Duration(days: 29),
    this.renewWithin = const Duration(days: 7),
  }) : _now = now ?? DateTime.now,
       _kind =
           platform ??
           (defaultTargetPlatform == TargetPlatform.android
               ? 'android'
               : 'ios');

  final PushPlatform _platform;

  /// What the relay is told this token is: `ios` (APNs) or `android` (FCM).
  final String _kind;
  final DateTime Function() _now;

  /// How long a grant lasts (the relay accepts up to 30 days).
  final Duration validity;

  /// A grant that ends within this is renewed by [renewIfDue].
  final Duration renewWithin;

  final _links = <String, _Link>{};
  final _extra = <String, ({String kind, String? ref, String token})>{};
  StreamSubscription<String>? _tokens;
  Timer? _timer;

  /// Call once at start. Registers with APNs again when the person already allowed notifications, so a token the
  /// system changed reaches [connected] links, and renews grants every day while the app runs.
  Future<void> start() async {
    _tokens ??= _platform.tokens.listen((t) {
      for (final id in _links.keys.toList()) {
        unawaited(_register(id, t));
      }
    });
    _timer ??= Timer.periodic(const Duration(days: 1), (_) => renewIfDue());
    if (await _platform.permissionStatus() == PushPermission.authorized) {
      await _platform.register();
    }
  }

  void dispose() {
    _tokens?.cancel();
    _timer?.cancel();
  }

  /// After pairing: asks for permission (the system dialog, once; the screen that calls this shows the reason first)
  /// and registers. Refusing leaves the app usable; [PushPermission.denied] means offer [openSystemSettings].
  Future<PushPermission> requestPermissionAndRegister() async {
    var status = await _platform.permissionStatus();
    if (status == PushPermission.notDetermined) {
      status = await _platform.requestPermission()
          ? PushPermission.authorized
          : PushPermission.denied;
    } else if (status == PushPermission.authorized) {
      await _platform.register();
    }
    return status;
  }

  /// Whether notifications are allowed, refused or not asked yet.
  Future<PushPermission> permissionStatus() => _platform.permissionStatus();

  Future<void> openSystemSettings() => _platform.openSettings();

  /// A token of a Live Activity (`activity`, [ref] is the activity's id) or the token that lets a push start one
  /// (`pushToStart`). Each is registered with a grant like the device token, on every connected relay, and again on every
  /// connection and renewal. A later token of the same kind and [ref] takes the place of the earlier one.
  Future<void> registerExtra({required String kind, String? ref, required String token}) async {
    _extra['$kind|${ref ?? ''}'] = (kind: kind, ref: ref, token: token);
    for (final id in _links.keys.toList()) {
      await _sendExtras(id);
    }
  }

  /// The activity ended: its token is not registered again.
  void clearExtra(String kind, {String? ref}) => _extra.remove('$kind|${ref ?? ''}');

  Future<void> _sendExtras(String relayId) async {
    final send = _links[relayId]?.send;
    if (send == null) return;
    for (final e in _extra.values.toList()) {
      try {
        final grant = await _platform.grant(
          token: e.token,
          relayId: relayId,
          expiresMs: _now().add(validity).millisecondsSinceEpoch,
        );
        await send({
          'type': 'pushToken',
          'platform': _kind,
          'env': await _platform.environment(),
          'kind': e.kind,
          if (e.ref != null) 'ref': e.ref,
          'token': e.token,
          'grant': grant,
        });
      } catch (_) {
        // Tried again with the next connection.
      }
    }
  }

  /// A connection to [relayId] is up: register (again). [relayId] is the id the relay's grants name (`RELAY_ID`, the
  /// host the phone registered with). With no token yet, the registration happens when the system hands one over.
  Future<void> connected(String relayId, FrameSink send) async {
    _links[relayId] = _Link(send);
    await _register(relayId, await _platform.token());
  }

  /// The connection dropped: nothing is sent until [connected] again, which registers again.
  void disconnected(String relayId) => _links[relayId]?.send = null;

  /// The computer was disconnected or revoked: stop renewing for [relayId]. The relay has no frame to remove a token;
  /// it forgets it when the pair is cut or the grant ends.
  void forget(String relayId) => _links.remove(relayId);

  /// Registers again where the grant ends soon. Called daily by the timer; a test or a resume can call it.
  Future<void> renewIfDue() async {
    final soon = _now().add(renewWithin);
    for (final e in _links.entries.toList()) {
      final ends = e.value.ends;
      if (ends == null || ends.isBefore(soon)) {
        await _register(e.key, e.value.token ?? await _platform.token());
      }
    }
  }

  Future<void> _register(String relayId, String? token) async {
    final link = _links[relayId];
    final send = link?.send;
    if (link == null || send == null || token == null) return;
    final ends = _now().add(validity);
    try {
      final grant = await _platform.grant(
        token: token,
        relayId: relayId,
        expiresMs: ends.millisecondsSinceEpoch,
      );
      await send({
        'type': 'pushToken',
        'platform': _kind,
        'env': await _platform.environment(),
        'kind': 'device',
        'token': token,
        'grant': grant,
      });
      link
        ..token = token
        ..ends = ends;
      await _sendExtras(relayId);
    } catch (_) {
      // No network, or the connection dropped under it: the next `connected` or daily renewal tries again.
      link.ends = null;
    }
  }
}

class _Link {
  _Link(this.send);
  FrameSink? send;
  String? token;
  DateTime? ends;
}
