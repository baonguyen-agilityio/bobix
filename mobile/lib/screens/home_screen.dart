import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../widgets/glass_backdrop.dart';
import '../data/models.dart';
import '../data/phone_data.dart';
import '../l10n/l10n.dart';
import '../log/activity_log.dart';
import '../theme/theme.dart';
import '../theme/tokens.dart';
import '../widgets/glass.dart';
import '../widgets/session_row.dart';
import '../widgets/status_banner.dart';
import 'computers_sheet.dart';
import '../features/actions/plan_screen.dart';
import '../features/actions/question_screen.dart';
import '../features/actions/request_screen.dart';
import '../features/actions/action_screen_parts.dart' show taskLogOpener;
import '../features/actions/actions_models.dart';
import '../features/actions/session_screen.dart';
import '../features/actions/start_screen.dart';
import '../pets/random_pet.dart';
import '../widgets/press_feedback.dart';

/// What a tap opens: the held item (a request, a plan, a question) or, for a request answered elsewhere first, that
/// request's closed view; otherwise the session.
void openSession(BuildContext context, Session x) {
  final id = x.waitingId;
  final Widget page = id == null
      ? SessionScreen(id: x.id)
      // Only the request screen has the closed view that says who answered.
      : x.answeredAtComputer
      ? RequestScreen(id: id)
      : switch (x.waitingKind) {
          WaitingKind.plan => PlanScreen(id: id),
          WaitingKind.question => QuestionScreen(id: id),
          _ => RequestScreen(id: id),
        };
  Navigator.of(context).push(pageRoute<void>(page));
}

/// What a waiting session asks for, in one line (what Home and Pets show under its name).
String waitingText(Strings s, Session x) => switch (x.waitingKind) {
  WaitingKind.plan => s('phone.ask.plan.h'),
  WaitingKind.question => s('phone.ask.q.h'),
  _ => x.tool == null ? s('phone.ask.h0') : s('phone.ask.h', {'tool': x.tool!}),
};

class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final data = ref.watch(homeProvider).value;
    if (data == null) return const PetsSplash();
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final conn = data.conn, clock = data.lastContact, computer = data.computer;

    final need = data.sessions.where((x) => x.answersHere).toList();
    // Claude waits, but asks in its own prompt at the computer: shown apart, never counted as needing this phone.
    final atComputer = data.sessions
        .where((x) => x.needsYou && x.answerAtComputer)
        .toList();
    String minutes(int n) => s('phone.min', {'n': n});

    Widget row(
      Session x,
      String subtitle, {
      String? meta,
      Color? subtitleColor,
      bool muted = false,
    }) => Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: SessionRow(
        petDesign: x.petDesign,
        petColor: x.petColor,
        petState: x.petState,
        title: x.name,
        subtitle: subtitle,
        meta: meta ?? x.ref,
        // What the Task log lists: Claude's activity and the commands sent from phones.
        logCount: ref.watch(taskLogProvider(x.id)).length + x.activityLines,
        logLabel: s('phone.tl.open', {'n': x.name}),
        onLog: taskLogOpener(context, ref, x.id),
        needsYou: x.answersHere,
        muted: muted,
        subtitleColor: subtitleColor,
        onTap: () => openSession(context, x),
      ),
    );

    final banner = switch (conn) {
      ConnState.ok => null,
      ConnState.noNetwork => StatusBanner(
        kind: BannerKind.warn,
        icon: Icons.wifi_off,
        title: s('phone.bn.nonet'),
        body: s('phone.bn.nonet.d', {'tm': clock}),
      ),
      ConnState.relayDown => StatusBanner(
        kind: BannerKind.err,
        icon: Icons.cloud_off,
        title: s('phone.bn.relay'),
        body: s('phone.bn.relay.d'),
        action: OutlinedButton(
          onPressed: () {},
          style: OutlinedButton.styleFrom(minimumSize: const Size(44, 44)),
          child: Text(s('phone.bn.retry')),
        ),
      ),
      ConnState.asleep => StatusBanner(
        kind: BannerKind.warn,
        icon: Icons.bedtime_outlined,
        title: s('phone.bn.asleep', {'c': computer}),
        body: s('phone.bn.asleep.d', {'tm': clock}),
      ),
      ConnState.offline => StatusBanner(
        kind: BannerKind.warn,
        icon: Icons.computer,
        title: s('phone.bn.offline', {'c': computer}),
        body: s('phone.bn.offline.d', {'tm': clock}),
      ),
    };

    final closed = data.sessions.where((x) => x.answeredAtComputer).toList();

    final issues = ref.watch(startIssuesProvider).value ?? const [];

    List<Widget> stage(Stage g) {
      final items = data.sessions
          .where((x) => x.stage == g && !x.needsYou && !x.answeredAtComputer)
          .toList();
      if (items.isEmpty) return const [];
      return [
        SectionHeader(s('phone.stage.${g.name}'), count: items.length),
        for (final x in items)
          row(x, switch (g) {
            Stage.doing =>
              x.tool == null
                  ? s('phone.st.working0')
                  : s('phone.st.working', {'tool': x.tool!}),
            Stage.verify => s('phone.st.pr', {'n': x.pullRequest ?? 0}),
            Stage.stuck => s('phone.st.stuck'),
            _ => s('phone.st.closed'),
          }, subtitleColor: g == Stage.stuck ? t.err : null),
      ];
    }

    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 96),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 0, 0, 16),
          child: Row(
            children: [
              Expanded(
                child: Semantics(
                  header: true,
                  child: Text(
                    s('phone.tab.home'),
                    style: TextStyle(
                      fontSize: 28,
                      fontWeight: FontWeight.w700,
                      letterSpacing: -.5,
                      color: t.ink,
                    ),
                  ),
                ),
              ),
              Semantics(
                button: true,
                label: computer,
                child: PressFeedback(
                  scale: .985,
                  opacity: .75,
                  child: InkWell(
                    borderRadius: BorderRadius.circular(22),
                    onTap: () => showComputersSheet(
                      context,
                      computer: computer,
                      waiting: need.length,
                    ),
                    child: Glass(
                      blur: true,
                      radius: 22,
                      padding: const EdgeInsets.symmetric(horizontal: 12),
                      child: SizedBox(
                        height: 44,
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Container(
                              width: 8,
                              height: 8,
                              decoration: BoxDecoration(
                                color: conn == ConnState.ok ? t.ok : t.warn,
                                shape: BoxShape.circle,
                              ),
                            ),
                            const SizedBox(width: 8),
                            Text(
                              computer,
                              style: TextStyle(
                                fontSize: 13.5,
                                fontWeight: FontWeight.w600,
                                color: t.ink,
                              ),
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
        ),
        ?banner,
        if (need.isEmpty)
          Glass(
            padding: const EdgeInsets.all(16),
            child: Column(
              children: [
                Text(
                  s('phone.home.none'),
                  style: TextStyle(fontWeight: FontWeight.w700, color: t.ink),
                ),
                const SizedBox(height: 4),
                Text(
                  s('phone.home.none.d'),
                  textAlign: TextAlign.center,
                  style: TextStyle(color: t.ink2),
                ),
              ],
            ),
          )
        else ...[
          SectionHeader(
            s('phone.home.need'),
            count: need.length,
            needsYou: true,
          ),
          for (final x in need)
            row(
              x,
              x.handledAtComputer ? s('phone.hc.short') : waitingText(s, x),
              meta:
                  '${x.project} · ${s('phone.home.waited', {'n': minutes(x.waitingMinutes)})}',
            ),
        ],
        if (atComputer.isNotEmpty) ...[
          SectionHeader(s('phone.home.atpc.h'), count: atComputer.length),
          for (final x in atComputer) row(x, s('phone.home.atpc'), muted: true),
        ],
        if (closed.isNotEmpty) ...[
          SectionHeader(s('phone.home.earlier'), count: closed.length),
          for (final x in closed)
            row(x, s('phone.cl.pc.short'), muted: true, subtitleColor: t.warn),
        ],
        for (final g in const [Stage.doing, Stage.verify, Stage.stuck])
          ...stage(g),
        if (issues.isNotEmpty) ...[
          SectionHeader(s('phone.stage.todo'), count: issues.length),
          for (final i in issues)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: _TodoRow(
                issue: i,
                onTap: () => ref.read(startRequestProvider.notifier).open(i.id),
              ),
            ),
        ],
        ...stage(Stage.done),
        if (conn != ConnState.ok)
          Padding(
            padding: const EdgeInsets.only(top: 10),
            child: Text(
              s('phone.home.last', {'tm': clock}),
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 12, color: t.ink3),
            ),
          ),
      ],
    );
  }
}

/// An issue the computer lists to start: a ticket tile, its title and where it belongs. A tap opens Start with it picked.
class _TodoRow extends StatelessWidget {
  const _TodoRow({required this.issue, required this.onTap});

  final StartIssue issue;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      button: true,
      label: '${issue.title}. ${issue.project} #${issue.number}',
      excludeSemantics: true,
      child: Glass(
        child: PressFeedback(
          scale: .985,
          opacity: .75,
          child: InkWell(
            borderRadius: BorderRadius.circular(16),
            onTap: onTap,
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: minTouch + 17),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(8, 8, 12, 8),
                child: Row(
                  children: [
                    Container(
                      width: 44,
                      height: 44,
                      decoration: BoxDecoration(
                        color: t.surface3,
                        borderRadius: BorderRadius.circular(44 * 16 / 62),
                      ),
                      child: Icon(
                        Icons.confirmation_number_outlined,
                        size: 22,
                        color: t.ink2,
                      ),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            issue.title,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 15,
                              fontWeight: FontWeight.w600,
                              color: t.ink,
                              height: 1.3,
                            ),
                          ),
                          Text(
                            '${issue.project} #${issue.number}',
                            style: TextStyle(fontSize: 12, color: t.ink3),
                          ),
                        ],
                      ),
                    ),
                    Icon(Icons.chevron_right, size: 18, color: t.ink3),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
