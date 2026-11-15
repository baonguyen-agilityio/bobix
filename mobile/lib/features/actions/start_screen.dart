import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import '../../widgets/status_banner.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'command_flow.dart';

/// A prompt longer than this (characters or lines) is collapsed; Start stays locked until it is opened.
const promptCollapseChars = 500, promptCollapseLines = 10;

final startIssuesProvider = FutureProvider.autoDispose<List<StartIssue>>(
  (ref) => ref.watch(actionsSourceProvider).issues(),
);

/// The Start tab: pick an issue, choose Full or Quick, then ask the computer what would run.
class StartScreen extends ConsumerStatefulWidget {
  const StartScreen({super.key});

  @override
  ConsumerState<StartScreen> createState() => _StartScreenState();
}

class _StartScreenState extends ConsumerState<StartScreen> {
  String? _issue;
  StartMode _mode = StartMode.full;

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final canSend = ref.watch(canSendProvider);
    final issues = ref.watch(startIssuesProvider);
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 96),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 6, 0, 8),
          child: Semantics(
            header: true,
            child: Text(
              s('phone.sp.title'),
              style: TextStyle(
                fontSize: 28,
                fontWeight: FontWeight.w700,
                letterSpacing: -.5,
                color: t.ink,
              ),
            ),
          ),
        ),
        const SendOffBanner(),
        _Heading(s('phone.sp.pick')),
        ...issues.when(
          loading: () => const [Center(child: CircularProgressIndicator())],
          error: (e, _) => [Text('$e')],
          data: (list) => list.isEmpty
              ? [FinePrint(s('phone.sp.noissues'))]
              : [
                  for (final i in list)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: _IssueRow(
                        issue: i,
                        selected: _issue == i.id,
                        onTap: () => setState(() => _issue = i.id),
                      ),
                    ),
                ],
        ),
        _Heading(s('phone.sp.how')),
        _ModeSwitch(mode: _mode, onChanged: (m) => setState(() => _mode = m)),
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 10, 4, 0),
          child: Text(
            s(_mode == StartMode.full ? 'phone.sp.full.d' : 'phone.sp.quick.d'),
            style: TextStyle(fontSize: 13, height: 1.45, color: t.ink2),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 8, 4, 0),
          child: Text(
            s(_mode == StartMode.full ? 'phone.sp.gate' : 'phone.sp.gate.q'),
            style: TextStyle(fontSize: 13, height: 1.45, color: t.ink3),
          ),
        ),
        const SizedBox(height: 16),
        ActionButton(
          s('phone.sp.preview'),
          kind: BtnKind.primary,
          onPressed: _issue == null || !canSend
              ? null
              : () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) =>
                        StartPreviewScreen(issueId: _issue!, mode: _mode),
                  ),
                ),
        ),
      ],
    );
  }
}

class _Heading extends StatelessWidget {
  const _Heading(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(0, 14, 0, 8),
    child: Text(
      text,
      style: TextStyle(
        fontSize: 13,
        fontWeight: FontWeight.w600,
        color: context.tokens.ink3,
      ),
    ),
  );
}

class _IssueRow extends StatelessWidget {
  const _IssueRow({
    required this.issue,
    required this.selected,
    required this.onTap,
  });
  final StartIssue issue;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      selected: selected,
      button: true,
      label: '${issue.title}, ${issue.project} #${issue.number}',
      child: ExcludeSemantics(
        child: InkWell(
          borderRadius: BorderRadius.circular(16),
          onTap: onTap,
          child: Glass(
            tint: selected ? t.accentBg : null,
            border: selected ? t.accent : null,
            padding: const EdgeInsets.all(12),
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: 44),
              child: Row(
                children: [
                  Icon(
                    selected
                        ? Icons.radio_button_checked
                        : Icons.radio_button_unchecked,
                    size: 22,
                    color: selected ? t.accentSolid : t.ink3,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          issue.title,
                          style: TextStyle(
                            fontSize: 15,
                            fontWeight: FontWeight.w600,
                            color: t.ink,
                          ),
                        ),
                        Text(
                          '${issue.project} #${issue.number}',
                          style: TextStyle(fontSize: 12, color: t.ink3),
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
    );
  }
}

class _ModeSwitch extends ConsumerWidget {
  const _ModeSwitch({required this.mode, required this.onChanged});
  final StartMode mode;
  final ValueChanged<StartMode> onChanged;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    Widget seg(StartMode m, String label) {
      final on = mode == m;
      return Expanded(
        child: Semantics(
          button: true,
          selected: on,
          label: label,
          child: ExcludeSemantics(
            child: InkWell(
              borderRadius: BorderRadius.circular(22),
              onTap: () => onChanged(m),
              child: Container(
                constraints: const BoxConstraints(minHeight: 44),
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: on ? t.surface : null,
                  borderRadius: BorderRadius.circular(22),
                  boxShadow: on
                      ? const [
                          BoxShadow(
                            color: Color(0x1A000000),
                            blurRadius: 3,
                            offset: Offset(0, 1),
                          ),
                        ]
                      : null,
                ),
                child: Text(
                  label,
                  style: TextStyle(
                    fontSize: 14,
                    fontWeight: FontWeight.w600,
                    color: on ? t.ink : t.ink2,
                  ),
                ),
              ),
            ),
          ),
        ),
      );
    }

    return Container(
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: t.surface3,
        borderRadius: BorderRadius.circular(25),
      ),
      child: Row(
        children: [
          seg(StartMode.full, s('phone.sp.full')),
          seg(StartMode.quick, s('phone.sp.quick')),
        ],
      ),
    );
  }
}

/// What would run, before anything is sent: the whole prompt, and the problems (folder missing, low disk). The
/// biometric step comes only after this.
class StartPreviewScreen extends ConsumerStatefulWidget {
  const StartPreviewScreen({
    super.key,
    required this.issueId,
    required this.mode,
  });
  final String issueId;
  final StartMode mode;

  @override
  ConsumerState<StartPreviewScreen> createState() => _StartPreviewScreenState();
}

class _StartPreviewScreenState extends ConsumerState<StartPreviewScreen> {
  late StartMode _mode = widget.mode;
  bool _open = false;
  late Future<StartPreview> _preview = _load();

  Future<StartPreview> _load() =>
      ref.read(actionsSourceProvider).startPreview(widget.issueId, _mode);

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final source = ref.watch(actionsSourceProvider);
    final canSend = ref.watch(canSendProvider);
    final c = source.computerName;
    return FutureBuilder<StartPreview>(
      future: _preview,
      builder: (context, snap) {
        if (!snap.hasData) {
          return ActionPage(
            title: s('phone.sp.title'),
            children: [
              const SizedBox(height: 80),
              const Center(child: CircularProgressIndicator()),
              const SizedBox(height: 16),
              Center(
                child: Text(
                  s('phone.sp.loading', {'c': c}),
                  style: TextStyle(color: t.ink2),
                ),
              ),
            ],
          );
        }
        final p = snap.data!;
        final full = p.mode == StartMode.full;
        final long =
            p.prompt.length > promptCollapseChars ||
            '\n'.allMatches(p.prompt).length >= promptCollapseLines;
        final collapsed = long && !_open;
        final blocked = p.folderMissing || (full && p.lowDisk);
        final locked = blocked || collapsed;
        final repo = p.issue.project;
        return ActionPage(
          title: s('phone.sp.title'),
          actions: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                width: double.infinity,
                child: ActionButton(
                  s(full ? 'phone.sp.go.full' : 'phone.sp.go.quick'),
                  kind: BtnKind.primary,
                  icon: Icons.lock_outline,
                  onPressed: locked || !canSend
                      ? null
                      : () => runCommand(
                          context,
                          ref,
                          kind: CommandKind.start,
                          mode: p.mode,
                          target: p.issue.id,
                          shown: p.promptHash,
                          reason: s('phone.un.start', {'c': c}),
                        ),
                ),
              ),
              const SizedBox(height: 8),
              FinePrint(
                p.folderMissing
                    ? s('phone.sp.blocked')
                    : collapsed
                    ? s('phone.sp.openfirst')
                    : s('phone.rq.unlock', {'os': bioName(s)}),
              ),
            ],
          ),
          children: [
            const SendOffBanner(),
            TitleCard(
              title: p.issue.title,
              subtitle: '${p.issue.project} #${p.issue.number}',
              icon: Icons.play_arrow_rounded,
              chips: [
                MetaChip(s(full ? 'phone.sp.full' : 'phone.sp.quick')),
                MetaChip(p.folder, icon: Icons.folder_outlined),
              ],
            ),
            const SizedBox(height: 12),
            if (p.folderMissing)
              StatusBanner(
                kind: BannerKind.err,
                icon: Icons.folder_off_outlined,
                title: s('phone.sw.miss', {'p': repo}),
                body: s('phone.sw.miss.d', {'c': c, 'r': repo}),
              ),
            if (full && p.lowDisk)
              StatusBanner(
                kind: BannerKind.warn,
                icon: Icons.storage,
                title: s('phone.sw.low', {'c': c}),
                body: s('phone.sw.low.d', {
                  'free': p.freeSpace,
                  'need': p.needSpace,
                }),
                action: OutlinedButton(
                  onPressed: () => setState(() {
                    _mode = StartMode.quick;
                    _open = false;
                    _preview = _load();
                  }),
                  style: OutlinedButton.styleFrom(
                    minimumSize: const Size(44, 44),
                  ),
                  child: Text(s('phone.sw.switch')),
                ),
              ),
            _Heading(s('phone.sp.what', {'c': c})),
            Glass(
              padding: const EdgeInsets.all(14),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (final line
                      in full
                          ? [
                              s('phone.sp.f1', {
                                'b': p.branch ?? '',
                                'd': p.folder,
                              }),
                              s('phone.sp.f2'),
                              s('phone.sp.f3'),
                              s('phone.sp.f4'),
                            ]
                          : [
                              s('phone.sp.q1', {'d': p.folder}),
                              s('phone.sp.q2'),
                              s('phone.sp.q3'),
                            ])
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 3),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text('•  ', style: TextStyle(color: t.ink3)),
                          Expanded(
                            child: Text(
                              line,
                              style: TextStyle(
                                fontSize: 14,
                                height: 1.45,
                                color: t.ink,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
            ),
            _Heading(s('phone.sp.prompt')),
            CodeBlock(
              p.prompt,
              maxHeight: collapsed ? 170 : null,
              fontSize: 12.5,
            ),
            if (long)
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton(
                  onPressed: () => setState(() => _open = !_open),
                  style: TextButton.styleFrom(minimumSize: const Size(44, 44)),
                  child: Text(
                    _open ? s('phone.rq.less') : s('phone.sp.prompt.all'),
                  ),
                ),
              ),
            const SizedBox(height: 4),
            FinePrint(s('phone.sp.prompt.n', {'c': c}), align: TextAlign.left),
          ],
        );
      },
    );
  }
}
