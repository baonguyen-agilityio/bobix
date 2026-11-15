import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'command_flow.dart';

/// The longest free-text answer, in UTF-8 bytes.
const maxAnswerBytes = 2000;

final questionProvider = FutureProvider.autoDispose.family<QuestionDoc, String>(
  (ref, id) => ref.watch(actionsSourceProvider).question(id),
);

/// Claude's question: pick an option (or several), or type your own answer, then Send.
class QuestionScreen extends ConsumerStatefulWidget {
  const QuestionScreen({super.key, required this.id});
  final String id;

  @override
  ConsumerState<QuestionScreen> createState() => _QuestionScreenState();
}

/// What the person chose or typed for one part of the question.
class _PartState {
  final picked = <int>{};
  bool other = false;
  final text = TextEditingController();
  int get bytes => utf8.encode(text.text).length;
}

class _QuestionScreenState extends ConsumerState<QuestionScreen> {
  List<_PartState> _states = [];

  @override
  void dispose() {
    for (final st in _states) {
      st.text.dispose();
    }
    super.dispose();
  }

  void _toggle(QuestionPart q, _PartState st, int i) => setState(() {
    if (q.multiSelect) {
      st.picked.contains(i) ? st.picked.remove(i) : st.picked.add(i);
    } else {
      st.picked
        ..clear()
        ..add(i);
      st.other = false;
    }
  });

  void _toggleOther(QuestionPart q, _PartState st) => setState(() {
    if (q.multiSelect) {
      st.other = !st.other;
    } else {
      st.other = true;
      st.picked.clear();
    }
  });

  /// The answer to one part exactly as chosen or typed; null while nothing valid is chosen.
  String? _answer(QuestionPart q, _PartState st) {
    final parts = [
      for (final i in (st.picked.toList()..sort())) q.options[i].label,
      if (st.other && st.text.text.trim().isNotEmpty) st.text.text,
    ];
    if (st.other && st.text.text.trim().isEmpty) return null;
    if (st.other && st.bytes > maxAnswerBytes) return null;
    return parts.isEmpty ? null : parts.join(', ');
  }

  /// One answer per part, in order; null until every part has a valid one.
  List<String>? _answers(QuestionDoc doc) {
    final out = <String>[];
    for (var i = 0; i < doc.parts.length; i++) {
      final a = _answer(doc.parts[i], _states[i]);
      if (a == null) return null;
      out.add(a);
    }
    return out;
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final canSend = ref.watch(canSendProvider);
    final c = ref.watch(actionsSourceProvider).computerName;
    final doc = ref.watch(questionProvider(widget.id));
    return doc.when(
      loading: () => ActionPage(
        title: s('phone.qs.title'),
        children: const [
          SizedBox(height: 80),
          Center(child: CircularProgressIndicator()),
        ],
      ),
      error: (e, _) =>
          ActionPage(title: s('phone.qs.title'), children: [Text('$e')]),
      data: (q) {
        if (_states.length != q.parts.length) {
          _states = [for (final _ in q.parts) _PartState()];
        }
        final answers = _answers(q);
        return ActionPage(
          title: s('phone.qs.title'),
          actions: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                width: double.infinity,
                child: ActionButton(
                  s('phone.qs.send'),
                  kind: BtnKind.primary,
                  icon: Icons.lock_outline,
                  onPressed: answers == null || !canSend
                      ? null
                      : () => runCommand(
                          context,
                          ref,
                          kind: CommandKind.answer,
                          target: q.id,
                          shown: q.shownHash,
                          text: answers.length == 1 ? answers.single : null,
                          answers: answers,
                          reason: s('phone.un.answer', {'c': c}),
                        ),
                ),
              ),
              const SizedBox(height: 8),
              FinePrint(answers == null ? s('phone.qs.pick') : s('phone.qs.n')),
            ],
          ),
          children: [
            const SendOffBanner(),
            TitleCard(
              title: s('phone.ask.q.h'),
              subtitle: q.sessionName,
              icon: Icons.help_outline,
            ),
            for (var i = 0; i < q.parts.length; i++)
              ..._part(
                context,
                s,
                t,
                q.parts[i],
                _states[i],
                i,
                q.parts.length,
              ),
          ],
        );
      },
    );
  }

  List<Widget> _part(
    BuildContext context,
    Strings s,
    BobixTokens t,
    QuestionPart q,
    _PartState st,
    int index,
    int count,
  ) {
    final tooLong = st.other && st.bytes > maxAnswerBytes;
    return [
      const SizedBox(height: 14),
      if (count > 1)
        Padding(
          padding: const EdgeInsets.only(bottom: 4),
          child: Text(
            s('phone.qs.part', {'n': index + 1, 'm': count}),
            style: TextStyle(
              fontSize: 13,
              fontWeight: FontWeight.w600,
              color: t.ink3,
            ),
          ),
        ),
      Semantics(
        header: true,
        child: Text(
          q.question,
          style: TextStyle(
            fontSize: 20,
            fontWeight: FontWeight.w700,
            height: 1.25,
            color: t.ink,
          ),
        ),
      ),
      if (q.multiSelect)
        Padding(
          padding: const EdgeInsets.only(top: 6),
          child: Text(
            s('phone.qs.multi'),
            style: TextStyle(fontSize: 13, color: t.ink3),
          ),
        ),
      const SizedBox(height: 12),
      for (var i = 0; i < q.options.length; i++)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: _Option(
            label: q.options[i].label,
            detail: q.options[i].detail,
            checked: st.picked.contains(i),
            multi: q.multiSelect,
            onTap: () => _toggle(q, st, i),
          ),
        ),
      _Option(
        label: s('phone.qs.other'),
        detail: s('phone.qs.other.d'),
        checked: st.other,
        multi: q.multiSelect,
        onTap: () => _toggleOther(q, st),
      ),
      if (st.other) ...[
        const SizedBox(height: 10),
        TextField(
          controller: st.text,
          minLines: 3,
          maxLines: 8,
          onChanged: (_) => setState(() {}),
          decoration: InputDecoration(
            hintText: s('phone.qs.other.ph'),
            filled: true,
            fillColor: t.surface,
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(12),
              borderSide: BorderSide(color: t.lineStrong),
            ),
          ),
          style: TextStyle(fontSize: 16, color: t.ink),
        ),
        const SizedBox(height: 4),
        Align(
          alignment: Alignment.centerRight,
          child: Text(
            tooLong
                ? s('phone.qs.toolong', {'max': maxAnswerBytes})
                : s('phone.qs.bytes', {'n': st.bytes, 'max': maxAnswerBytes}),
            style: TextStyle(fontSize: 12, color: tooLong ? t.err : t.ink3),
          ),
        ),
      ],
    ];
  }
}

class _Option extends StatelessWidget {
  const _Option({
    required this.label,
    required this.checked,
    required this.multi,
    required this.onTap,
    this.detail,
  });
  final String label;
  final String? detail;
  final bool checked, multi;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      checked: checked,
      inMutuallyExclusiveGroup: !multi,
      button: true,
      label: label,
      child: ExcludeSemantics(
        child: Material(
          color: Colors.transparent,
          child: InkWell(
            borderRadius: BorderRadius.circular(16),
            onTap: onTap,
            child: Glass(
              tint: checked ? t.accentBg : null,
              border: checked ? t.accent : null,
              padding: const EdgeInsets.all(14),
              child: ConstrainedBox(
                constraints: const BoxConstraints(minHeight: 36),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Icon(
                      multi
                          ? (checked
                                ? Icons.check_box
                                : Icons.check_box_outline_blank)
                          : (checked
                                ? Icons.radio_button_checked
                                : Icons.radio_button_unchecked),
                      size: 22,
                      color: checked ? t.accentSolid : t.ink3,
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            label,
                            style: TextStyle(
                              fontSize: 15,
                              fontWeight: FontWeight.w600,
                              color: t.ink,
                            ),
                          ),
                          if (detail != null)
                            Text(
                              detail!,
                              style: TextStyle(
                                fontSize: 13,
                                height: 1.4,
                                color: t.ink2,
                              ),
                            ),
                        ],
                      ),
                    ),
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
