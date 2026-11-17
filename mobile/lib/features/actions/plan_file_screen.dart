import 'dart:ui' show FontFeature;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../widgets/glass_backdrop.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import '../../widgets/press_feedback.dart';
import '../../widgets/status_banner.dart';
import 'action_screen_parts.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'plan_screen.dart';

/// "1,229 B" below a kilobyte, otherwise "2.7 KB" (a comma in Vietnamese).
String fileSize(int bytes, String lang) {
  if (bytes < 1024) return '$bytes B';
  final kb = (bytes / 1024).toStringAsFixed(1);
  return '${lang == 'vi' ? kb.replaceFirst('.', ',') : kb} KB';
}

/// A size in figures that keep their width, so the sizes of a list line up.
class _Size extends StatelessWidget {
  const _Size(this.bytes, {this.style});
  final int bytes;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) => Consumer(
    builder: (context, ref, _) => Text(
      fileSize(bytes, ref.watch(langProvider)),
      style: (style ?? const TextStyle()).copyWith(
        fontFeatures: const [FontFeature.tabularFigures()],
      ),
    ),
  );
}

/// A small rounded label: the size, "Shortened", "Not sent".
class _Badge extends StatelessWidget {
  const _Badge({this.text, this.size, this.warn = false});
  final String? text;
  final int? size;
  final bool warn;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final style = TextStyle(
      fontSize: 12,
      fontWeight: FontWeight.w600,
      height: 1.3,
      color: warn ? t.warn : t.ink2,
    );
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 1),
      constraints: const BoxConstraints(minHeight: 22),
      decoration: BoxDecoration(
        color: warn ? t.warnBg : t.surface3,
        borderRadius: BorderRadius.circular(11),
      ),
      child: size != null
          ? _Size(size!, style: style)
          : Text(text!, style: style),
    );
  }
}

/// "Files in this plan (N)": one row per file; a file that was sent opens read only, one that was not is only listed.
class PlanFilesGroup extends ConsumerWidget {
  const PlanFilesGroup(this.files, {super.key});
  final List<PlanFileDoc> files;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.watch(actionsSourceProvider).computerName;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 16, 0, 8),
          child: Semantics(
            header: true,
            child: Text(
              s('phone.pf.sec', {'n': files.length}),
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: t.ink3,
              ),
            ),
          ),
        ),
        Glass(
          child: Material(
            type: MaterialType.transparency,
            child: Column(
              children: [
                for (var i = 0; i < files.length; i++) ...[
                  if (i > 0) Divider(height: .5, thickness: .5, color: t.line),
                  _FileRow(
                    file: files[i],
                    onTap: files[i].sent
                        ? () => Navigator.of(context).push(
                            pageRoute<void>(PlanFileScreen(file: files[i])),
                          )
                        : null,
                    computer: c,
                  ),
                ],
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _FileRow extends ConsumerWidget {
  const _FileRow({required this.file, required this.computer, this.onTap});
  final PlanFileDoc file;
  final String computer;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return PressFeedback(
      scale: .985,
      opacity: onTap == null ? 1 : .75,
      child: InkWell(
        onTap: onTap,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 60),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(14, 8, 14, 8),
            child: Row(
              children: [
                Icon(Icons.description_outlined, size: 22, color: t.ink2),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        file.name,
                        style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w500,
                          height: 1.3,
                          color: t.ink,
                        ),
                      ),
                      Row(
                        children: [
                          Flexible(
                            child: Text(
                              '${file.folder} · ',
                              style: TextStyle(
                                fontSize: 12,
                                height: 1.35,
                                color: t.ink3,
                              ),
                            ),
                          ),
                          _Size(
                            file.bytes,
                            style: TextStyle(
                              fontSize: 12,
                              height: 1.35,
                              color: t.ink3,
                            ),
                          ),
                        ],
                      ),
                      if (!file.sent)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            s('phone.pf.big', {'c': computer}),
                            style: TextStyle(
                              fontSize: 12,
                              height: 1.35,
                              color: t.ink3,
                            ),
                          ),
                        ),
                      if (file.partial)
                        Padding(
                          padding: const EdgeInsets.only(top: 6),
                          child: _Badge(
                            text: s(
                              file.cut ? 'phone.pf.cut' : 'phone.pf.unsent',
                            ),
                            warn: true,
                          ),
                        ),
                    ],
                  ),
                ),
                if (onTap != null) ...[
                  const SizedBox(width: 8),
                  Icon(Icons.chevron_right, size: 16, color: t.ink3),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// One file of a plan, read only: the text the computer sent, as Markdown, clamped until asked for whole.
class PlanFileScreen extends ConsumerStatefulWidget {
  const PlanFileScreen({super.key, required this.file});
  final PlanFileDoc file;

  @override
  ConsumerState<PlanFileScreen> createState() => _PlanFileScreenState();
}

class _PlanFileScreenState extends ConsumerState<PlanFileScreen> {
  static const _clampLines = 14;
  static const _lineHeight = 14 * 1.5;
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.watch(actionsSourceProvider).computerName;
    final f = widget.file;
    final lines = '\n'.allMatches(f.text).length + 1;
    final long = lines > _clampLines;
    final body = PlanMarkdown(f.text);
    return ActionPage(
      title: f.name,
      children: [
        Glass(
          padding: const EdgeInsets.all(14),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(Icons.description_outlined, size: 22, color: t.ink2),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          f.name,
                          style: TextStyle(
                            fontSize: 15,
                            fontWeight: FontWeight.w600,
                            color: t.ink,
                          ),
                        ),
                        Text(
                          '${s('phone.pf.dir')}: ${f.folder}',
                          style: TextStyle(fontSize: 12, color: t.ink3),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 10),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  _Badge(size: f.bytes),
                  if (f.cut) _Badge(text: s('phone.pf.cut'), warn: true),
                ],
              ),
            ],
          ),
        ),
        if (f.cut) ...[
          const SizedBox(height: 12),
          StatusBanner(
            kind: BannerKind.warn,
            icon: Icons.content_cut,
            title: s('phone.pf.cut'),
            body: s('phone.pf.cutn', {'c': c}),
          ),
        ],
        const SizedBox(height: 12),
        if (long && !_open)
          ClipRect(
            child: SizedBox(
              height: _clampLines * _lineHeight,
              child: OverflowBox(
                alignment: Alignment.topLeft,
                maxHeight: double.infinity,
                child: body,
              ),
            ),
          )
        else
          body,
        if (long)
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton(
              onPressed: () => setState(() => _open = !_open),
              style: TextButton.styleFrom(minimumSize: const Size(44, 44)),
              child: Text(
                _open ? s('phone.pl.less') : s('phone.pf.all', {'n': lines}),
              ),
            ),
          ),
        const SizedBox(height: 14),
        ShieldNote(s('phone.pf.ro', {'c': c})),
      ],
    );
  }
}
