import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';
import '../../data/models.dart';
import '../../data/phone_data.dart';
import '../../l10n/l10n.dart';
import '../../link/link_providers.dart';
import '../../screens/task_log_screen.dart';
import '../../theme/tokens.dart';
import 'action_widgets.dart';
import 'actions_source.dart';

/// The Home session an action screen belongs to: [id] is the session's own id or the id of what it waits on (a request,
/// a plan, a question). Null while Home has not told, or when the item is not one of its sessions.
Session? sessionFor(HomeData? home, String id) {
  if (home == null) return null;
  for (final x in home.sessions) {
    if (x.waitingId == id || x.id == id) return x;
  }
  return null;
}

final homeSessionProvider = Provider.family<Session?, String>(
  (ref, id) => sessionFor(ref.watch(homeProvider).value, id),
);

/// The name this phone was given when it paired; null when no computer is paired.
final phoneNameProvider = Provider<String?>(
  (ref) => ref.watch(pairedComputerProvider)?.phoneName,
);

/// Opens the log of the task [id] belongs to.
VoidCallback taskLogOpener(BuildContext context, WidgetRef ref, String id) =>
    () => Navigator.of(context).push(
      pageRoute<void>(
        TaskLogScreen(
          sessionId: sessionFor(ref.read(homeProvider).value, id)?.id ?? id,
        ),
      ),
    );

/// "waiting 5 min".
String waitedText(Strings s, int minutes) => s('phone.home.waited', {
  'n': s('phone.min', {'n': minutes}),
});

enum ChipTone { accent, info, plain }

/// The header card of the action screens: the session's pet, name, where it belongs, and what it is doing. Reads what
/// Home knows about the session; without it the pet is the default one and only [name] is shown.
class ActionSessionCard extends ConsumerWidget {
  const ActionSessionCard({
    super.key,
    required this.id,
    this.name,
    this.chip,
    this.tone = ChipTone.accent,
    this.showIssue = true,
    this.where,
    this.extraChips = const [],
  });

  /// The id the screen was opened with (see [sessionFor]).
  final String id;

  /// What the screen's own data calls the session; Home's name wins when it has one.
  final String? name;
  final String? chip;
  final ChipTone tone;

  /// "project #12" (true) or just "project" (a permission request).
  final bool showIssue;

  /// "project #12" as the screen's own data says it, for when Home does not know the session.
  final String? where;
  final List<Widget> extraChips;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final x = ref.watch(homeSessionProvider(id));
    final title = x?.name ?? name;
    if (title == null) return const SizedBox.shrink();
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.watch(actionsSourceProvider).computerName;
    final on = s('phone.rq.on', {'c': c});
    final place = x == null ? where : (showIssue ? x.ref : x.project);
    final (Color fg, Color bg) = switch (tone) {
      ChipTone.accent => (t.accentInk, t.accentBg),
      ChipTone.info => (t.info, t.infoBg),
      ChipTone.plain => (t.ink2, t.surface3),
    };
    return SessionCard(
      petDesign: x?.petDesign ?? 'default',
      petColor: x?.petColor ?? 0,
      petState: x?.petState ?? 'waiting',
      halo: (x?.petState ?? 'waiting') == 'waiting',
      title: title,
      subtitle: place == null || place.isEmpty ? on : '$place · $on',
      chip: chip,
      chipColor: fg,
      chipBg: bg,
      extraChips: extraChips,
    );
  }
}

/// What the line under the buttons says about sending: why it is off, or that the computer logs it.
String sendNote(Strings s, ConnState conn, String computer) => switch (conn) {
  ConnState.noNetwork => s('phone.sn.nonet'),
  ConnState.relayDown => s('phone.sn.relay'),
  ConnState.asleep || ConnState.offline => s('phone.sn.held'),
  ConnState.ok => s('phone.sn.ok', {'c': computer}),
};

/// The action bar of the prototype: the secondary button on the left, the primary on the right (equal widths) or one button
/// alone, and a fine print under them.
class ActionButtons extends StatelessWidget {
  const ActionButtons({
    super.key,
    required this.primary,
    this.secondary,
    this.fine,
    this.lock = false,
    this.note,
  });

  final Widget primary;
  final Widget? secondary;
  final String? fine;

  /// A line of fine print above [fine]: what the buttons cover (no lock).
  final String? note;

  /// A lock before the fine print (the command asks for the biometric).
  final bool lock;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (secondary == null)
          SizedBox(width: double.infinity, child: primary)
        else
          Row(
            children: [
              Expanded(child: secondary!),
              const SizedBox(width: 10),
              Expanded(child: primary),
            ],
          ),
        if (note != null) ...[const SizedBox(height: 8), FinePrint(note!)],
        if (fine != null) ...[
          const SizedBox(height: 8),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              if (lock) ...[
                Icon(Icons.lock_outline, size: 14, color: t.ink3),
                const SizedBox(width: 6),
              ],
              Flexible(child: FinePrint(fine!)),
            ],
          ),
        ],
      ],
    );
  }
}

/// A note with the shield, left aligned: what the app promises about this screen.
class ShieldNote extends StatelessWidget {
  const ShieldNote(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Padding(
        padding: const EdgeInsets.only(top: 1),
        child: Icon(
          Icons.shield_outlined,
          size: 16,
          color: context.tokens.ink3,
        ),
      ),
      const SizedBox(width: 6),
      Expanded(child: FinePrint(text, align: TextAlign.left)),
    ],
  );
}

/// The heading under the session card ("Bash wants to run", "Plan").
class ScreenHeading extends StatelessWidget {
  const ScreenHeading(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(0, 14, 0, 10),
    child: Semantics(
      header: true,
      child: Text(
        text,
        style: TextStyle(
          fontSize: 20,
          fontWeight: FontWeight.w700,
          height: 1.25,
          letterSpacing: -.2,
          color: context.tokens.ink,
        ),
      ),
    ),
  );
}
