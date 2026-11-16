import 'dart:math' show Random;

import 'package:flutter/material.dart';

import '../theme/tokens.dart';
import '../widgets/glass_backdrop.dart';
import 'creature.dart';
import 'pet_view.dart';

/// A pet picked at random from every design and colour each time it is built into the tree, drawn with its own
/// animation. Nothing about a session: it is for the screens that show no data (the splash, the lock, the cover).
class RandomPet extends StatefulWidget {
  const RandomPet({
    super.key,
    this.size = 96,
    this.state = 'working',
    this.random,
  });

  final double size;
  final String state;

  /// A fixed source of randomness, for tests.
  final Random? random;

  @override
  State<RandomPet> createState() => _RandomPetState();
}

class _RandomPetState extends State<RandomPet> {
  late final String _design;
  late final int _color;

  @override
  void initState() {
    super.initState();
    final r = widget.random ?? Random();
    _design = designs[r.nextInt(designs.length)];
    _color = r.nextInt(petColorCount);
  }

  @override
  Widget build(BuildContext context) => PetView(
    design: _design,
    color: _color,
    state: widget.state,
    size: widget.size,
  );
}

/// The screen shown while the app starts or connects: a few pets at work and the name. No data, in both themes.
class PetsSplash extends StatelessWidget {
  const PetsSplash({super.key});

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      label: 'Bobix',
      container: true,
      child: GlassBackdrop(
        // A Material above the text: without one it falls back to the monospace default style.
        child: Material(
          color: Colors.transparent,
          child: SizedBox.expand(
            child: Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      RandomPet(size: 64),
                      SizedBox(width: 12),
                      RandomPet(size: 88),
                      SizedBox(width: 12),
                      RandomPet(size: 64),
                    ],
                  ),
                  const SizedBox(height: 16),
                  ExcludeSemantics(
                    child: Text(
                      'Bobix',
                      style: TextStyle(
                        fontSize: 24,
                        fontWeight: FontWeight.w700,
                        letterSpacing: -.2,
                        color: t.ink,
                        decoration: TextDecoration.none,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
