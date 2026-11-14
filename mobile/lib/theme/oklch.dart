import 'dart:math' as math;
import 'dart:ui';

/// A colour written as in `src/ui/theme.css`: `oklch(L% C H / alpha)`, with [lightness] in percent.
Color oklch(double lightness, double chroma, double hue, [double alpha = 1]) {
  final l0 = lightness / 100;
  final a = chroma * math.cos(hue * math.pi / 180);
  final b = chroma * math.sin(hue * math.pi / 180);
  final l = math.pow(l0 + 0.3963377774 * a + 0.2158037573 * b, 3);
  final m = math.pow(l0 - 0.1055613458 * a - 0.0638541728 * b, 3);
  final s = math.pow(l0 - 0.0894841775 * a - 1.291485548 * b, 3);
  double gamma(num x) {
    final v = x <= 0.0031308 ? 12.92 * x : 1.055 * math.pow(x, 1 / 2.4) - 0.055;
    return v.clamp(0.0, 1.0).toDouble();
  }

  return Color.from(
    alpha: alpha,
    red: gamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    green: gamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    blue: gamma(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  );
}
