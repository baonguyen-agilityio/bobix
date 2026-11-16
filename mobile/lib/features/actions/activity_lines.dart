import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import 'action_widgets.dart' show FinePrint;
import 'hidden_chars.dart' show showEscapes;
import 'actions_source.dart';

/// One line of a session's activity split into its time, the tool it names and what it did. The computer sends plain
/// lines like `21:13 Edit src/app.js` or `Bash ls -la`: an optional time, a first word that looks like a tool name, then
/// the rest.
({String time, String tool, String text}) splitActivityLine(String line) {
  var rest = line.trim();
  var time = '';
  final t = RegExp(
    r'^(\d{1,2}:\d{2}(?::\d{2})?)\s+(.*)$',
    dotAll: true,
  ).firstMatch(rest);
  if (t != null) {
    time = t.group(1)!;
    rest = t.group(2)!;
  }
  final m = RegExp(
    r'^([A-Za-z][A-Za-z0-9_]*)(?:\s+(.*))?$',
    dotAll: true,
  ).firstMatch(rest);
  if (m == null) return (time: time, tool: '', text: rest);
  var tool = m.group(1)!;
  // `mcp__server__send_message` reads as `send_message`.
  if (tool.startsWith('mcp__')) tool = tool.split('__').last;
  return (time: time, tool: tool, text: m.group(2) ?? '');
}

/// A session's last lines as a list a person can read: the tool as a small label, what it did on up to two lines.
/// Control and format characters stay visible escapes, as in a code block.
class ActivityLines extends StatelessWidget {
  const ActivityLines(this.lines, {super.key, required this.empty});
  final List<String> lines;
  final String empty;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    if (lines.isEmpty) {
      return Glass(
        padding: const EdgeInsets.all(18),
        child: Center(
          child: Text(
            empty,
            textAlign: TextAlign.center,
            style: TextStyle(fontSize: 14, height: 1.5, color: t.ink2),
          ),
        ),
      );
    }
    return Glass(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (var i = 0; i < lines.length; i++) ...[
            if (i > 0) Divider(height: .5, thickness: .5, color: t.line),
            _Row(splitActivityLine(lines[i])),
          ],
        ],
      ),
    );
  }
}

class _Row extends StatelessWidget {
  const _Row(this.line);
  final ({String time, String tool, String text}) line;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (line.tool.isNotEmpty || line.time.isNotEmpty)
            Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: line.tool,
                    style: TextStyle(
                      fontWeight: FontWeight.w700,
                      color: t.accentInk,
                    ),
                  ),
                  if (line.tool.isNotEmpty && line.time.isNotEmpty)
                    const TextSpan(text: '  '),
                  TextSpan(
                    text: line.time,
                    style: TextStyle(color: t.ink3),
                  ),
                ],
              ),
              style: const TextStyle(fontSize: 12),
            ),
          if (line.text.isNotEmpty)
            Padding(
              padding: EdgeInsets.only(
                top: line.tool.isEmpty && line.time.isEmpty ? 0 : 2,
              ),
              child: Text.rich(
                TextSpan(
                  children: [
                    for (final p in showEscapes(line.text))
                      TextSpan(
                        text: p.text,
                        style: p.escaped
                            ? TextStyle(
                                fontWeight: FontWeight.w700,
                                backgroundColor: t.err.withValues(alpha: .28),
                              )
                            : null,
                      ),
                  ],
                ),
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 13, height: 1.4, color: t.ink),
              ),
            ),
        ],
      ),
    );
  }
}

/// The activity block of one session: its title, the list and the note on where the lines come from. Nothing when the
/// computer did not grant this phone the right to read them ([lines] null).
class SessionActivity extends ConsumerWidget {
  const SessionActivity({super.key, required this.lines, required this.at});
  final List<String>? lines;
  final String at;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final lines = this.lines;
    if (lines == null) return const SizedBox.shrink();
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.watch(actionsSourceProvider).computerName;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
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
        ActivityLines(lines, empty: s('phone.sv.lines.empty')),
        const SizedBox(height: 8),
        FinePrint(
          s('phone.sv.act.n', {'c': c, 'tm': at}),
          align: TextAlign.left,
        ),
      ],
    );
  }
}
