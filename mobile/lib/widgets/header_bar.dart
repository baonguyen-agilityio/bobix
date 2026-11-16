import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../features/actions/action_widgets.dart' show HeaderPill;
import '../theme/tokens.dart';

/// The header of a screen: [leading] and [trailing] pills at the edges and the title centred on the SCREEN, whatever the
/// two sides hold (a Back only, Back and Log, nothing). Both sides reserve the width of the wider one, so the title stays
/// in the middle; it wraps to a second line, with an ellipsis as the backstop, and the bar grows with large text.
class HeaderBar extends StatelessWidget {
  const HeaderBar({super.key, this.title, this.leading, this.trailing});

  final String? title;
  final HeaderPill? leading, trailing;

  /// The gap between a pill and the screen's edge, and between a pill and the title.
  static const edge = 16.0, gap = 12.0;

  /// The width a [HeaderPill] with [label] takes: its padding, border, icon, gap and the label at its capped text scale.
  static double pillWidth(BuildContext context, String label) {
    final painter = TextPainter(
      text: TextSpan(text: label, style: const TextStyle(fontSize: 16)),
      textDirection: TextDirection.ltr,
      textScaler: MediaQuery.textScalerOf(context)
          .clamp(maxScaleFactor: HeaderPill.maxTextScale),
    )..layout();
    return HeaderPill.chrome + painter.width;
  }

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final left = leading == null ? 0.0 : pillWidth(context, leading!.label);
    final right = trailing == null ? 0.0 : pillWidth(context, trailing!.label);
    final side = math.max(left, right);
    final width = MediaQuery.sizeOf(context).width;
    final inset = side == 0 ? edge : math.min(edge + side + gap, width * .36);
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 52),
      child: SizedBox(
        width: double.infinity,
        child: Stack(
          alignment: Alignment.center,
          children: [
            if (title != null)
              Padding(
                padding: EdgeInsets.symmetric(horizontal: inset),
                child: Semantics(
                  header: true,
                  child: Text(
                    title!,
                    textAlign: TextAlign.center,
                    // A long title (Vietnamese) wraps to a second line before it is cut.
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 17,
                      height: 1.15,
                      fontWeight: FontWeight.w600,
                      color: t.ink,
                    ),
                  ),
                ),
              )
            else
              const SizedBox(height: 52),
            if (leading != null)
              Positioned(
                left: edge,
                top: 0,
                bottom: 0,
                child: Align(alignment: Alignment.centerLeft, child: leading),
              ),
            if (trailing != null)
              Positioned(
                right: edge,
                top: 0,
                bottom: 0,
                child: Align(alignment: Alignment.centerRight, child: trailing),
              ),
          ],
        ),
      ),
    );
  }
}
