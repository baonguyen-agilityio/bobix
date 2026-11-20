import 'dart:async';

import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/models.dart';
import '../data/phone_data.dart';
import '../link/computer_link.dart';
import '../link/link_providers.dart';
import '../push/hide_details.dart';
import '../push/push_providers.dart';
import '../src/rust/api/messages.dart' show PhoneRights;
import 'surfaces_platform.dart';
import 'surfaces_sync.dart';

final surfacesPlatformProvider = Provider<SurfacesPlatform>(
  (ref) => ChannelSurfacesPlatform(),
);

/// How often the list is written again while the computer is reachable, so that content never reaches the 15 minutes after
/// which every surface draws it stale just because nothing changed. Well under 15 minutes.
final surfaceHeartbeatProvider = Provider<Duration>((ref) => const Duration(minutes: 5));

final surfacesSyncProvider = Provider<SurfacesSync>(
  (ref) => SurfacesSync(ref.watch(surfacesPlatformProvider)),
);

/// Nothing of a computer stays on the widgets, the Lock Screen or in the Dynamic Island once the pairing is gone
/// (Disconnect, Erase and pair again, a `revoked` from the computer): entering "not paired" or "pair again" clears them.
/// Watched from the root of the app, which stays mounted when the paired screens go.
final surfacesClearProvider = Provider<void>((ref) {
  ref.listen(linkControllerProvider, (prev, next) {
    bool gone(AppPhase? p) => p is Unpaired || p is PairAgain;
    if (gone(next.value) && !gone(prev?.value)) {
      ref.read(surfacesSyncProvider).reset();
      unawaited(ref.read(surfacesPlatformProvider).clear().catchError((Object _) {}));
      // A notification already shown has the session's name and request: it goes with the pairing.
      unawaited(ref.read(pushPlatformProvider).clearDelivered().catchError((Object _) {}));
    }
  });
});

/// Around the app while a computer is paired: every `home` goes to the widgets, the controls and the Live Activity, and
/// the activity's tokens go to the relay.
class SurfacesHost extends ConsumerStatefulWidget {
  const SurfacesHost({super.key, required this.link, required this.child});
  final ComputerLink link;
  final Widget child;

  @override
  ConsumerState<SurfacesHost> createState() => _SurfacesHostState();
}

class _SurfacesHostState extends ConsumerState<SurfacesHost> with WidgetsBindingObserver {
  final _subs = <StreamSubscription<Object?>>[];
  Timer? _beat;
  ValueListenable<PhoneRights>? _rights;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    // A change of rights alone (the Start right) leaves the `home` equal, so nothing else would refresh the surfaces.
    _rights = widget.link.computer.rightsListenable..addListener(_rightsChanged);
    try {
      final p = ref.read(surfacesPlatformProvider);
      final reg = ref.read(pushRegistrationProvider);
      _subs.add(
        p.activityTokens.listen(
          (t) => unawaited(reg.registerExtra(kind: 'activity', ref: t.id, token: t.token)),
        ),
      );
      _subs.add(
        p.activityEnded.listen((id) => reg.clearExtra('activity', ref: id)),
      );
      _subs.add(
        p.pushToStartTokens.listen(
          (t) => unawaited(reg.registerExtra(kind: 'pushToStart', token: t)),
        ),
      );
      // The system handed these over at launch, before anything listened.
      unawaited(p.replayTokens().catchError((Object _) {}));
    } catch (_) {
      // No platform (a test): nothing to register.
    }
  }

  void _rightsChanged() {
    final c = widget.link.computer;
    unawaited(ref.read(surfacesSyncProvider).again(computerId: c.id, canStart: c.rights.start));
  }

  @override
  void dispose() {
    _rights?.removeListener(_rightsChanged);
    WidgetsBinding.instance.removeObserver(this);
    _beat?.cancel();
    for (final s in _subs) {
      s.cancel();
    }
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    ref.read(surfacesSyncProvider).visible = state == AppLifecycleState.resumed;
  }

  @override
  Widget build(BuildContext context) {
    // Hide details turned off: the names the list lost come back at once, not at the next `home`.
    ref.listen(hideDetailsProvider, (prev, next) {
      final c = widget.link.computer;
      if (prev != null) unawaited(ref.read(surfacesSyncProvider).again(computerId: c.id, canStart: c.rights.start));
    });
    ref.listen(homeProvider, (_, next) {
      final h = next.value;
      if (h == null) return;
      final c = widget.link.computer;
      final sync = ref.read(surfacesSyncProvider);
      unawaited(sync.sync(h, computerId: c.id, canStart: c.rights.start));
      _beat?.cancel();
      _beat = h.conn == ConnState.ok
          ? Timer.periodic(
              ref.read(surfaceHeartbeatProvider),
              (_) => unawaited(sync.again(computerId: c.id, canStart: c.rights.start)),
            )
          : null;
    });
    return widget.child;
  }
}
