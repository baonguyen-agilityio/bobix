import 'dart:async';

import 'package:flutter/foundation.dart' show defaultTargetPlatform;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../features/actions/action_widgets.dart' show bioName;
import '../keys/platform_secure_keys.dart';
import '../keys/secure_keys.dart';
import '../l10n/l10n.dart';
import '../pets/pet_view.dart';
import '../link/link_providers.dart';
import '../settings/app_settings.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';

/// The app lock and the cover.
///
/// - **Lock.** With "Require Face ID / fingerprint to open" on (the default) and a computer paired, the app asks when it
///   comes to the foreground, before any content is drawn: while locked the child stays mounted (so the person comes back
///   to the screen they left) but is not painted, and [LockScreen] is shown. It locks again when the app leaves the foreground. Reconnecting asks nothing (the link runs on its
///   own), and every command still asks, as before.
/// - **Cover.** While the app is not active (the app switcher takes a picture of it, a system dialog is up) a plain
///   screen covers the content.
class AppLock extends ConsumerStatefulWidget {
  const AppLock({super.key, required this.child});

  final Widget child;

  @override
  ConsumerState<AppLock> createState() => _AppLockState();
}

class _AppLockState extends ConsumerState<AppLock> {
  late final AppLifecycleListener _lifecycle;
  bool _locked = true;
  bool _covered = false;
  bool _asking = false;

  /// Raised each time the app is hidden: an answer to a check that began before is stale and never unlocks.
  int _generation = 0;

  /// The check was already asked for since the app was locked: it is not asked again by itself (the button does).
  bool _askedThisLock = false;
  AppAuth? _lastAnswer;

  @override
  void initState() {
    super.initState();
    final state = WidgetsBinding.instance.lifecycleState;
    _covered = state != null && state != AppLifecycleState.resumed;
    _lifecycle = AppLifecycleListener(
      onInactive: () => setState(() => _covered = true),
      // Really in the background (not only a system prompt over the app): the next visit asks again.
      onHide: () => setState(() {
        _covered = true;
        _locked = true;
        _askedThisLock = false;
        _generation++;
      }),
      onResume: () {
        setState(() => _covered = false);
        _askIfNeeded();
      },
    );
    WidgetsBinding.instance.addPostFrameCallback((_) => _askIfNeeded());
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    super.dispose();
  }

  bool get _needed {
    final paired = ref.read(pairedComputerProvider) != null;
    final on = ref.read(settingsProvider).value?.requireUnlock ?? true;
    return paired && on;
  }

  void _askIfNeeded() {
    // Not before the settings were read: the switch may be off.
    if (!mounted || !ref.read(settingsProvider).hasValue) return;
    if (_locked && _needed && !_covered && !_askedThisLock) unawaited(_ask());
  }

  Future<void> _ask() async {
    if (_asking) return;
    _asking = true;
    _askedThisLock = true;
    final reason = ref.read(stringsProvider)('phone.lock.reason');
    final generation = _generation;
    final answer = await ref
        .read(secureKeysProvider)
        .authenticate(reason: reason);
    _asking = false;
    if (!mounted) return;
    if (generation != _generation) {
      // The app went to the background while the check was up: this answer is not for the next visit.
      _askIfNeeded();
      return;
    }
    setState(() {
      _lastAnswer = answer;
      // A phone with nothing to ask with (no passcode, no biometrics) cannot lock: the app opens.
      if (answer == AppAuth.ok || answer == AppAuth.unavailable) {
        _locked = false;
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    // Settings and the pairing change what is needed: asking starts as soon as the app is locked and needs it.
    ref.listen(settingsProvider, (_, _) => _askIfNeeded());
    ref.listen(pairedComputerProvider, (_, _) => _askIfNeeded());
    // Watched, so the screen follows the switch and the pairing as they change.
    final paired = ref.watch(pairedComputerProvider) != null;
    final on = ref.watch(settingsProvider).value?.requireUnlock ?? true;
    final locked = _locked && paired && on;
    return Stack(
      textDirection: TextDirection.ltr,
      children: [
        // The app stays mounted under the lock (its screens, a half-typed answer, the route the person was on) but is not
        // painted, animated, focusable or read out while locked.
        Positioned.fill(
          child: Offstage(
            offstage: locked,
            child: TickerMode(
              enabled: !locked,
              child: ExcludeFocus(
                excluding: locked,
                child: ExcludeSemantics(excluding: locked, child: widget.child),
              ),
            ),
          ),
        ),
        if (locked)
          Positioned.fill(
            child: LockScreen(
              failed: _lastAnswer == AppAuth.failed,
              onUnlock: _ask,
            ),
          ),
        if (_covered) const Positioned.fill(child: CoverScreen()),
      ],
    );
  }
}

/// What is shown instead of the app while it is locked: the sleeping pet, what is the matter and the way in.
class LockScreen extends ConsumerWidget {
  const LockScreen({super.key, required this.onUnlock, this.failed = false});

  final VoidCallback onUnlock;
  final bool failed;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return Material(
      color: Colors.transparent,
      child: SafeArea(
        child: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const PetView(
                  design: 'default',
                  color: 0,
                  state: 'idle',
                  size: 96,
                ),
                const SizedBox(height: 12),
                ExcludeSemantics(
                  child: Text(
                    'Bobix',
                    style: TextStyle(
                      fontSize: 24,
                      fontWeight: FontWeight.w700,
                      letterSpacing: -.2,
                      color: t.ink,
                    ),
                  ),
                ),
                const SizedBox(height: 12),
                Semantics(
                  header: true,
                  child: Text(
                    s('phone.lock.t'),
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 15,
                      fontWeight: FontWeight.w600,
                      color: t.ink,
                    ),
                  ),
                ),
                const SizedBox(height: 4),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 280),
                  child: Text(
                    failed ? s('phone.lock.failed') : s('phone.lock.d'),
                    textAlign: TextAlign.center,
                    style: TextStyle(fontSize: 14, height: 1.45, color: t.ink2),
                  ),
                ),
                const SizedBox(height: 20),
                PillButton(
                  label: s('phone.lock.open', {'os': bioName(s)}),
                  primary: true,
                  icon: defaultTargetPlatform == TargetPlatform.iOS
                      ? Icons.face_unlock_outlined
                      : Icons.fingerprint,
                  onPressed: onUnlock,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The plain screen the app switcher keeps a picture of: nothing of the app shows.
class CoverScreen extends ConsumerWidget {
  const CoverScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.tokens;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      child: Material(
        color: t.bg,
        child: Center(
          child: Text(
            'Bobix',
            style: TextStyle(
              fontSize: 28,
              fontWeight: FontWeight.w700,
              color: t.ink3,
            ),
          ),
        ),
      ),
    );
  }
}
