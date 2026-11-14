import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/models.dart';
import '../data/phone_data.dart';
import '../l10n/l10n.dart';
import '../widgets/command_block.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/status_banner.dart';
import '../widgets/who_card.dart';
import 'request_screen.dart';

/// A session: where it stands, the request it waits on, and the last lines the computer sent. Read only.
class SessionScreen extends ConsumerWidget {
  const SessionScreen({super.key, required this.sessionId});

  final String sessionId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final detail = ref.watch(sessionProvider(sessionId)).value;
    final computer = ref.watch(homeProvider).value?.computer ?? '';
    final title = s('phone.sv.title');
    if (detail == null) {
      return DetailScaffold.loading(title: title);
    }

    final x = detail.session;
    final (label, tone) = statusOf(s, x);
    return DetailScaffold(
      title: title,
      children: [
        WhoCard(
          session: x,
          subtitle:
              '${x.project} #${x.issue} · ${s('phone.rq.on', {'c': computer})}',
          badges: [Pill(label, tone: tone)],
        ),
        if (x.answerAtComputer)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: StatusBanner(
              kind: BannerKind.info,
              icon: Icons.computer,
              title: s('phone.home.atpc'),
              body: s('phone.sv.atpc.d', {'c': computer}),
            ),
          )
        else if (x.needsYou)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: PillButton(
              compact: true,
              label: s('phone.ask.h', {'tool': x.tool ?? ''}),
              icon: Icons.chevron_right,
              onPressed: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => RequestScreen(sessionId: x.id),
                ),
              ),
            ),
          ),
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 16, 0, 8),
          child: Semantics(
            header: true,
            child: Text(
              s('phone.sv.act'),
              style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
            ),
          ),
        ),
        CommandBlock(detail.activity.join('\n'), fontSize: 12),
        Padding(
          padding: const EdgeInsets.only(top: 8),
          child: Text(
            s('phone.sv.act.n', {'c': computer, 'tm': detail.lastContact}),
            style: TextStyle(
              fontSize: 12,
              height: 1.35,
              color: Theme.of(context).hintColor,
            ),
          ),
        ),
      ],
    );
  }
}

/// The badge text and tone of a session's state.
(String, PillTone) statusOf(Strings s, Session x) {
  if (x.stage == Stage.stuck) return (s('phone.st.stuck'), PillTone.err);
  if (x.needsYou) {
    return (
      x.answerAtComputer ? s('phone.home.atpc') : s('phone.status.waiting'),
      PillTone.accent,
    );
  }
  return switch (x.stage) {
    Stage.doing => (
      x.tool == null
          ? s('phone.st.working0')
          : s('phone.st.working', {'tool': x.tool!}),
      PillTone.info,
    ),
    Stage.done => (s('phone.pet.done'), PillTone.ok),
    _ => (s('phone.status.idle'), PillTone.neutral),
  };
}
