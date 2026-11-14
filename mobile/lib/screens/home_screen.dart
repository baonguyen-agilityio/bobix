import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../fixtures/fixtures.dart';
import '../l10n/l10n.dart';
import '../theme/tokens.dart';
import '../widgets/glass.dart';
import '../widgets/session_row.dart';
import '../widgets/status_banner.dart';

/// What the connection looks like to the person; the real connection replaces this provider.
class Conn extends Notifier<ConnState> {
  @override
  ConnState build() => ConnState.values.firstWhere(
    (c) => c.name == const String.fromEnvironment('BOBIX_FIXTURE_CONN'),
    orElse: () => ConnState.ok,
  );
}

final connProvider = NotifierProvider<Conn, ConnState>(Conn.new);

class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final conn = ref.watch(connProvider);
    final t = context.tokens;
    const clock = '21:02';

    final need = fixtureSessions.where((x) => x.needsYou).toList();
    String minutes(int n) => s('phone.min', {'n': n});

    Widget row(
      Session x,
      String subtitle, {
      String? meta,
      bool needsYou = false,
      Color? subtitleColor,
    }) => Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: SessionRow(
        petDesign: x.petDesign,
        petColor: x.petColor,
        petState: needsYou
            ? 'waiting'
            : switch (x.stage) {
                Stage.doing => 'working',
                Stage.verify => 'idle',
                Stage.stuck => 'failed',
                _ => 'done',
              },
        title: x.name,
        subtitle: subtitle,
        meta: meta ?? '${x.project} #${x.issue}',
        needsYou: needsYou,
        subtitleColor: subtitleColor,
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
        title: s('phone.bn.asleep', {'c': fixtureComputer}),
        body: s('phone.bn.asleep.d', {'tm': clock}),
      ),
      ConnState.offline => StatusBanner(
        kind: BannerKind.warn,
        icon: Icons.computer,
        title: s('phone.bn.offline', {'c': fixtureComputer}),
        body: s('phone.bn.offline.d', {'tm': clock}),
      ),
    };

    String stageName(Stage g) => s('phone.stage.${g.name}');
    List<Widget> stage(Stage g) {
      final items = fixtureSessions
          .where((x) => x.stage == g && !x.needsYou)
          .toList();
      if (items.isEmpty) return const [];
      return [
        SectionHeader(stageName(g), count: items.length),
        for (final x in items)
          row(x, switch (g) {
            Stage.doing =>
              x.tool == null
                  ? s('phone.st.working0')
                  : s('phone.st.working', {'tool': x.tool!}),
            Stage.verify => s('phone.st.pr', {'n': x.pullRequest!}),
            Stage.stuck => s('phone.stage.stuck'),
            _ => s('phone.st.closed'),
          }, subtitleColor: g == Stage.stuck ? t.err : null),
      ];
    }

    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 96),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 6, 0, 8),
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
              Glass(
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
                        fixtureComputer,
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
              s('phone.ask.h', {'tool': x.tool!}),
              meta:
                  '${x.project} · ${s('phone.home.waited', {'n': minutes(x.waitingMinutes)})}',
              needsYou: true,
            ),
        ],
        for (final g in Stage.values) ...stage(g),
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
