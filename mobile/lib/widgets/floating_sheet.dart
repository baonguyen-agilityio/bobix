import 'package:flutter/material.dart';

import '../theme/tokens.dart';
import 'glass.dart';

/// The gap between a floating sheet and the sides and bottom edge of the screen.
const double sheetGap = 8;

/// The prototype's bottom sheet: a Liquid Glass card (blurred backdrop, translucent fill, sheen, hairline border and a
/// deep outer shadow) that floats [sheetGap] off the sides and the bottom edge of the screen, with a grab handle on top.
/// The home indicator sits inside the card, so the card's own bottom padding grows by what the indicator needs. Shown
/// with `showModalBottomSheet` (transparent background, scroll controlled); every sheet of the app is one of these.
class FloatingSheet extends StatelessWidget {
  const FloatingSheet({
    super.key,
    required this.children,
    this.horizontal = 16,
  });

  final List<Widget> children;

  /// The left and right padding of the content.
  final double horizontal;

  /// The card's bottom padding: at least 16, and enough to keep the content clear of the home indicator that overlaps
  /// the card (the card ends [sheetGap] above the screen's edge).
  static double bottomPadding(BuildContext context) {
    final inset = MediaQuery.viewPaddingOf(context).bottom;
    return inset - sheetGap > 16 ? inset - sheetGap : 16;
  }

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return SafeArea(
      bottom: false,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(sheetGap, 0, sheetGap, sheetGap),
        child: Glass(
          radius: 32,
          blur: true,
          tint: t.isDark
              ? t.surface.withValues(alpha: .86)
              : const Color(0xD6FFFFFF),
          border: t.isDark ? null : const Color(0xE6FFFFFF),
          shadowBlur: 50,
          shadowOffset: const Offset(0, 20),
          shadowColor: t.isDark
              ? const Color(0x80000000)
              : const Color(0x2E141828),
          padding: EdgeInsets.fromLTRB(
            horizontal,
            12,
            horizontal,
            bottomPadding(context),
          ),
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Center(
                  child: Container(
                    width: 38,
                    height: 5,
                    decoration: BoxDecoration(
                      color: t.lineStrong,
                      borderRadius: BorderRadius.circular(3),
                    ),
                  ),
                ),
                const SizedBox(height: 10),
                ...children,
              ],
            ),
          ),
        ),
      ),
    );
  }
}
