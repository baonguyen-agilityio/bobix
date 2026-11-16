import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';
import '../../data/models.dart';
import '../../data/phone_data.dart';
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

/// The computer's name: from the actions source once one is paired, else from what Home was told.
final _computerNameProvider = Provider<String>((ref) {
  try {
    return ref.watch(actionsSourceProvider).computerName;
  } catch (_) {
    return ref.watch(homeProvider).value?.computer ?? '';
  }
});

/// An issue another screen (Home's "To do") asks the Start tab to have picked; Start takes it and clears it.
class StartRequest extends Notifier<String?> {
  @override
  String? build() => null;

  void open(String issueId) => state = issueId;

  void clear() => state = null;
}

final startRequestProvider = NotifierProvider<StartRequest, String?>(
  StartRequest.new,
);

/// The Start tab: pick an issue, choose Full or Quick, read what would happen, then ask the computer what would run.
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
    final conn = ref.watch(connProvider);
    final computer = ref.watch(_computerNameProvider);
    final issues = ref.watch(startIssuesProvider);
    ref.listen(startRequestProvider, (_, id) {
      if (id == null) return;
      setState(() => _issue = id);
      ref.read(startRequestProvider.notifier).clear();
    });
    final list = issues.value ?? const <StartIssue>[];
    final picked = list.isEmpty
        ? null
        : list.firstWhere((i) => i.id == _issue, orElse: () => list.first);
    final full = _mode == StartMode.full;
    final steps = full
        ? [
            s('phone.sp.f1', {'c': computer}),
            s('phone.sp.f2'),
            s('phone.sp.f3'),
            s('phone.sp.f4'),
          ]
        : [
            s('phone.sp.q1', {'d': picked?.project ?? ''}),
            s('phone.sp.q2'),
            s('phone.sp.q3'),
          ];
    final sendNote = switch (conn) {
      ConnState.ok => s('phone.sn.ok', {'c': computer}),
      ConnState.noNetwork => s('phone.sn.nonet'),
      ConnState.relayDown => s('phone.sn.relay'),
      _ => s('phone.sn.held'),
    };
    return Column(
      children: [
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 20),
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(0, 6, 0, 8),
                child: Semantics(
                  header: true,
                  child: Text(
                    s('phone.tab.start'),
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
                loading: () => const [
                  Center(child: CircularProgressIndicator()),
                ],
                error: (e, _) => [Text('$e')],
                data: (list) => list.isEmpty
                    ? [FinePrint(s('phone.sp.noissues'))]
                    : [
                        for (final i in list)
                          Padding(
                            padding: const EdgeInsets.only(bottom: 8),
                            child: _OptionCard(
                              title: i.title,
                              subtitle: '${i.project} #${i.number}',
                              selected: picked?.id == i.id,
                              onTap: () => setState(() => _issue = i.id),
                            ),
                          ),
                      ],
              ),
              // Nothing to start: only the empty note above, no way to start and no steps to read.
              if (picked != null) ...[
                _Heading(s('phone.sp.how')),
                _OptionCard(
                  title: s('phone.sp.full'),
                  subtitle: s('phone.sp.full.d'),
                  selected: full,
                  onTap: () => setState(() => _mode = StartMode.full),
                ),
                const SizedBox(height: 8),
                _OptionCard(
                  title: s('phone.sp.quick'),
                  subtitle: s('phone.sp.quick.d'),
                  selected: !full,
                  onTap: () => setState(() => _mode = StartMode.quick),
                ),
                _Heading(s('phone.sp.what', {'c': computer})),
                Glass(
                  padding: const EdgeInsets.all(14),
                  child: Column(
                    children: [
                      for (final (n, line) in steps.indexed)
                        Padding(
                          padding: EdgeInsets.only(top: n == 0 ? 0 : 10),
                          child: Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Container(
                                width: 26,
                                height: 26,
                                alignment: Alignment.center,
                                decoration: BoxDecoration(
                                  color: t.surface3,
                                  shape: BoxShape.circle,
                                ),
                                child: Text(
                                  '${n + 1}',
                                  style: TextStyle(
                                    fontSize: 12,
                                    fontWeight: FontWeight.w700,
                                    color: t.ink2,
                                  ),
                                ),
                              ),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Padding(
                                  padding: const EdgeInsets.only(top: 2),
                                  child: Text(
                                    line,
                                    style: TextStyle(
                                      fontSize: 14,
                                      height: 1.45,
                                      color: t.ink,
                                    ),
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ),
                    ],
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(0, 12, 0, 0),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Icon(Icons.shield_outlined, size: 16, color: t.ink3),
                      const SizedBox(width: 6),
                      Expanded(
                        child: FinePrint(
                          s(full ? 'phone.sp.gate' : 'phone.sp.gate.q'),
                          align: TextAlign.left,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
              // Clear of the floating tab bar when there is no action bar above it.
              if (picked == null) const SizedBox(height: 96),
            ],
          ),
        ),
        if (picked != null)
          GlassActionBar(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                SizedBox(
                  width: double.infinity,
                  child: ActionButton(
                    s(full ? 'phone.sp.go.full' : 'phone.sp.go.quick'),
                    kind: BtnKind.primary,
                    icon: Icons.lock_outline,
                    onPressed: !canSend
                        ? null
                        : () => Navigator.of(context).push(
                            pageRoute<void>(
                              StartPreviewScreen(
                                issueId: picked.id,
                                mode: _mode,
                              ),
                            ),
                          ),
                  ),
                ),
                const SizedBox(height: 8),
                FinePrint(
                  '${s('phone.rq.unlock', {'os': bioName(s)})} · $sendNote',
                ),
                // The floating tab bar sits over the bottom of the bar.
                SizedBox(height: 64 + MediaQuery.viewPaddingOf(context).bottom),
              ],
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

/// One choice of a group of radio cards: the picked one has the accent ring and a tint, and a filled radio.
class _OptionCard extends StatelessWidget {
  const _OptionCard({
    required this.title,
    required this.subtitle,
    required this.selected,
    required this.onTap,
  });
  final String title, subtitle;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      selected: selected,
      checked: selected,
      inMutuallyExclusiveGroup: true,
      button: true,
      label: '$title, $subtitle',
      excludeSemantics: true,
      child: Glass(
        tint: selected
            ? Color.alphaBlend(t.accent.withValues(alpha: .09), t.surface)
            : null,
        border: selected ? t.accent.withValues(alpha: .55) : null,
        child: InkWell(
          borderRadius: BorderRadius.circular(16),
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 64),
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(
                    width: 22,
                    height: 22,
                    margin: const EdgeInsets.only(top: 1),
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border: Border.all(
                        width: 2,
                        color: selected ? t.accentSolid : t.lineStrong,
                      ),
                    ),
                    child: selected
                        ? Container(
                            width: 9,
                            height: 9,
                            decoration: BoxDecoration(
                              shape: BoxShape.circle,
                              color: t.accentSolid,
                            ),
                          )
                        : null,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          title,
                          style: TextStyle(
                            fontSize: 15,
                            fontWeight: FontWeight.w600,
                            color: t.ink,
                            height: 1.3,
                          ),
                        ),
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            subtitle,
                            style: TextStyle(
                              fontSize: 13,
                              height: 1.4,
                              color: t.ink2,
                            ),
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
