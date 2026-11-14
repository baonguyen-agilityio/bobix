import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// A flat translucent surface with a hairline border: the prototype's glass without the blur. Blur is added once the
/// Android glass experiment says it is fast enough.
class Glass extends StatelessWidget {
  const Glass({
    super.key,
    required this.child,
    this.radius = 16,
    this.padding,
    this.tint,
    this.border,
  });

  final Widget child;
  final double radius;
  final EdgeInsetsGeometry? padding;
  final Color? tint;
  final Color? border;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final fill =
        tint ??
        (t.isDark ? t.surface2.withValues(alpha: .6) : const Color(0x85FFFFFF));
    return DecoratedBox(
      decoration: BoxDecoration(
        color: fill,
        borderRadius: BorderRadius.circular(radius),
        border: Border.all(
          width: .5,
          color:
              border ??
              (t.isDark ? const Color(0x17FFFFFF) : const Color(0xE6FFFFFF)),
        ),
        boxShadow: [
          BoxShadow(
            color: Color(t.isDark ? 0x4D000000 : 0x12141828),
            blurRadius: 14,
            offset: const Offset(0, 4),
          ),
        ],
      ),
      child: Padding(padding: padding ?? EdgeInsets.zero, child: child),
    );
  }
}

class CountBadge extends StatelessWidget {
  const CountBadge(this.count, {super.key, this.accent = false});

  final int count;
  final bool accent;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 1),
      constraints: const BoxConstraints(minHeight: 22),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: accent ? t.accentBg : t.surface3,
        borderRadius: BorderRadius.circular(11),
      ),
      child: Text(
        '$count',
        style: TextStyle(
          fontSize: 12,
          fontWeight: FontWeight.w600,
          color: accent ? t.accentInk : t.ink2,
        ),
      ),
    );
  }
}

class SectionHeader extends StatelessWidget {
  const SectionHeader(
    this.title, {
    super.key,
    this.count,
    this.needsYou = false,
  });

  final String title;
  final int? count;
  final bool needsYou;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Padding(
      padding: const EdgeInsets.fromLTRB(0, 18, 0, 8),
      child: Row(
        children: [
          Flexible(
            child: Text(
              title,
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: needsYou ? t.accentInk : t.ink3,
              ),
            ),
          ),
          if (count != null) ...[
            const SizedBox(width: 8),
            CountBadge(count!, accent: needsYou),
          ],
        ],
      ),
    );
  }
}
