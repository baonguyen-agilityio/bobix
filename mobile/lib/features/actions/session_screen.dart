import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';
import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import 'action_screen_parts.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'command_flow.dart';
import 'activity_lines.dart';
import 'request_screen.dart';

final sessionProvider = FutureProvider.autoDispose
    .family<SessionDetail, String>(
      (ref, id) => ref.watch(actionsSourceProvider).session(id),
    );

/// One session: what it does now, its last lines (read only, only when the computer granted that right), and Stop
/// (only when `home` says it is possible: the session waits on a request held for this phone and the phone has both
/// the Stop and the Answer right).
class SessionScreen extends ConsumerWidget {
  const SessionScreen({super.key, required this.id});
  final String id;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final canSend = ref.watch(canSendProvider);
    final c = ref.watch(actionsSourceProvider).computerName;
    final session = ref.watch(sessionProvider(id));
    return session.when(
      loading: () => ActionPage(
        title: s('phone.sv.title'),
        children: const [
          SizedBox(height: 80),
          Center(child: CircularProgressIndicator()),
        ],
      ),
      error: (e, _) =>
          ActionPage(title: s('phone.sv.title'), children: [Text('$e')]),
      data: (d) {
        final (String chip, ChipTone tone) = switch (d.phase) {
          SessionPhase.working => (
            d.tool.isEmpty
                ? s('phone.st.working0')
                : s('phone.st.working', {'tool': d.tool}),
            ChipTone.info,
          ),
          SessionPhase.waiting => (s('phone.status.waiting'), ChipTone.accent),
          SessionPhase.idle => (s('phone.status.idle'), ChipTone.plain),
        };
        return ActionPage(
          title: s('phone.sv.title'),
          onLog: taskLogOpener(context, ref, d.id),
          // Stop is absent, not greyed, when `home` does not allow it; it is off only while sending is off.
          actions: d.stopAllowed
              ? ActionButtons(
                  primary: ActionButton(
                    s('phone.sv.stop'),
                    kind: BtnKind.danger,
                    icon: Icons.lock_outline,
                    onPressed: !canSend
                        ? null
                        : () => runCommand(
                            context,
                            ref,
                            kind: CommandKind.stop,
                            target: d.id,
                            shown: d.shownHash,
                            // Stop names the request it denies.
                            text: d.waitingOnRequest,
                            reason: s('phone.un.stop', {'c': c}),
                            project: d.project,
                          ),
                  ),
                  fine: canSend
                      ? s('phone.sv.stop.d', {'c': c})
                      : s('phone.sv.stop.paused'),
                )
              : null,
          children: [
            const SendOffBanner(),
            ActionSessionCard(
              id: d.id,
              name: d.name,
              chip: chip,
              tone: tone,
              where: d.issue == null ? d.project : '${d.project} #${d.issue}',
            ),
            // Waiting, but nothing is held for this phone: it can only be answered on the computer, and says so.
            // Not when a plan or a question is held for this phone: Home opens those on their own screens.
            if (d.phase == SessionPhase.waiting &&
                d.waitingOnRequest == null &&
                ref.watch(homeSessionProvider(d.id))?.waitingId == null) ...[
              const SizedBox(height: 12),
              _AtComputerNote(s('phone.sv.atpc.d', {'c': c})),
            ],
            if (d.waitingOnRequest != null) ...[
              const SizedBox(height: 12),
              ActionButton(
                s('phone.sv.open'),
                icon: Icons.chevron_right,
                onPressed: () => Navigator.of(
                  context,
                ).push(pageRoute<void>(RequestScreen(id: d.waitingOnRequest!))),
              ),
            ],
            SessionActivity(
              lines: d.lines,
              at: d.linesAt,
              empty: s('phone.sv.lines.empty'),
              limit: 3,
              onSeeAll: taskLogOpener(context, ref, d.id),
            ),
          ],
        );
      },
    );
  }
}

/// Why a waiting session has nothing to answer on the phone.
class _AtComputerNote extends StatelessWidget {
  const _AtComputerNote(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: t.accentBg,
        borderRadius: BorderRadius.circular(16),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.desktop_mac_outlined, size: 18, color: t.accentInk),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              text,
              style: TextStyle(fontSize: 14, height: 1.45, color: t.ink),
            ),
          ),
        ],
      ),
    );
  }
}
