import 'dart:ui' show ImageFilter;

import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// The prototype's glass: a translucent surface with a hairline border. It blurs what is behind it (as the desktop's
/// `backdrop-filter: blur(20px)`) only when [blur] is set, which is for surfaces that float over scrolling content: the
/// tab bar, the Start button and the computer chip. Everywhere else the page behind is flat, a blur would blur nothing and
/// would cost a layer each, so the fill is flat. [blurEnabled] turns every blur off at once (the spike measured the cost
/// on an emulator only; switch it off if a phone's numbers are bad).
class Glass extends StatelessWidget {
  const Glass({
    super.key,
    required this.child,
    this.radius = 16,
    this.padding,
    this.tint,
    this.border,
    this.blur = false,
  });

  /// The one switch for every blur in the app.
  static bool blurEnabled = true;

  static const blurSigma = 20.0;

  final Widget child;
  final double radius;
  final EdgeInsetsGeometry? padding;
  final Color? tint;
  final Color? border;

  /// This surface floats over scrolling content: blur what is behind it (when [blurEnabled]).
  final bool blur;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final fill =
        tint ??
        (t.isDark ? t.surface2.withValues(alpha: .6) : const Color(0x85FFFFFF));
    final shape = BorderRadius.circular(radius);
    final content = Padding(padding: padding ?? EdgeInsets.zero, child: child);
    final body = DecoratedBox(
      decoration: BoxDecoration(
        color: fill,
        borderRadius: shape,
        border: Border.all(
          width: .5,
          color:
              border ??
              (t.isDark ? const Color(0x17FFFFFF) : const Color(0xE6FFFFFF)),
        ),
      ),
      child: content,
    );
    final blurred = blur && blurEnabled;
    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: shape,
        boxShadow: [
          BoxShadow(
            color: Color(t.isDark ? 0x4D000000 : 0x12141828),
            blurRadius: 14,
            offset: const Offset(0, 4),
          ),
        ],
      ),
      child: blurred
          ? ClipRRect(
              borderRadius: shape,
              child: BackdropFilter(
                filter: ImageFilter.blur(sigmaX: blurSigma, sigmaY: blurSigma),
                child: body,
              ),
            )
          : body,
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
