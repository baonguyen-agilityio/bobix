import 'package:flutter/material.dart';

/// Fades its child back in whenever [value] changes (the language, say): the new text arrives with a short fade instead
/// of snapping. The child is never rebuilt into a second copy, so the navigation inside it stays where it was.
class FadeOnChange extends StatefulWidget {
  const FadeOnChange({super.key, required this.value, required this.child});

  final Object? value;
  final Widget child;

  @override
  State<FadeOnChange> createState() => _FadeOnChangeState();
}

class _FadeOnChangeState extends State<FadeOnChange>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 260),
    value: 1,
  );

  @override
  void didUpdateWidget(FadeOnChange old) {
    super.didUpdateWidget(old);
    if (old.value != widget.value) _c.forward(from: .25);
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => FadeTransition(
    opacity: CurvedAnimation(parent: _c, curve: Curves.easeOut),
    child: widget.child,
  );
}
