import 'package:flutter/material.dart';

enum PetState { working, waiting, idle, failed }

/// A session's pet. Placeholder drawing until `creature.dart` (the port of the desktop pets) lands.
class PetView extends StatelessWidget {
  const PetView({
    super.key,
    required this.design,
    required this.color,
    required this.state,
    this.size = 44,
  });

  final int design, color;
  final PetState state;
  final double size;

  static const _bodies = [
    Color(0xFF9CCBEA),
    Color(0xFF9FE0BF),
    Color(0xFFCDB8F0),
    Color(0xFFF2C5A0),
    Color(0xFFEFDB8A),
    Color(0xFFF0B3B6),
  ];

  @override
  Widget build(BuildContext context) {
    return ExcludeSemantics(
      child: SizedBox(
        width: size,
        height: size,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: _bodies[color % _bodies.length],
            shape: BoxShape.circle,
          ),
        ),
      ),
    );
  }
}
