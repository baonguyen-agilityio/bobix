import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// A command or a few log lines in a monospace box. [marks] are parts to highlight; [clamped] cuts the box short with a
/// fade; [danger] tints it.
class CommandBlock extends StatelessWidget {
  const CommandBlock(
    this.text, {
    super.key,
    this.marks = const [],
    this.clamped = false,
    this.danger = false,
    this.fontSize = 13,
  });

  final String text;
  final List<String> marks;
  final bool clamped, danger;
  final double fontSize;

  List<InlineSpan> _spans(TextStyle base, TextStyle mark) {
    if (marks.isEmpty) return [TextSpan(text: text)];
    final re = RegExp(marks.map(RegExp.escape).join('|'));
    final out = <InlineSpan>[];
    var at = 0;
    for (final m in re.allMatches(text)) {
      if (m.start > at) out.add(TextSpan(text: text.substring(at, m.start)));
      out.add(TextSpan(text: m[0], style: mark));
      at = m.end;
    }
    if (at < text.length) out.add(TextSpan(text: text.substring(at)));
    return out;
  }

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final base = TextStyle(
      fontFamily: 'Menlo',
      fontFamilyFallback: const ['monospace'],
      fontSize: fontSize,
      height: 1.5,
      color: t.ink,
    );
    final mark = base.copyWith(
      fontWeight: FontWeight.w700,
      backgroundColor: t.err.withValues(alpha: .28),
    );
    Widget body = Text.rich(
      TextSpan(style: base, children: _spans(base, mark)),
      maxLines: clamped ? 6 : null,
      overflow: TextOverflow.clip,
    );
    if (clamped) {
      body = Stack(
        children: [
          body,
          Positioned(
            left: 0,
            right: 0,
            bottom: 0,
            height: fontSize * 3,
            child: DecoratedBox(
              decoration: BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [t.surface2.withValues(alpha: 0), t.surface2],
                ),
              ),
            ),
          ),
        ],
      );
    }
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: danger
            ? Color.alphaBlend(t.err.withValues(alpha: .08), t.surface2)
            : t.surface2,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(
          width: danger ? 1 : .5,
          color: danger ? t.err : t.line,
        ),
      ),
      child: body,
    );
  }
}

/// A solid warning band (destructive command, hidden characters).
class WarningBand extends StatelessWidget {
  const WarningBand({
    super.key,
    required this.icon,
    required this.title,
    required this.body,
  });

  final IconData icon;
  final String title, body;

  @override
  Widget build(BuildContext context) {
    const fg = Colors.white;
    return Container(
      margin: const EdgeInsets.only(bottom: 10),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: _bandFill,
        borderRadius: BorderRadius.circular(16),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 20, color: fg),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: const TextStyle(
                    fontSize: 14,
                    fontWeight: FontWeight.w700,
                    color: fg,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  body,
                  style: const TextStyle(fontSize: 13, height: 1.4, color: fg),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

// The prototype's --err-solid, oklch(50% .19 22): the same in light and dark.
const _bandFill = Color(0xFFC62F3B);
