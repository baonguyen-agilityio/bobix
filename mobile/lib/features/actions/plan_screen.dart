import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';

import 'package:markdown/markdown.dart' as md;

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import '../../widgets/status_banner.dart';
import 'action_screen_parts.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'command_flow.dart';
import 'plan_file_screen.dart';
import '../../widgets/press_feedback.dart';

final planProvider = FutureProvider.autoDispose.family<PlanDoc, String>(
  (ref, id) => ref.watch(actionsSourceProvider).plan(id),
);

/// A link written as text: a plan can hold addresses written by text Claude read, so nothing here opens.
class _LinkAsText extends MarkdownElementBuilder {
  @override
  Widget? visitElementAfterWithContext(
    BuildContext context,
    md.Element element,
    TextStyle? preferredStyle,
    TextStyle? parentStyle,
  ) {
    final href = element.attributes['href'];
    final text = element.textContent;
    return Text(
      href == null || href == text ? text : '$text ($href)',
      style: parentStyle,
    );
  }
}

/// The plan as markdown: no remote image is ever requested (the alt text is shown) and links are plain text.
class PlanMarkdown extends StatelessWidget {
  const PlanMarkdown(this.data, {super.key});
  final String data;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final base = TextStyle(fontSize: 14, height: 1.5, color: t.ink);
    return MarkdownBody(
      data: data,
      // Autolinked addresses become `a` elements too, and the builder below draws every one of them as text.
      extensionSet: md.ExtensionSet.gitHubFlavored,
      builders: {'a': _LinkAsText()},
      imageBuilder: (uri, title, alt) => Text(
        alt == null || alt.isEmpty ? '[image]' : '[image: $alt]',
        style: base.copyWith(color: t.ink3, fontStyle: FontStyle.italic),
      ),
      styleSheet: MarkdownStyleSheet(
        p: base,
        h1: base.copyWith(fontSize: 22, fontWeight: FontWeight.w700),
        h2: base.copyWith(fontSize: 19, fontWeight: FontWeight.w700),
        h3: base.copyWith(fontSize: 16, fontWeight: FontWeight.w600),
        listBullet: base,
        strong: base.copyWith(fontWeight: FontWeight.w700),
        em: base.copyWith(fontStyle: FontStyle.italic),
        code: base.copyWith(
          fontFamily: 'Menlo',
          fontFamilyFallback: const ['Roboto Mono', 'monospace'],
          fontSize: 12.5,
          backgroundColor: t.surface3,
        ),
        codeblockPadding: const EdgeInsets.all(12),
        codeblockDecoration: BoxDecoration(
          color: t.surface2,
          borderRadius: BorderRadius.circular(10),
          border: Border.all(width: .5, color: t.line),
        ),
        tableHead: base.copyWith(fontWeight: FontWeight.w700),
        tableBody: base,
        tableBorder: TableBorder.all(color: t.lineStrong, width: .5),
        tableCellsPadding: const EdgeInsets.all(6),
        blockquoteDecoration: BoxDecoration(
          color: t.surface2,
          borderRadius: BorderRadius.circular(6),
        ),
        a: base,
      ),
    );
  }
}

/// A plan waiting for approval: Approve, or Send back with a note.
class PlanScreen extends ConsumerWidget {
  const PlanScreen({super.key, required this.id});
  final String id;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final canSend = ref.watch(canSendProvider);
    final source = ref.watch(actionsSourceProvider);
    final plan = ref.watch(planProvider(id));
    return plan.when(
      loading: () => ActionPage(
        title: s('phone.pl.title'),
        children: const [
          SizedBox(height: 80),
          Center(child: CircularProgressIndicator()),
        ],
      ),
      error: (e, _) =>
          ActionPage(title: s('phone.pl.title'), children: [Text('$e')]),
      data: (p) {
        final c = source.computerName;
        final conn = ref.watch(connProvider);
        return ActionPage(
          title: s('phone.pl.title'),
          onLog: taskLogOpener(context, ref, p.id),
          actions: ActionButtons(
            secondary: ActionButton(
              s('phone.pl.back'),
              icon: Icons.edit_note_outlined,
              onPressed: !canSend
                  ? null
                  : () =>
                        Navigator.of(context)
                            .push(pageRoute<void>(PlanNoteScreen(plan: p))),
            ),
            primary: ActionButton(
              s('phone.pl.approve'),
              kind: BtnKind.primary,
              icon: Icons.lock_outline,
              onPressed: !canSend || p.cut || p.filesPartial
                  ? null
                  : () => runCommand(
                      context,
                      ref,
                      kind: CommandKind.approve,
                      target: p.id,
                      shown: p.shownHash,
                      reason: s('phone.un.approve', {'c': c}),
                    ),
            ),
            lock: !p.cut,
            note: p.files.isEmpty && p.omitted == 0
                ? null
                : p.omitted > 0
                ? s('phone.pf.more.s', {'n': p.omitted})
                : p.filesPartial
                ? s('phone.pf.bad.s')
                : s('phone.pf.ok', {'n': p.files.length}),
            fine: p.cut
                ? s('phone.pl.cut.s')
                : '${s('phone.rq.unlock', {'os': bioName(s)})} · ${sendNote(s, conn, c)}',
          ),
          children: [
            const SendOffBanner(),
            if (p.files.any((f) => f.partial))
              StatusBanner(
                kind: BannerKind.err,
                icon: Icons.error_outline,
                title: s('phone.pf.bad.h'),
                body: s('phone.pf.bad.d', {'c': c}),
              ),
            if (p.omitted > 0)
              StatusBanner(
                kind: BannerKind.err,
                icon: Icons.error_outline,
                title: s('phone.pf.more', {'n': p.omitted}),
                body: s('phone.pf.more.d', {'c': c}),
              ),
            if (p.cut)
              StatusBanner(
                kind: BannerKind.warn,
                icon: Icons.content_cut,
                title: s('phone.pl.cut'),
                body: s('phone.pl.cut.d'),
              ),
            ActionSessionCard(id: p.id, name: p.sessionName),
            ScreenHeading(s('phone.pl.h')),
            PlanMarkdown(p.markdown),
            if (p.files.isNotEmpty) PlanFilesGroup(p.files),
            const SizedBox(height: 10),
            ShieldNote(s('phone.pl.text')),
            const SizedBox(height: 10),
            ShieldNote(s('phone.pl.auto')),
          ],
        );
      },
    );
  }
}

/// Send the plan back with a note. The note is optional, as at the computer.
class PlanNoteScreen extends ConsumerStatefulWidget {
  const PlanNoteScreen({super.key, required this.plan});
  final PlanDoc plan;

  @override
  ConsumerState<PlanNoteScreen> createState() => _PlanNoteScreenState();
}

class _PlanNoteScreenState extends ConsumerState<PlanNoteScreen> {
  final _note = TextEditingController();

  @override
  void dispose() {
    _note.dispose();
    super.dispose();
  }

  void _add(String text) {
    final cur = _note.text;
    _note.text = cur.isEmpty ? text : '$cur, $text';
    _note.selection = TextSelection.collapsed(offset: _note.text.length);
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final canSend = ref.watch(canSendProvider);
    final c = ref.watch(actionsSourceProvider).computerName;
    final p = widget.plan;
    return ActionPage(
      title: s('phone.pn.title'),
      actions: SizedBox(
        width: double.infinity,
        child: ActionButton(
          s('phone.pn.send'),
          kind: BtnKind.primary,
          icon: Icons.lock_outline,
          onPressed: !canSend
              ? null
              : () => runCommand(
                  context,
                  ref,
                  kind: CommandKind.back,
                  target: p.id,
                  shown: p.shownHash,
                  text: _note.text,
                  reason: s('phone.un.back', {'c': c}),
                ),
        ),
      ),
      children: [
        const SendOffBanner(),
        Text(
          s('phone.pn.d', {'n': p.sessionName}),
          style: TextStyle(fontSize: 14, height: 1.5, color: t.ink2),
        ),
        const SizedBox(height: 14),
        Text(
          s('phone.pn.l'),
          style: TextStyle(
            fontSize: 13,
            fontWeight: FontWeight.w600,
            color: t.ink2,
          ),
        ),
        const SizedBox(height: 6),
        Glass(
          radius: 16,
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          child: TextField(
            controller: _note,
            minLines: 4,
            maxLines: 8,
            textInputAction: TextInputAction.newline,
            cursorColor: t.accentInk,
            decoration: InputDecoration.collapsed(
              hintText: s('phone.pn.ph'),
              hintStyle: TextStyle(fontSize: 16, color: t.ink3),
            ),
            style: TextStyle(fontSize: 16, color: t.ink),
          ),
        ),
        const SizedBox(height: 10),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            for (final k in const ['phone.pn.c1', 'phone.pn.c2', 'phone.pn.c3'])
              _SuggestionChip(label: s(k), onTap: () => _add(s(k))),
          ],
        ),
        const SizedBox(height: 14),
        FinePrint(s('phone.pn.n'), align: TextAlign.left),
      ],
    );
  }
}

/// A capsule of glass that adds a ready-made phrase to the note.
class _SuggestionChip extends StatelessWidget {
  const _SuggestionChip({required this.label, required this.onTap});
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return PressFeedback(
      scale: .985,
      opacity: .75,
      child: InkWell(
        borderRadius: BorderRadius.circular(22),
        onTap: onTap,
        child: Glass(
          radius: 22,
          padding: const EdgeInsets.symmetric(horizontal: 14),
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 44),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: Text(
                    label,
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: t.ink2,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
