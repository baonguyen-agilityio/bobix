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

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    Widget item(TabSpec s, int i) {
      final on = i == index;
      return Semantics(
        button: true,
        selected: on,
        label: s.label,
        excludeSemantics: true,
        child: InkResponse(
          onTap: () => onSelect(i),
          radius: 30,
          // 48 x 36 item of the capsule, in a 44 pt high touch target.
          child: SizedBox(
            width: 48,
            height: minTouch,
            child: Center(
              child: Container(
                width: 48,
                height: 36,
                decoration: BoxDecoration(
                  color: on
                      ? (t.isDark
                            ? const Color(0x14FFFFFF)
                            : t.ink.withValues(alpha: .06))
                      : null,
                  borderRadius: BorderRadius.circular(18),
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
              ),
            ),
          ),
        ),
      );
    }

    final slots = [
      for (var i = 0, n = 0; i < tabs.length + 1; i++)
        if (i != startIndex) (tabs[n++], i),
    ];
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Glass(
            blur: true,
            radius: 21,
            padding: const EdgeInsets.all(3),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [for (final (s, i) in slots) item(s, i)],
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
