import 'package:flutter/material.dart';

import '../theme/oklch.dart';
import '../theme/tokens.dart';

/// The page behind every screen. In light mode the prototype tints it with three soft radial washes (warm top right,
/// blue left, green bottom right) so the glass on top has something to read against; dark mode is flat.
class GlassBackdrop extends StatelessWidget {
  const GlassBackdrop({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    if (t.isDark) return ColoredBox(color: t.bg, child: child);
    return CustomPaint(painter: _WashPainter(t.bg), child: child);
  }
}

class _WashPainter extends CustomPainter {
  const _WashPainter(this.base);
  final Color base;

  static final _washes = <(double, double, double, double, Color, double)>[
    // (centre x, centre y, radius x, radius y) as fractions of the page, colour, fade-out stop
    (1.0, 0.0, 1.2, 0.6, oklch(90, .06, 33, .55), .6),
    (0.0, 0.35, 0.9, 0.5, oklch(90, .05, 250, .5), .65),
    (0.8, 1.0, 1.0, 0.55, oklch(91, .05, 165, .45), .6),
  ];

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(Offset.zero & size, Paint()..color = base);
    for (final (cx, cy, rx, ry, color, stop) in _washes) {
      final centre = Offset(cx * size.width, cy * size.height);
      final radiusX = rx * size.width, radiusY = ry * size.height;
      canvas.save();
      canvas.translate(centre.dx, centre.dy);
      canvas.scale(1, radiusY / radiusX);
      final shader = RadialGradient(
        colors: [color, color.withValues(alpha: 0)],
        stops: [0, stop],
      ).createShader(Rect.fromCircle(center: Offset.zero, radius: radiusX));
      canvas.drawRect(
        Rect.fromCenter(
          center: Offset.zero,
          width: radiusX * 2,
          height: radiusX * 2,
        ),
        Paint()..shader = shader,
      );
      canvas.restore();
    }
  }

  @override
  bool shouldRepaint(_WashPainter old) => old.base != base;
}
