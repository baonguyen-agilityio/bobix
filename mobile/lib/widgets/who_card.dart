import 'package:flutter/material.dart';

import '../data/models.dart';
import '../pets/pet_view.dart';
import '../theme/tokens.dart';
import 'glass.dart';

enum PillTone { neutral, accent, ok, info, err }

/// A small rounded label (a badge of the prototype).
class Pill extends StatelessWidget {
  const Pill(this.text, {super.key, this.tone = PillTone.neutral});

  final String text;
  final PillTone tone;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final (Color fg, Color bg) = switch (tone) {
      PillTone.accent => (t.accentInk, t.accentBg),
      PillTone.ok => (t.ok, t.okBg),
      PillTone.info => (t.info, t.infoBg),
      PillTone.err => (t.err, t.err.withValues(alpha: .14)),
      PillTone.neutral => (t.ink2, t.surface3),
    };
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 1),
      constraints: const BoxConstraints(minHeight: 22),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(6),
      ),
      child: Text(
        text,
        style: TextStyle(
          fontSize: 12,
          fontWeight: FontWeight.w600,
          color: fg,
          height: 1.3,
        ),
      ),
    );
  }
}

/// The session a detail screen is about: its pet, its name, where it lives, and a few badges.
class WhoCard extends StatelessWidget {
  const WhoCard({
    super.key,
    required this.session,
    required this.subtitle,
    this.badges = const [],
  });

  final Session session;
  final String subtitle;
  final List<Widget> badges;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Glass(
      padding: const EdgeInsets.all(14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              PetView(
                design: session.petDesign,
                color: session.petColor,
                state: session.petState,
                size: 60,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      session.name,
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w700,
                        height: 1.3,
                        color: t.ink,
                      ),
                    ),
                    Text(
                      subtitle,
                      style: TextStyle(fontSize: 13, color: t.ink2),
                    ),
                  ],
                ),
              ),
            ],
          ),
          if (badges.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 10),
              child: Wrap(spacing: 6, runSpacing: 6, children: badges),
            ),
        ],
      ),
    );
  }
}
