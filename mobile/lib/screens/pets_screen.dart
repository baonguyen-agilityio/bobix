import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/models.dart';
import '../data/phone_data.dart';
import '../features/actions/action_widgets.dart' show FinePrint;
import '../l10n/l10n.dart';
import '../pets/pet_view.dart';
import '../theme/tokens.dart';
import '../widgets/glass.dart';
import 'home_screen.dart';
import '../widgets/press_feedback.dart';

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
          for (var i = 0; i < sessions.length; i += 2)
            Padding(
              padding: const EdgeInsets.only(bottom: 10),
              child: IntrinsicHeight(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Expanded(child: _PetCard(sessions[i])),
                    const SizedBox(width: 10),
                    Expanded(
                      child: i + 1 < sessions.length
                          ? _PetCard(sessions[i + 1])
                          : const SizedBox(),
                    ),
                  ],
                ),
              ),
            ),
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: FinePrint(s('phone.pets.n')),
          ),
        ],
      ],
    );
  }
}

enum _Chip { neutral, accent, ok, info, err }

/// A session as a card: its pet, the project, the session's name and a chip that says what it is doing.
class _PetCard extends ConsumerWidget {
  const _PetCard(this.x);

  final Session x;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final (text, kind) = switch (x.petState) {
      _ when x.stage == Stage.stuck && !x.needsYou => (
        s('phone.st.stuck'),
        _Chip.err,
      ),
      'waiting' => (
        x.answerAtComputer
            ? s('phone.status.atpc')
            : x.waitingKind == null || x.waitingKind == WaitingKind.request
            ? s('phone.status.waiting')
            : waitingText(s, x),
        x.answerAtComputer ? _Chip.neutral : _Chip.accent,
      ),
      'working' => (
        x.tool == null
            ? s('phone.st.working0')
            : s('phone.st.working', {'tool': x.tool!}),
        _Chip.info,
      ),
      'done' => (s('phone.pet.done'), _Chip.ok),
      _ => (s('phone.status.idle'), _Chip.neutral),
    };
    final (bg, fg) = switch (kind) {
      _Chip.accent => (t.accentBg, t.accentInk),
      _Chip.ok => (t.okBg, t.ok),
      _Chip.info => (t.infoBg, t.info),
      _Chip.err => (t.err.withValues(alpha: .14), t.err),
      _Chip.neutral => (t.surface3, t.ink2),
    };
    return Semantics(
      button: true,
      label: '${x.project}. ${x.name}. $text',
      excludeSemantics: true,
      child: Glass(
        radius: 16,
        tint: x.answersHere
            ? Color.alphaBlend(t.accent.withValues(alpha: .10), t.surface)
            : null,
        child: PressFeedback(
          scale: .985,
          opacity: .75,
          child: InkWell(
            borderRadius: BorderRadius.circular(16),
            onTap: () => openSession(context, x),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(8, 14, 8, 12),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  PetView(
                    design: x.petDesign,
                    color: x.petColor,
                    state: x.petState,
                    size: 96,
                    halo: x.answersHere,
                  ),
                  const SizedBox(height: 4),
                  Text(
                    x.project,
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 15,
                      fontWeight: FontWeight.w700,
                      height: 1.25,
                      color: t.ink,
                    ),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    x.name,
                    textAlign: TextAlign.center,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 12, height: 1.3, color: t.ink2),
                  ),
                  const SizedBox(height: 4),
                  Container(
                    constraints: const BoxConstraints(minHeight: 22),
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 1,
                    ),
                    decoration: BoxDecoration(
                      color: bg,
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: Text(
                      text,
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                        height: 1.3,
                        color: fg,
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
