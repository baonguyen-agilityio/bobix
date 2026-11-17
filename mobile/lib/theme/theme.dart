import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import 'tokens.dart';

/// Touch targets are at least this large, in logical points.
const double minTouch = 44;

ThemeData bobixTheme(Brightness brightness) {
  final t = brightness == Brightness.dark
      ? BobixTokens.dark
      : BobixTokens.light;
  // iOS answers a press by dimming and shrinking the control (`PressFeedback`), not by a ripple.
  final ios =
      defaultTargetPlatform == TargetPlatform.iOS ||
      defaultTargetPlatform == TargetPlatform.macOS;
  return ThemeData(
    splashFactory: ios ? NoSplash.splashFactory : null,
    highlightColor: ios ? Colors.transparent : null,
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

/// The decoration of a boxed text field: rounded, softly filled like the glass cards, a hairline border and the accent
/// when focused. Fields that sit inside a card of their own (a `Glass` around a bare field) do not use it.
InputDecoration bobixFieldDecoration(
  BobixTokens t, {
  required String label,
  bool alignLabelWithHint = false,
}) {
  OutlineInputBorder border(Color color, [double width = 1]) =>
      OutlineInputBorder(
        borderRadius: BorderRadius.circular(16),
        borderSide: BorderSide(color: color, width: width),
      );
  return InputDecoration(
    labelText: label,
    alignLabelWithHint: alignLabelWithHint,
    filled: true,
    fillColor: t.isDark
        ? t.surface2.withValues(alpha: .6)
        : const Color(0x85FFFFFF),
    contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
    labelStyle: TextStyle(color: t.ink3),
    floatingLabelStyle: TextStyle(color: t.accentInk),
    border: border(t.lineStrong),
    enabledBorder: border(t.lineStrong),
    focusedBorder: border(t.accent, 1.5),
    errorBorder: border(t.err),
    focusedErrorBorder: border(t.err, 1.5),
  );
}
