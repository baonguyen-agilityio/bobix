import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/phone_data.dart';
import '../link/computer_link.dart';
import '../push/push_providers.dart';
import 'surfaces_platform.dart';
import 'surfaces_sync.dart';

final surfacesPlatformProvider = Provider<SurfacesPlatform>(
  (ref) => ChannelSurfacesPlatform(),
);

final surfacesSyncProvider = Provider<SurfacesSync>(
  (ref) => SurfacesSync(ref.watch(surfacesPlatformProvider)),
);

/// Around the app while a computer is paired: every `home` goes to the widgets, the controls and the Live Activity, and
/// the activity's tokens go to the relay.
class SurfacesHost extends ConsumerStatefulWidget {
  const SurfacesHost({super.key, required this.link, required this.child});
  final ComputerLink link;
  final Widget child;

  @override
  ConsumerState<SurfacesHost> createState() => _SurfacesHostState();
}

class _SurfacesHostState extends ConsumerState<SurfacesHost> {
  final _subs = <StreamSubscription<Object?>>[];

  @override
  void initState() {
    super.initState();
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
    } catch (_) {
      // No platform (a test): nothing to register.
    }
  }

  @override
  void dispose() {
    for (final s in _subs) {
      s.cancel();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    ref.listen(homeProvider, (_, next) {
      final h = next.value;
      if (h == null) return;
      final c = widget.link.computer;
      unawaited(
        ref
            .read(surfacesSyncProvider)
            .sync(h, computerId: c.id, canStart: c.rights.start),
      );
    });
    return widget.child;
  }
}
