import 'package:flutter/material.dart';

import 'tokens.dart';

/// Touch targets are at least this large, in logical points.
const double minTouch = 44;

ThemeData bobixTheme(Brightness brightness) {
  final t = brightness == Brightness.dark
      ? BobixTokens.dark
      : BobixTokens.light;
  return ThemeData(
    useMaterial3: true,
    brightness: brightness,
    scaffoldBackgroundColor: t.bg,
    colorScheme:
        ColorScheme.fromSeed(
          seedColor: t.accentSolid,
          brightness: brightness,
        ).copyWith(
          surface: t.surface,
          onSurface: t.ink,
          primary: t.accentSolid,
          onPrimary: t.onAccent,
          error: t.err,
        ),
    dividerColor: t.line,
    textTheme:
        (brightness == Brightness.dark
                ? Typography.material2021().white
                : Typography.material2021().black)
            .apply(bodyColor: t.ink, displayColor: t.ink),
    switchTheme: SwitchThemeData(
      thumbColor: const WidgetStatePropertyAll(Color(0xFFFFFFFF)),
      trackColor: WidgetStateProperty.resolveWith(
        (s) => s.contains(WidgetState.selected) ? t.ok : t.n300,
      ),
      trackOutlineColor: const WidgetStatePropertyAll(Colors.transparent),
    ),
    extensions: [t],
  );
}
