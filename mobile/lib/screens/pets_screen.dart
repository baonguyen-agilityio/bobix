import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/phone_data.dart';
import '../l10n/l10n.dart';
import '../pets/pet_view.dart';
import '../theme/tokens.dart';
import '../widgets/glass.dart';
import 'home_screen.dart';

/// Every session as its pet; the ones that need the person wear the coral tint.
class PetsScreen extends ConsumerWidget {
  const PetsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final sessions = ref.watch(homeProvider).value?.sessions ?? const [];
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 96),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 6, 0, 4),
          child: Semantics(
            header: true,
            child: Text(
              s('phone.pets.title'),
              style: TextStyle(
                fontSize: 28,
                fontWeight: FontWeight.w700,
                letterSpacing: -.5,
                color: t.ink,
              ),
            ),
          ),
        ),
        if (sessions.isEmpty)
          Glass(
            padding: const EdgeInsets.all(16),
            child: Column(
              children: [
                Text(
                  s('phone.pets.empty'),
                  style: TextStyle(fontWeight: FontWeight.w700, color: t.ink),
                ),
                const SizedBox(height: 4),
                Text(
                  s('phone.pets.empty.d'),
                  textAlign: TextAlign.center,
                  style: TextStyle(color: t.ink2),
                ),
              ],
            ),
          )
        else ...[
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: Text(
              s('phone.pets.n'),
              style: TextStyle(fontSize: 14, color: t.ink2),
            ),
          ),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              for (final x in sessions)
                SizedBox(
                  width: (MediaQuery.sizeOf(context).width - 32 - 12) / 2,
                  child: Semantics(
                    button: true,
                    label: '${x.name}. ${x.ref}',
                    excludeSemantics: true,
                    child: Glass(
                      radius: 20,
                      tint: x.needsYou
                          ? Color.alphaBlend(
                              t.accent.withValues(alpha: .10),
                              t.surface,
                            )
                          : null,
                      child: InkWell(
                        borderRadius: BorderRadius.circular(20),
                        onTap: () => openSession(context, x),
                        child: Padding(
                          padding: const EdgeInsets.fromLTRB(8, 14, 8, 12),
                          child: Column(
                            children: [
                              PetView(
                                design: x.petDesign,
                                color: x.petColor,
                                state: x.petState,
                                size: 72,
                                halo: x.needsYou,
                              ),
                              const SizedBox(height: 4),
                              Text(
                                x.name,
                                textAlign: TextAlign.center,
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 15,
                                  fontWeight: FontWeight.w700,
                                  height: 1.25,
                                  color: t.ink,
                                ),
                              ),
                              Text(
                                x.ref,
                                style: TextStyle(fontSize: 12, color: t.ink2),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ],
      ],
    );
  }
}
