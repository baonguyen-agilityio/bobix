import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../features/actions/action_widgets.dart' show bioName;
import '../keys/platform_secure_keys.dart';
import '../keys/secure_keys.dart';
import '../l10n/l10n.dart';
import '../link/link_providers.dart';
import '../settings/app_settings.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';

/// The app lock and the cover.
///
/// - **Lock.** With "Require Face ID / fingerprint to open" on (the default) and a computer paired, the app asks when it
///   comes to the foreground, before any content is drawn: while locked only [LockScreen] is built, so nothing of the
///   child exists. It locks again when the app leaves the foreground. Reconnecting asks nothing (the link runs on its
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
    final answer = await ref
        .read(secureKeysProvider)
        .authenticate(reason: reason);
    _asking = false;
    if (!mounted) return;
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
        Positioned.fill(
          child: locked
              ? LockScreen(
                  failed: _lastAnswer == AppAuth.failed,
                  onUnlock: _ask,
                )
              : widget.child,
        ),
        if (_covered) const Positioned.fill(child: CoverScreen()),
      ],
    );
  }
}

/// What is shown instead of the app while it is locked.
class LockScreen extends ConsumerWidget {
  const LockScreen({super.key, required this.onUnlock, this.failed = false});

  final VoidCallback onUnlock;
  final bool failed;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return Material(
      color: t.bg,
      child: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(Icons.lock_outline, size: 56, color: t.accent),
              const SizedBox(height: 16),
              Semantics(
                header: true,
                child: Text(
                  s('phone.lock.t'),
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: 26,
                    fontWeight: FontWeight.w700,
                    color: t.ink,
                  ),
                ),
              ),
              const SizedBox(height: 8),
              Text(
                failed
                    ? s('phone.lock.failed')
                    : s('phone.lock.d', {'os': bioName(s)}),
                textAlign: TextAlign.center,
                style: TextStyle(fontSize: 16, height: 1.4, color: t.ink2),
              ),
              const SizedBox(height: 24),
              PillButton(
                label: s('phone.lock.open', {'os': bioName(s)}),
                primary: true,
                icon: Icons.lock_open_outlined,
                onPressed: onUnlock,
              ),
            ],
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
