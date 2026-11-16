import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/theme.dart' show minTouch;
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import 'action_widgets.dart' show FinePrint;
import 'hidden_chars.dart' show showEscapes;
import 'actions_source.dart';

/// Tools the computer names at the start of a line. A line is `{tool} {text}` for a tool and the bare text for anything
/// else (a session starting, a notice), so the first word is only a tool when it is one of these or an MCP tool.
const _knownTools = {
  'Bash',
  'BashOutput',
  'KillBash',
  'Read',
  'Write',
  'Edit',
  'MultiEdit',
  'NotebookEdit',
  'Glob',
  'Grep',
  'LS',
  'Task',
  'Agent',
  'Skill',
  'TodoWrite',
  'WebFetch',
  'WebSearch',
  'SendMessage',
  'AskUserQuestion',
  'ExitPlanMode',
  'EnterPlanMode',
};

/// One line of a session's activity split into the tool it names (empty when it names none) and what it did.
({String tool, String text}) splitActivityLine(String line) {
  final rest = line.trim();
  final m = RegExp(r'^(\S+)(?:\s+(.*))?$', dotAll: true).firstMatch(rest);
  if (m == null) return (tool: '', text: rest);
  final first = m.group(1)!;
  if (!_knownTools.contains(first) && !first.startsWith('mcp__')) {
    return (tool: '', text: rest);
  }
  // `mcp__server__send_message` reads as `send_message`.
  final tool = first.startsWith('mcp__') ? first.split('__').last : first;
  return (tool: tool, text: m.group(2) ?? '');
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
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Text(empty, style: TextStyle(fontSize: 14, color: t.ink3)),
      );
    }
    return Glass(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (var i = 0; i < lines.length; i++) ...[
            if (i > 0) Divider(height: .5, thickness: .5, color: t.line),
            _Row(key: ValueKey('$i:${lines[i]}'), splitActivityLine(lines[i])),
          ],
        ],
      ),
    );
  }
}

/// One line. A long text is cut at two lines; a tap opens the whole of it and a second tap folds it again.
class _Row extends StatefulWidget {
  const _Row(this.line, {super.key});
  final ({String tool, String text}) line;

  @override
  State<_Row> createState() => _RowState();
}

class _RowState extends State<_Row> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final line = widget.line;
    return InkWell(
      onTap: line.text.isEmpty ? null : () => setState(() => _open = !_open),
      child: AnimatedSize(
        duration: const Duration(milliseconds: 180),
        curve: Curves.easeOutCubic,
        alignment: Alignment.topCenter,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (line.tool.isNotEmpty)
                Text(
                  line.tool,
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w700,
                    color: t.accentInk,
                  ),
                ),
              if (line.text.isNotEmpty)
                Padding(
                  padding: EdgeInsets.only(top: line.tool.isEmpty ? 0 : 2),
                  child: Text.rich(
                    TextSpan(
                      children: [
                        for (final p in showEscapes(line.text))
                          TextSpan(
                            text: p.text,
                            style: p.escaped
                                ? TextStyle(
                                    fontWeight: FontWeight.w700,
                                    backgroundColor: t.err.withValues(
                                      alpha: .28,
                                    ),
                                  )
                                : null,
                          ),
                      ],
                    ),
                    maxLines: _open ? null : 2,
                    overflow: _open
                        ? TextOverflow.visible
                        : TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 13, height: 1.4, color: t.ink),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The activity block of one session: its title, the list and the note on where the lines come from. Nothing when the
/// computer did not grant this phone the right to read them ([lines] null). [limit] keeps only the newest lines and
/// [onSeeAll] adds a link to the rest; [empty] is the one muted line shown when there is nothing yet.
class SessionActivity extends ConsumerWidget {
  const SessionActivity({
    super.key,
    required this.lines,
    required this.at,
    required this.empty,
    this.limit,
    this.onSeeAll,
  });
  final List<String>? lines;
  final String at;
  final String empty;
  final int? limit;
  final VoidCallback? onSeeAll;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final all = lines;
    if (all == null) return const SizedBox.shrink();
    final shown = limit == null ? all : all.take(limit!).toList();
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.watch(actionsSourceProvider).computerName;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 18, 0, 8),
          child: Row(
            children: [
              Expanded(
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
              if (onSeeAll != null && all.isNotEmpty)
                InkWell(
                  onTap: onSeeAll,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(minHeight: minTouch),
                    child: Center(
                      widthFactor: 1,
                      child: Text(
                        s('phone.sv.all'),
                        style: TextStyle(
                          fontSize: 14,
                          fontWeight: FontWeight.w600,
                          color: t.accentInk,
                        ),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
        ActivityLines(shown, empty: empty),
        if (all.isNotEmpty) ...[
          const SizedBox(height: 8),
          FinePrint(
            s('phone.sv.act.n', {'c': c, 'tm': at}),
            align: TextAlign.left,
          ),
        ],
      ],
    );
  }
}
