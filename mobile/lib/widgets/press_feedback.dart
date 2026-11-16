import 'package:flutter/gestures.dart' show kTouchSlop;
import 'package:flutter/material.dart';

/// How a pressed control answers the finger, per platform. iOS and macOS dim and shrink it a little while the finger is
/// down; elsewhere the child is left alone, and its own `InkWell` draws the Material ripple.
///
/// It watches the pointer without joining the gesture arena, so the child's own tap, scroll and long press work as
/// before. A finger that moves past the touch slop (a scroll) lets go.
class PressFeedback extends StatefulWidget {
  const PressFeedback({
    super.key,
    required this.child,
    this.scale = .97,
    this.opacity = .8,
    this.onPressedChanged,
  });

  final Widget child;

  /// Size while pressed (1 keeps it).
  final double scale;

  /// Opacity while pressed (1 keeps it).
  final double opacity;

  /// Tells a parent that wants to draw its own pressed state (the tab bar's pills) when the press starts and ends.
  final ValueChanged<bool>? onPressedChanged;

  /// Whether this platform answers a press by dimming (iOS look) rather than by a ripple.
  static bool dims(BuildContext context) =>
      switch (Theme.of(context).platform) {
        TargetPlatform.iOS || TargetPlatform.macOS => true,
        _ => false,
      };

  @override
  State<PressFeedback> createState() => _PressFeedbackState();
}

class _PressFeedbackState extends State<PressFeedback> {
  bool _down = false;
  Offset? _origin;

  void _set(bool down) {
    if (_down == down) return;
    setState(() => _down = down);
    widget.onPressedChanged?.call(down);
  }

  @override
  Widget build(BuildContext context) {
    if (!PressFeedback.dims(context)) return widget.child;
    return Listener(
      onPointerDown: (e) {
        _origin = e.position;
        _set(true);
      },
      onPointerMove: (e) {
        final o = _origin;
        if (o != null && (e.position - o).distance > kTouchSlop) _set(false);
      },
      onPointerUp: (_) => _set(false),
      onPointerCancel: (_) => _set(false),
      child: AnimatedScale(
        scale: _down ? widget.scale : 1,
        duration: Duration(milliseconds: _down ? 70 : 160),
        curve: Curves.easeOut,
        child: AnimatedOpacity(
          opacity: _down ? widget.opacity : 1,
          duration: Duration(milliseconds: _down ? 70 : 160),
          child: widget.child,
        ),
      ),
    );
  }
}
