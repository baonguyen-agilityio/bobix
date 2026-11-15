import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'command_flow.dart';
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
    final t = context.tokens;
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
        final badge = switch (d.phase) {
          SessionPhase.working => (s('phone.st.working0'), t.info, t.infoBg),
          SessionPhase.waiting => (
            s('phone.status.waiting'),
            t.accentInk,
            t.accentBg,
          ),
          SessionPhase.idle => (s('phone.status.idle'), t.ink2, t.surface3),
        };
        return ActionPage(
          title: s('phone.sv.title'),
          // Stop is absent, not greyed, when `home` does not allow it; it is off only while sending is off.
          actions: d.stopAllowed
              ? Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    SizedBox(
                      width: double.infinity,
                      child: ActionButton(
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
                                reason: s('phone.un.stop', {'c': c}),
                              ),
                      ),
                    ),
                    const SizedBox(height: 8),
                    FinePrint(
                      canSend
                          ? s('phone.sv.stop.d', {'c': c})
                          : s('phone.sv.stop.paused'),
                    ),
                  ],
                )
              : null,
          children: [
            const SendOffBanner(),
            TitleCard(
              title: d.name,
              subtitle:
                  '${d.project} #${d.issue} · ${s('phone.rq.on', {'c': c})}',
              icon: Icons.bolt_outlined,
              chips: [
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 3,
                  ),
                  decoration: BoxDecoration(
                    color: badge.$3,
                    borderRadius: BorderRadius.circular(11),
                  ),
                  child: Text(
                    badge.$1,
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: badge.$2,
                    ),
                  ),
                ),
              ],
            ),
            if (d.waitingOnRequest != null) ...[
              const SizedBox(height: 12),
              ActionButton(
                s('phone.sv.open'),
                icon: Icons.chevron_right,
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) => RequestScreen(id: d.waitingOnRequest!),
                  ),
                ),
              ),
            ],
            if (d.lines != null) ...[
              Padding(
                padding: const EdgeInsets.fromLTRB(0, 18, 0, 8),
                child: Semantics(
                  header: true,
                  child: Text(
                    s('phone.sv.act'),
                    style: TextStyle(
                      fontSize: 15,
                      fontWeight: FontWeight.w600,
                      color: t.ink,
                    ),
                  ),
                ),
              ),
              CodeBlock(
                d.lines!.isEmpty
                    ? s('phone.sv.lines.empty')
                    : d.lines!.join('\n'),
                fontSize: 12,
              ),
              const SizedBox(height: 8),
              FinePrint(
                s('phone.sv.act.n', {'c': c, 'tm': d.linesAt}),
                align: TextAlign.left,
              ),
            ],
          ],
        );
      },
    );
  }
}
