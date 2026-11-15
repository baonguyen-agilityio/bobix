import 'package:flutter/material.dart';

import 'oklch.dart';

/// The prototype's tokens (panel style B of `src/ui/theme.css`), light and dark.
@immutable
class BobixTokens extends ThemeExtension<BobixTokens> {
  const BobixTokens({
    required this.bg,
    required this.surface,
    required this.surface2,
    required this.surface3,
    required this.line,
    required this.lineStrong,
    required this.ink,
    required this.ink2,
    required this.ink3,
    required this.accent,
    required this.accentInk,
    required this.accentBg,
    required this.accentSolid,
    required this.onAccent,
    required this.ok,
    required this.okBg,
    required this.warn,
    required this.warnBg,
    required this.info,
    required this.infoBg,
    required this.err,
    required this.isDark,
  });

  final Color bg, surface, surface2, surface3, line, lineStrong;
  final Color ink, ink2, ink3;
  final Color accent, accentInk, accentBg, accentSolid, onAccent;
  final Color ok, okBg, warn, warnBg, info, infoBg, err;
  final bool isDark;

  static final light = BobixTokens(
    bg: oklch(97.4, .004, 262),
    surface: oklch(99.6, .001, 262),
    surface2: oklch(96.6, .004, 262),
    surface3: oklch(93.6, .007, 262),
    line: oklch(25, .02, 262, .10),
    lineStrong: oklch(25, .02, 262, .17),
    ink: oklch(22, .02, 264),
    ink2: oklch(40, .02, 264),
    ink3: oklch(51, .018, 264),
    accent: oklch(66, .175, 33),
    accentInk: oklch(50, .16, 33),
    accentBg: oklch(66, .175, 33, .12),
    accentSolid: oklch(55, .17, 33),
    onAccent: oklch(99, .005, 33),
    ok: oklch(52, .13, 155),
    okBg: oklch(60, .13, 155, .13),
    warn: oklch(52, .12, 70),
    warnBg: oklch(78, .14, 80, .20),
    info: oklch(52, .15, 255),
    infoBg: oklch(60, .14, 255, .12),
    err: oklch(52, .19, 15),
    isDark: false,
  );

  static final dark = BobixTokens(
    bg: oklch(16.4, .006, 265),
    surface: oklch(20.4, .007, 265),
    surface2: oklch(23.8, .008, 265),
    surface3: oklch(27.4, .01, 265),
    line: oklch(100, 0, 0, .075),
    lineStrong: oklch(100, 0, 0, .13),
    ink: oklch(95, .004, 265),
    ink2: oklch(80, .008, 265),
    ink3: oklch(66, .012, 265),
    accent: oklch(66, .175, 33),
    accentInk: oklch(77, .13, 33),
    accentBg: oklch(66, .175, 33, .16),
    accentSolid: oklch(68, .17, 33),
    onAccent: oklch(17, .03, 33),
    ok: oklch(76, .14, 155),
    okBg: oklch(70, .14, 155, .14),
    warn: oklch(84, .13, 82),
    warnBg: oklch(80, .14, 80, .13),
    info: oklch(75, .12, 255),
    infoBg: oklch(70, .12, 255, .15),
    err: oklch(72, .17, 18),
    isDark: true,
  );

  @override
  BobixTokens copyWith() => this;

  @override
  BobixTokens lerp(ThemeExtension<BobixTokens>? other, double t) =>
      t < .5 ? this : (other as BobixTokens? ?? this);
}

extension BobixThemeContext on BuildContext {
  BobixTokens get tokens => Theme.of(this).extension<BobixTokens>()!;
}
