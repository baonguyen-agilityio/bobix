import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';
import '../../features/actions/action_widgets.dart' show bioName;
import '../../l10n/l10n.dart';
import '../../theme/oklch.dart';
import '../../theme/theme.dart' show minTouch;
import '../../theme/tokens.dart';
import '../../widgets/detail_scaffold.dart';
import '../../widgets/glass.dart';
import 'control_center.dart';
import 'home_screen_widgets.dart';
import 'lock_widgets.dart';
import 'widget_preview_data.dart';

/// Settings > Widgets: how Bobix shows up outside the app. Three rows open previews of the Home Screen, the Lock Screen and
/// Control Center; below them a switch for what the previews draw and the widgets themselves, drawn from what the app has.
class WidgetsScreen extends ConsumerWidget {
  const WidgetsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    void open(Widget page) => Navigator.of(context).push(pageRoute<void>(page));
    return DetailScaffold(
      title: s('phone.settings.wg'),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 4, 4, 12),
          child: Text(
            s('phone.wgs.n', {'os': bioName(s)}),
            style: TextStyle(fontSize: 14, height: 1.4, color: t.ink2),
          ),
        ),
        Glass(
          child: Material(
            type: MaterialType.transparency,
            child: Column(
              children: [
                _NavRow(
                  icon: Icons.home_outlined,
                  title: s('phone.wgs.hs'),
                  subtitle: s('phone.wgs.hs.d'),
                  onTap: () => open(const HomeScreenPreviewScreen()),
                ),
                Divider(height: .5, thickness: .5, color: t.line),
                _NavRow(
                  icon: Icons.lock_outline,
                  title: s('phone.wgs.lk'),
                  subtitle: s('phone.wgs.lk.d'),
                  onTap: () => open(const LockWidgetsScreen()),
                ),
                Divider(height: .5, thickness: .5, color: t.line),
                _NavRow(
                  icon: Icons.tune,
                  title: s('phone.wgs.cc'),
                  subtitle: s('phone.wgs.cc.d'),
                  onTap: () => open(const ControlCenterScreen()),
                ),
              ],
            ),
          ),
        ),
        _Section(s('phone.wgs.state')),
        const _StateSwitch(),
        _Section(s('phone.wgs.home')),
        _Panel(
          dark: false,
          children: [
            _Spec(s('phone.wgs.small'), '170×170'),
            const SmallHomeWidget(),
            _Spec(s('phone.wgs.medium'), '364×170'),
            const MediumHomeWidget(),
            _Spec(s('phone.wgs.large'), '364×382'),
            const LargeHomeWidget(),
          ],
        ),
        _Section(s('phone.wgs.lk')),
        _Panel(
          dark: true,
          children: [
            _Spec(s('phone.wgs.inline'), ''),
            const LockInlineWidget(),
            _Spec(
              '${s('phone.wgs.circ')} · ${s('phone.wgs.rect')}',
              '72×72 · 160×72',
            ),
            const Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                LockCircleWidget(),
                SizedBox(width: 10),
                LockRectWidget(),
              ],
            ),
          ],
        ),
        _Section(s('phone.wgs.la')),
        const _Panel(dark: true, children: [LiveActivityCard()]),
        _Section(s('phone.wgs.di')),
        _Panel(
          dark: true,
          children: [
            _Spec(s('phone.wgs.di.c'), ''),
            const IslandPreview(IslandKind.compact),
            _Spec(s('phone.wgs.di.m'), ''),
            const IslandPreview(IslandKind.minimal),
            _Spec(s('phone.wgs.di.x'), ''),
            const IslandPreview(IslandKind.expanded),
          ],
        ),
        _Section(s('phone.wgs.cc')),
        const _Panel(
          dark: true,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [BobixControl(), SizedBox(width: 12), StartControl()],
            ),
          ],
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 16, 4, 0),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.shield_outlined, size: 16, color: t.ink3),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  s('phone.wgs.safe', {'os': bioName(s)}),
                  style: TextStyle(fontSize: 12, height: 1.35, color: t.ink3),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _Section extends StatelessWidget {
  const _Section(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(4, 18, 0, 8),
    child: Semantics(
      header: true,
      child: Text(
        text,
        style: TextStyle(
          fontSize: 13,
          fontWeight: FontWeight.w600,
          color: context.tokens.ink3,
        ),
      ),
    ),
  );
}

class _NavRow extends StatelessWidget {
  const _NavRow({
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.onTap,
  });
  final IconData icon;
  final String title, subtitle;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      button: true,
      label: '$title, $subtitle',
      excludeSemantics: true,
      child: InkWell(
        onTap: onTap,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 60),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            child: Row(
              children: [
                Icon(icon, size: 22, color: t.ink),
                const SizedBox(width: 14),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        title,
                        style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w500,
                          color: t.ink,
                        ),
                      ),
                      Text(
                        subtitle,
                        style: TextStyle(
                          fontSize: 12.5,
                          height: 1.35,
                          color: t.ink3,
                        ),
                      ),
                    ],
                  ),
                ),
                Icon(Icons.chevron_right, size: 20, color: t.ink3),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// "Normal / Nothing / Offline": what the previews below and on the next screens draw.
class _StateSwitch extends ConsumerWidget {
  const _StateSwitch();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final now = ref.watch(previewStateProvider);
    return Semantics(
      container: true,
      label: s('phone.wgs.state'),
      child: Container(
        padding: const EdgeInsets.all(3),
        decoration: BoxDecoration(
          color: t.isDark ? t.surface3 : const Color(0x14000000),
          borderRadius: BorderRadius.circular(999),
        ),
        child: Row(
          children: [
            for (final v in PreviewState.values)
              Expanded(
                child: Semantics(
                  button: true,
                  selected: v == now,
                  inMutuallyExclusiveGroup: true,
                  label: s(
                    'phone.wgs.s.${v.name == 'normal' ? 'normal' : v.name}',
                  ),
                  excludeSemantics: true,
                  child: InkWell(
                    borderRadius: BorderRadius.circular(999),
                    onTap: () => ref.read(previewStateProvider.notifier).set(v),
                    child: Container(
                      height: minTouch,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: v == now
                            ? (t.isDark ? t.surface : Colors.white)
                            : null,
                        borderRadius: BorderRadius.circular(999),
                        boxShadow: v == now
                            ? const [
                                BoxShadow(
                                  color: Color(0x1F000000),
                                  blurRadius: 4,
                                  offset: Offset(0, 1),
                                ),
                              ]
                            : null,
                      ),
                      child: Text(
                        s('phone.wgs.s.${v.name}'),
                        style: TextStyle(
                          fontSize: 14,
                          fontWeight: FontWeight.w600,
                          color: v == now ? t.ink : t.ink2,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// A tinted panel the previews sit in: warm, blue and green washes in light mode (the Home Screen) or a dark wallpaper
/// (the lock screen).
class _Panel extends StatelessWidget {
  const _Panel({required this.dark, required this.children});
  final bool dark;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final gradient = dark
        ? const LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [Color(0xFF34406E), Color(0xFF6B4A86), Color(0xFFD9735F)],
          )
        : LinearGradient(
            begin: Alignment.topRight,
            end: Alignment.bottomLeft,
            colors: t.isDark
                ? [t.surface2, t.surface3]
                : [
                    oklch(91, .05, 33, .7),
                    oklch(92, .04, 250, .6),
                    oklch(92, .05, 165, .6),
                  ],
          );
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        gradient: gradient,
        borderRadius: BorderRadius.circular(22),
        border: Border.all(width: .5, color: t.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final c in children)
            if (c is _Spec) c else Center(child: c),
        ],
      ),
    );
  }
}

/// The name of a preview and its size in points.
class _Spec extends StatelessWidget {
  const _Spec(this.name, this.size);
  final String name, size;

  @override
  Widget build(BuildContext context) {
    final inDark =
        context.findAncestorWidgetOfExactType<_Panel>()?.dark ?? false;
    final c = inDark ? const Color(0xB3FFFFFF) : context.tokens.ink3;
    final style = TextStyle(
      fontSize: 11,
      fontWeight: FontWeight.w700,
      color: c,
    );
    return Padding(
      padding: const EdgeInsets.fromLTRB(2, 8, 2, 6),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Flexible(child: Text(name, style: style)),
          Text(size, style: style.copyWith(fontWeight: FontWeight.w500)),
        ],
      ),
    );
  }
}
