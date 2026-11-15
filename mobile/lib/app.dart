import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'data/phone_data.dart';
import 'l10n/l10n.dart';
import 'link/computer_link.dart';
import 'link/link_providers.dart';
import 'screens/home_screen.dart';
import 'screens/pair_again_screen.dart';
import 'screens/unavailable_screen.dart';
import 'screens/pairing_screens.dart';
import 'screens/pets_screen.dart';
import 'screens/placeholder_screen.dart';
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
    return switch (phase.value) {
      null when phase.hasError => const UnavailableScreen(),
      null => const Scaffold(body: Center(child: CircularProgressIndicator())),
      Unpaired() => const PairingFlow(),
      Unavailable() => const UnavailableScreen(),
      Paired() => const AppShell(),
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

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
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
                  PlaceholderScreen(
                    titleKey: 'phone.start.title',
                    bodyKey: 'phone.start.soon',
                  ),
                  PlaceholderScreen(
                    titleKey: 'phone.settings.title',
                    bodyKey: 'phone.settings.soon',
                  ),
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
    if (text == null) return const SizedBox.shrink();
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
