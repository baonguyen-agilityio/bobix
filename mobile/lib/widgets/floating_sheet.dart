import 'package:flutter/material.dart';

import '../theme/tokens.dart';
import 'glass.dart';

/// The prototype's bottom sheet: a rounded glass card that floats 8 pt off the edges of the screen, with a grab handle on
/// top. Shown with `showModalBottomSheet` (transparent background, scroll controlled).
class FloatingSheet extends StatelessWidget {
  const FloatingSheet({super.key, required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(8),
        child: Glass(
          radius: 32,
          blur: true,
          tint: t.isDark
              ? t.surface.withValues(alpha: .86)
              : const Color(0xD6FFFFFF),
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
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
