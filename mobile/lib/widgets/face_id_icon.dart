import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

/// The iOS biometric glyph: four rounded corner brackets around a face (two eyes, a nose, a smile). Material has no
/// such icon, so it is drawn here on the same 24 unit grid as the Material icons.
class FaceIdIcon extends StatelessWidget {
  const FaceIdIcon({super.key, this.size = 24, this.color});

  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final c = color ?? IconTheme.of(context).color ?? const Color(0xFF000000);
    return SizedBox(
      width: size,
      height: size,
      child: CustomPaint(painter: _FaceIdPainter(c)),
    );
  }
}

class _FaceIdPainter extends CustomPainter {
  const _FaceIdPainter(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final k = size.width / 24;
    final p = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.8 * k
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    Offset o(double x, double y) => Offset(x * k, y * k);
    final brackets = Path()
      ..moveTo(4 * k, 8 * k)
      ..lineTo(4 * k, 6 * k)
      ..arcToPoint(o(6, 4), radius: Radius.circular(2 * k))
      ..lineTo(8 * k, 4 * k)
      ..moveTo(16 * k, 4 * k)
      ..lineTo(18 * k, 4 * k)
      ..arcToPoint(o(20, 6), radius: Radius.circular(2 * k))
      ..lineTo(20 * k, 8 * k)
      ..moveTo(20 * k, 16 * k)
      ..lineTo(20 * k, 18 * k)
      ..arcToPoint(o(18, 20), radius: Radius.circular(2 * k))
      ..lineTo(16 * k, 20 * k)
      ..moveTo(8 * k, 20 * k)
      ..lineTo(6 * k, 20 * k)
      ..arcToPoint(o(4, 18), radius: Radius.circular(2 * k))
      ..lineTo(4 * k, 16 * k);
    final face = Path()
      ..moveTo(9 * k, 9 * k)
      ..lineTo(9 * k, 10.5 * k)
      ..moveTo(15 * k, 9 * k)
      ..lineTo(15 * k, 10.5 * k)
      ..moveTo(12 * k, 9 * k)
      ..lineTo(12 * k, 13 * k)
      ..lineTo(11 * k, 13 * k)
      ..moveTo(9 * k, 16 * k)
      ..quadraticBezierTo(12 * k, 18.2 * k, 15 * k, 16 * k);
    canvas
      ..drawPath(brackets, p)
      ..drawPath(face, p);
  }

  @override
  bool shouldRepaint(_FaceIdPainter old) => old.color != color;
}

/// The glyph of the phone's own check: the Face ID face on iPhone, a fingerprint elsewhere.
Widget bioGlyph({double size = 24, Color? color}) =>
    defaultTargetPlatform == TargetPlatform.iOS
    ? FaceIdIcon(size: size, color: color)
    : Icon(Icons.fingerprint, size: size, color: color);
