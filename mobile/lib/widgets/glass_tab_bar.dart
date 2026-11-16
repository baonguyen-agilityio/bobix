import 'package:flutter/material.dart';

import '../theme/theme.dart';
import '../theme/tokens.dart';
import 'glass.dart';

class TabSpec {
  const TabSpec(this.icon, this.label, {this.badge = 0});
  final IconData icon;
  final String label;
  final int badge;
}

/// The floating tab bar: a capsule with Home, Pets and Settings, and Start as a separate round button.
class GlassTabBar extends StatelessWidget {
  const GlassTabBar({
    super.key,
    required this.tabs,
    required this.start,
    required this.index,
    required this.startIndex,
    required this.onSelect,
  });

  final List<TabSpec> tabs; // in the order of their indexes, skipping Start
  final TabSpec start;
  final int index;
  final int startIndex;
  final ValueChanged<int> onSelect;

  static const double capsulePad = 3, itemW = 48, itemH = 36;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    // The capsule is `itemH + 2 * capsulePad` high and its items are `itemW x itemH`, so the selected pill sits
    // `capsulePad` from the capsule on the top, the bottom and (for the first and last item) the outer side.
    Widget pill(TabSpec s, int i) {
      final on = i == index;
      return Container(
        key: ValueKey('tab-pill-$i'),
        width: itemW,
        height: itemH,
        decoration: BoxDecoration(
          color: on
              ? (t.isDark
                    ? const Color(0x14FFFFFF)
                    : t.ink.withValues(alpha: .06))
              : null,
          borderRadius: BorderRadius.circular(itemH / 2),
        ),
        child: Stack(
          alignment: Alignment.center,
          children: [
            Icon(s.icon, size: 20, color: on ? t.accentInk : t.ink2),
            if (s.badge > 0)
              Positioned(
                top: 1,
                right: 4,
                child: Container(
                  constraints: const BoxConstraints(
                    minWidth: 16,
                    minHeight: 16,
                  ),
                  padding: const EdgeInsets.symmetric(horizontal: 4),
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: t.accentSolid,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Text(
                    '${s.badge}',
                    style: TextStyle(
                      fontSize: 10,
                      fontWeight: FontWeight.w700,
                      color: t.onAccent,
                    ),
                  ),
                ),
              ),
          ],
        ),
      );
    }

    // The touch target of an item: the pill's width and a full 44 pt in height, laid over the capsule, which is shorter.
    Widget target(TabSpec s, int i) => Semantics(
      button: true,
      selected: i == index,
      label: s.label,
      excludeSemantics: true,
      child: InkResponse(
        onTap: () => onSelect(i),
        radius: 30,
        child: const SizedBox(width: itemW, height: minTouch),
      ),
    );

    final slots = [
      for (var i = 0, n = 0; i < tabs.length + 1; i++)
        if (i != startIndex) (tabs[n++], i),
    ];
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          SizedBox(
            width: itemW * slots.length + capsulePad * 2,
            height: minTouch,
            child: Stack(
              alignment: Alignment.center,
              children: [
                Glass(
                  key: const ValueKey('tab-capsule'),
                  blur: true,
                  radius: (itemH + capsulePad * 2) / 2,
                  padding: const EdgeInsets.all(capsulePad),
                  child: ExcludeSemantics(
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [for (final (s, i) in slots) pill(s, i)],
                    ),
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: capsulePad),
                  child: Row(
                    children: [for (final (s, i) in slots) target(s, i)],
                  ),
                ),
              ],
            ),
          ),
          Semantics(
            button: true,
            selected: index == startIndex,
            label: start.label,
            excludeSemantics: true,
            child: InkResponse(
              onTap: () => onSelect(startIndex),
              // Solid accent like a primary button, with a glass sheen and a soft accent shadow; the icon is the
              // token's on-accent (light in light mode, dark in dark mode).
              child: DecoratedBox(
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: t.accentSolid,
                  boxShadow: [
                    BoxShadow(
                      color: t.accent.withValues(alpha: .30),
                      blurRadius: 16,
                      offset: const Offset(0, 6),
                    ),
                    const BoxShadow(
                      color: Color(0x14000000),
                      blurRadius: 2,
                      offset: Offset(0, 1),
                    ),
                  ],
                ),
                // A gradient replaces the colour in a BoxDecoration, so the sheen is its own layer over the solid fill.
                child: DecoratedBox(
                  decoration: const BoxDecoration(
                    shape: BoxShape.circle,
                    gradient: LinearGradient(
                      begin: Alignment.topCenter,
                      end: Alignment.bottomCenter,
                      colors: [Color(0x38FFFFFF), Color(0x00FFFFFF)],
                      stops: [0, .55],
                    ),
                  ),
                  child: SizedBox(
                    width: minTouch,
                    height: minTouch,
                    child: Icon(start.icon, size: 22, color: t.onAccent),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
