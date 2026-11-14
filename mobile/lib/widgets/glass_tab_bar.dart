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
          child: Container(
            width: 56,
            height: minTouch,
            decoration: BoxDecoration(
              color: on ? t.ink.withValues(alpha: .06) : null,
              borderRadius: BorderRadius.circular(22),
            ),
            child: Stack(
              alignment: Alignment.center,
              children: [
                Icon(s.icon, size: 22, color: on ? t.accentInk : t.ink2),
                if (s.badge > 0)
                  Positioned(
                    top: 4,
                    right: 8,
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
            radius: 24,
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
              child: Glass(
                radius: 25,
                child: SizedBox(
                  width: 50,
                  height: 50,
                  child: Icon(
                    start.icon,
                    size: 24,
                    color: index == startIndex ? t.accentInk : t.ink2,
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
