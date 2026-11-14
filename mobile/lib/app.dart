import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'data/phone_data.dart';
import 'l10n/l10n.dart';
import 'screens/home_screen.dart';
import 'screens/pets_screen.dart';
import 'screens/placeholder_screen.dart';
import 'theme/theme.dart';
import 'theme/tokens.dart';
import 'widgets/glass_tab_bar.dart';

class BobixApp extends ConsumerWidget {
  const BobixApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return MaterialApp(
      title: 'Bobix',
      debugShowCheckedModeBanner: false,
      theme: bobixTheme(Brightness.light),
      darkTheme: bobixTheme(Brightness.dark),
      home: const AppShell(),
    );
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
