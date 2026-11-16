import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../features/actions/action_screen_parts.dart';
import '../features/actions/action_widgets.dart';
import '../features/actions/actions_source.dart';
import '../features/actions/activity_lines.dart';
import '../features/actions/session_screen.dart' show sessionProvider;
import '../l10n/l10n.dart';
import '../lock/secure_scope.dart';
import '../log/activity_log.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/glass.dart';

/// The log of one task: every command sent from this phone for the session, newest first. Kept in memory for this run
/// of the app only.
class TaskLogScreen extends ConsumerWidget {
  const TaskLogScreen({super.key, required this.sessionId});
  final String sessionId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.watch(actionsSourceProvider).computerName;
    return SecureScope(
      child: DetailScaffold(
        title: s('phone.tl.title'),
        children: [
          ActionSessionCard(id: sessionId),
          // What the session did lately, read from the computer; empty (and absent) when this phone may not read it.
          SessionActivity(
            lines: ref.watch(sessionProvider(sessionId)).value?.lines,
            at: ref.watch(sessionProvider(sessionId)).value?.linesAt ?? '',
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(0, 12, 0, 4),
            child: Text(
              s('phone.tl.n', {'c': c}),
              style: TextStyle(fontSize: 14, height: 1.5, color: t.ink2),
            ),
          ),
          LogEntries(
            entries: ref.watch(taskLogProvider(sessionId)),
            empty: s('phone.tl.empty'),
          ),
        ],
      ),
    );
  }
}

/// "Today N" and the entries in one glass card; the empty text when there are none.
class LogEntries extends ConsumerWidget {
  const LogEntries({super.key, required this.entries, required this.empty});
  final List<LogEntry> entries;
  final String empty;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionHeader(s('phone.lg.today'), count: entries.length),
        if (entries.isEmpty)
          Glass(
            padding: const EdgeInsets.all(18),
            child: Center(
              child: Text(
                empty,
                textAlign: TextAlign.center,
                style: TextStyle(fontSize: 14, height: 1.5, color: t.ink2),
              ),
            ),
          )
        else
          Glass(
            child: Column(
              children: [
                for (var i = 0; i < entries.length; i++) ...[
                  if (i > 0) Divider(height: .5, thickness: .5, color: t.line),
                  _LogRow(entries[i]),
                ],
              ],
            ),
          ),
      ],
    );
  }
}

class _LogRow extends ConsumerWidget {
  const _LogRow(this.e);
  final LogEntry e;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final phone = ref.watch(phoneNameProvider);
    final isApp = e.kind == LogKind.pair || e.kind == LogKind.disconnect;
    final isTool = e.kind == LogKind.allow || e.kind == LogKind.deny;
    final title = switch (e.kind) {
      LogKind.allow => s('phone.lg.allow', {'tool': _tool(s)}),
      LogKind.deny => s('phone.lg.deny', {'tool': _tool(s)}),
      LogKind.approve => s('phone.lg.approve'),
      LogKind.back => s('phone.lg.back'),
      LogKind.answer => s('phone.lg.answer'),
      LogKind.stop => s('phone.lg.stop'),
      LogKind.start => s('phone.lg.start.full'),
      LogKind.startQuick => s('phone.lg.start.quick'),
      LogKind.pair => s('phone.lg.pair', {'c': e.detail}),
      LogKind.disconnect => s('phone.lg.disc'),
    };
    final at =
        '${e.at.hour.toString().padLeft(2, '0')}:${e.at.minute.toString().padLeft(2, '0')}';
    final meta = [at, ?phone, if (e.project.isNotEmpty) e.project].join(' · ');
    final showDetail = e.detail.isNotEmpty && e.kind != LogKind.pair;
    return Padding(
      padding: const EdgeInsets.all(14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Semantics(
                  header: true,
                  child: Text(
                    title,
                    style: TextStyle(
                      fontSize: 14,
                      fontWeight: FontWeight.w600,
                      height: 1.3,
                      color: t.ink,
                    ),
                  ),
                ),
              ),
              if (!isApp) ...[
                const SizedBox(width: 8),
                _Badge(applied: e.applied, s: s),
              ],
            ],
          ),
          const SizedBox(height: 6),
          Text(meta, style: TextStyle(fontSize: 12, color: t.ink3)),
          if (showDetail) ...[
            const SizedBox(height: 6),
            if (isTool)
              CodeBlock(e.detail, fontSize: 12, maxHeight: 54)
            else
              Text(e.detail, style: TextStyle(fontSize: 12, color: t.ink2)),
          ],
        ],
      ),
    );
  }

  String _tool(Strings s) => e.tool.isEmpty ? s('phone.ask.tool0') : e.tool;
}

class _Badge extends StatelessWidget {
  const _Badge({required this.applied, required this.s});
  final bool applied;
  final Strings s;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 1),
      constraints: const BoxConstraints(minHeight: 22),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: applied ? t.okBg : t.warnBg,
        borderRadius: BorderRadius.circular(11),
      ),
      child: Text(
        applied ? s('phone.lg.r.ok') : s('phone.lg.r.no'),
        style: TextStyle(
          fontSize: 12,
          fontWeight: FontWeight.w600,
          color: applied ? t.ok : t.warn,
        ),
      ),
    );
  }
}
