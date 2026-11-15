import 'dart:async';

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
    this.validity = const Duration(days: 29),
    this.renewWithin = const Duration(days: 7),
  }) : _now = now ?? DateTime.now;

  final PushPlatform _platform;
  final DateTime Function() _now;

  /// How long a grant lasts (the relay accepts up to 30 days).
  final Duration validity;

  /// A grant that ends within this is renewed by [renewIfDue].
  final Duration renewWithin;

  final _links = <String, _Link>{};
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

  Future<void> openSystemSettings() => _platform.openSettings();

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
        'platform': 'ios',
        'env': await _platform.environment(),
        'kind': 'device',
        'token': token,
        'grant': grant,
      });
      link
        ..token = token
        ..ends = ends;
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
