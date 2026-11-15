import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'data/phone_data.dart';
import 'l10n/l10n.dart';
import 'features/actions/start_screen.dart';
import 'link/computer_link.dart';
import 'link/link_providers.dart';
import 'push/push_platform.dart';
import 'push/push_providers.dart';
import 'surfaces/surfaces_host.dart';
import 'lock/app_lock.dart';
import 'settings/app_settings.dart';
import 'screens/home_screen.dart';
import 'screens/pair_again_screen.dart';
import 'screens/unavailable_screen.dart';
import 'screens/pairing_screens.dart';
import 'screens/pets_screen.dart';
import 'screens/settings_screen.dart';
import 'theme/theme.dart';
import 'theme/tokens.dart';
import 'widgets/glass_tab_bar.dart';
import 'widgets/status_banner.dart';

class BobixApp extends ConsumerWidget {
  const BobixApp({super.key, this.home = const RootGate()});

  /// The first screen: the gate that picks pairing, the app or "pair again". Tests give the shell directly.
  final Widget home;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return MaterialApp(
      title: 'Bobix',
      debugShowCheckedModeBanner: false,
      theme: bobixTheme(Brightness.light),
      darkTheme: bobixTheme(Brightness.dark),
      themeMode:
          ref.watch(settingsProvider).value?.themeMode ?? ThemeMode.system,
      // The lock and the cover sit over everything, the pairing flow included once a computer is paired.
      builder: (context, child) => AppLock(child: child ?? const SizedBox()),
      home: home,
    );
  }
}

/// Not paired: the pairing flow. Paired: the app. A pairing that cannot be used any more: "pair again".
class RootGate extends ConsumerWidget {
  const RootGate({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final phase = ref.watch(linkControllerProvider);
    ref.watch(surfacesClearProvider);
    return switch (phase.value) {
      null when phase.hasError => const UnavailableScreen(),
      null => const Scaffold(body: Center(child: CircularProgressIndicator())),
      Unpaired() => const PairingFlow(),
      Unavailable() => const UnavailableScreen(),
      final Paired p => SurfacesHost(link: p.link, child: const AppShell()),
      final PairAgain a => PairAgainScreen(phase: a),
    };
  }
}

const _home = 0, _start = 2;

/// The tab skeleton: the screen fills the page and the glass tab bar floats over its bottom.
class AppShell extends ConsumerStatefulWidget {
  const AppShell({super.key});

  @override
  ConsumerState<AppShell> createState() => _AppShellState();
}

class _AppShellState extends ConsumerState<AppShell> {
  int _index = _home;
  StreamSubscription<PushTap>? _taps;
  PushTap? _pending;

  @override
  void initState() {
    super.initState();
    unawaited(_listenForTaps());
  }

  @override
  void dispose() {
    _taps?.cancel();
    super.dispose();
  }

  /// A notification, a widget, the Live Activity or a control opened the app: listen first, then take the one that
  /// started it (the platform holds a tap until this first `initialTap` call).
  Future<void> _listenForTaps() async {
    try {
      final p = ref.read(pushPlatformProvider);
      _taps = p.taps.listen(_onTap);
      final first = await p.initialTap();
      if (first != null) _onTap(first);
    } catch (_) {
      // No platform (a test): nothing opens the app but the person.
    }
  }

  void _onTap(PushTap t) {
    if (!mounted) return;
    if (t.kind == 'start') return setState(() => _index = _start);
    if (t.kind == 'home') return setState(() => _index = _home);
    final home = ref.read(homeProvider).value;
    if (home == null) {
      _pending = t; // cold start: the first `home` has not arrived yet
      return;
    }
    setState(() => _index = _home);
    final match = home.sessions.where((x) => x.waitingId == t.ref);
    if (match.isNotEmpty) openSession(context, match.first);
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final pending = _pending;
    if (pending != null && ref.watch(homeProvider).value != null) {
      _pending = null;
      WidgetsBinding.instance.addPostFrameCallback((_) => _onTap(pending));
    }
    final need =
        ref
            .watch(homeProvider)
            .value
            ?.sessions
            .where((x) => x.needsYou)
            .length ??
        0;
    return Scaffold(
      backgroundColor: t.bg,
      body: SafeArea(
        bottom: false,
        child: Stack(
          children: [
            Positioned.fill(
              child: IndexedStack(
                index: _index,
                children: const [
                  HomeScreen(),
                  PetsScreen(),
                  StartScreen(),
                  SettingsScreen(),
                ],
              ),
            ),
            Positioned(left: 0, right: 0, top: 0, child: _LinkProblem()),
            Positioned(
              left: 0,
              right: 0,
              bottom: 0,
              child: SafeArea(
                top: false,
                child: GlassTabBar(
                  tabs: [
                    TabSpec(
                      Icons.home_outlined,
                      s('phone.tab.home'),
                      badge: need,
                    ),
                    TabSpec(Icons.pets_outlined, s('phone.tab.pets')),
                    TabSpec(Icons.settings_outlined, s('phone.tab.settings')),
                  ],
                  start: TabSpec(
                    Icons.play_arrow_rounded,
                    s('phone.tab.start'),
                  ),
                  index: _index,
                  startIndex: _start,
                  onSelect: (i) => setState(() => _index = i),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A problem with the link that the person has to act on: an update, or an invitation the relay wants.
class _LinkProblem extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final status = ref.watch(linkStatusProvider).value;
    final the = s('phone.pair.thecomputer');
    final (String, String)? text = switch (status?.problem) {
      LinkProblem.updatePhone => (
        s('phone.link.update.phone'),
        s('phone.link.update.phone.d'),
      ),
      LinkProblem.updateComputer => (
        s('phone.link.update.computer', {'c': the}),
        s('phone.link.update.computer.d'),
      ),
      LinkProblem.invite => (
        s('phone.link.invite', {'c': the}),
        s('phone.link.invite.d'),
      ),
      _ => null,
    };
    final moved = status?.movedHost;
    if (text == null && moved == null) return const SizedBox.shrink();
    if (text == null) {
      return Padding(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
        child: StatusBanner(
          kind: BannerKind.info,
          icon: Icons.swap_horiz,
          title: s('phone.link.moved', {'h': moved!}),
          body: s('phone.link.moved.d'),
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      child: StatusBanner(
        kind: BannerKind.warn,
        icon: Icons.system_update_alt,
        title: text.$1,
        body: text.$2,
      ),
    );
  }
}
