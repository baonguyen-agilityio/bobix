import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';
import '../../data/models.dart';
import '../../widgets/detail_scaffold.dart';
import '../../widgets/floating_sheet.dart';
import '../../settings/app_settings.dart';
import '../../data/phone_data.dart';
import '../../l10n/l10n.dart';
import '../../theme/theme.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import '../../widgets/status_banner.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'command_flow.dart';
import '../../widgets/press_feedback.dart';

/// A prompt longer than this (characters or lines) is collapsed; Start stays locked until it is opened.
const promptCollapseChars = 500, promptCollapseLines = 10;

final startIssuesProvider = FutureProvider.autoDispose<StartList>(
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

  /// The Repository sheet: All repositories first, then each one; the choice is remembered on the phone.
  Future<void> _chooseRepo(
    List<StartRepo> repos,
    StartRepo? current,
    String computer,
  ) async {
    final key = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      backgroundColor: Colors.transparent,
      builder: (_) =>
          _RepoSheet(repos: repos, current: current, computer: computer),
    );
    if (key == null || !mounted) return;
    await ref.read(settingsProvider.notifier).setStartRepo(key);
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final canSend = ref.watch(canSendProvider);
    final conn = ref.watch(connProvider);
    final computer = ref.watch(_computerNameProvider);
    final issues = ref.watch(startIssuesProvider);
    final remembered = ref.watch(settingsProvider).value?.startRepo ?? '';
    ref.listen(startRequestProvider, (_, id) {
      if (id == null) return;
      // An issue another screen picked is shown whatever repository the filter had.
      final repos = ref.read(startIssuesProvider).value?.repos ?? const [];
      if (repos.any(
            (r) => r.key == remembered && r.issues.any((i) => i.id == id),
          ) ==
          false) {
        unawaited(ref.read(settingsProvider.notifier).setStartRepo(''));
      }
      setState(() => _issue = id);
      ref.read(startRequestProvider.notifier).clear();
    });
    final all = issues.value ?? StartList.empty;
    final error = issues.hasError ? StartListError.read : all.error;
    final loaded = issues.hasValue || issues.hasError;
    // No list at all: not read yet, or nothing ticked on the computer. The Repository row and the issues are left out.
    final noList = error != null || (loaded && all.repos.isEmpty);
    final sel = all.repos.where((r) => r.key == remembered).firstOrNull;
    final shown = sel == null ? all.repos : [sel];
    final list = noList
        ? const <StartIssue>[]
        : [for (final r in shown) ...r.issues];
    final picked = list.isEmpty
        ? null
        : list.firstWhere((i) => i.id == _issue, orElse: () => list.first);
    final pickedRepo = picked == null
        ? null
        : all.repos.firstWhere((r) => r.issues.contains(picked));
    final blocked =
        picked != null && (!pickedRepo!.cloned || picked.hasSession);
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
              if (picked != null && !pickedRepo!.cloned)
                StatusBanner(
                  kind: BannerKind.err,
                  icon: Icons.folder_off_outlined,
                  title: s('phone.sw.miss', {'p': picked.project}),
                  body: s('phone.sw.miss.d', {
                    'c': computer,
                    'r': picked.project,
                  }),
                ),
              if (picked != null && picked.hasSession)
                StatusBanner(
                  kind: BannerKind.info,
                  icon: Icons.play_circle_outline,
                  title: s('phone.rp.session.h', {
                    'r': picked.project,
                    'n': picked.number,
                    'c': computer,
                  }),
                  body: s('phone.rp.session.d'),
                ),
              if (!loaded)
                const Padding(
                  padding: EdgeInsets.only(top: 24),
                  child: Center(child: CircularProgressIndicator()),
                )
              else if (error != null)
                switch (error) {
                  StartListError.read => StatusBanner(
                    kind: BannerKind.err,
                    icon: Icons.warning_amber_rounded,
                    title: s('phone.rp.err.h', {'c': computer}),
                    body: s('phone.rp.err.d', {'c': computer}),
                  ),
                  StartListError.reading => StatusBanner(
                    kind: BannerKind.info,
                    icon: Icons.sync,
                    title: s('phone.rp.reading.h', {'c': computer}),
                    body: s('phone.rp.reading.d'),
                  ),
                  StartListError.noRight => StatusBanner(
                    kind: BannerKind.warn,
                    icon: Icons.lock_outline,
                    title: s('phone.rp.right.h'),
                    body: s('phone.rp.right.d', {'c': computer}),
                  ),
                }
              else if (noList)
                _StateCard(
                  icon: Icons.folder_outlined,
                  title: s('phone.rp.none.h', {'c': computer}),
                  body: s('phone.rp.none.d'),
                )
              else ...[
                _Heading(s('phone.rp.sec')),
                _RepoRow(
                  title: sel == null
                      ? '${s('phone.rp.all')} · ${_nIssues(s, list.length)}'
                      : '${sel.path} · ${_nIssues(s, list.length)}',
                  host: sel?.host,
                  onTap: () => _chooseRepo(all.repos, sel, computer),
                ),
                if (list.isEmpty)
                  _StateCard(
                    icon: Icons.confirmation_number_outlined,
                    title: sel == null
                        ? s('phone.rp.open0')
                        : s('phone.rp.empty.h', {'r': sel.path}),
                    body: s('phone.rp.empty.d'),
                  )
                else ...[
                  _Heading(s('phone.sp.pick')),
                  for (final r in shown)
                    if (r.issues.isNotEmpty) ...[
                      if (sel == null)
                        SectionHeader(r.path, count: r.issues.length),
                      for (final i in r.issues)
                        Padding(
                          padding: const EdgeInsets.only(bottom: 8),
                          child: _OptionCard(
                            title: i.title,
                            subtitle: '#${i.number}',
                            warn: !r.cloned
                                ? s('phone.rp.nofolder')
                                : i.hasSession
                                ? s('phone.rp.session')
                                : null,
                            selected: picked?.id == i.id,
                            onTap: () => setState(() => _issue = i.id),
                          ),
                        ),
                    ],
                ],
              ],
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
                _Heading(s('phone.rp.fix')),
                _RepoRow(title: pickedRepo!.path, host: pickedRepo.host),
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Icon(
                        Icons.computer,
                        size: 16,
                        color: context.tokens.ink3,
                      ),
                      const SizedBox(width: 6),
                      Expanded(
                        child: FinePrint(
                          s('phone.rp.fix.n', {'c': computer}),
                          align: TextAlign.left,
                        ),
                      ),
                    ],
                  ),
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
                    onPressed: !canSend || blocked
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

String _nIssues(Strings s, int n) =>
    s(n == 1 ? 'phone.rp.n1' : 'phone.rp.n', {'n': n});

String _nOpen(Strings s, int n) => s(
  n == 0
      ? 'phone.rp.open0'
      : n == 1
      ? 'phone.rp.open1'
      : 'phone.rp.open',
  {'n': n},
);

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
    this.warn,
  });
  final String title, subtitle;

  /// Why this one cannot start; said in the card, after the subtitle.
  final String? warn;
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
      label: warn == null ? '$title, $subtitle' : '$title, $subtitle, $warn',
      excludeSemantics: true,
      child: Glass(
        tint: selected
            ? Color.alphaBlend(t.accent.withValues(alpha: .09), t.surface)
            : null,
        border: selected ? t.accent.withValues(alpha: .55) : null,
        child: PressFeedback(
          scale: .985,
          opacity: .75,
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
                            child: Text.rich(
                              TextSpan(
                                text: subtitle,
                                children: [
                                  if (warn != null)
                                    TextSpan(
                                      text: ' · $warn',
                                      style: TextStyle(color: t.err),
                                    ),
                                ],
                              ),
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
      ),
    );
  }
}

/// A folder-icon row of Start: the repository filter (opens the sheet when [onTap] is set) or the repository an issue is
/// fixed in (not tappable: that is chosen on the computer).
class _RepoRow extends StatelessWidget {
  const _RepoRow({required this.title, this.host, this.onTap});
  final String title;
  final String? host;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      button: onTap != null,
      label: host == null ? title : '$title, $host',
      excludeSemantics: true,
      child: Glass(
        child: PressFeedback(
          scale: .985,
          opacity: onTap == null ? 1 : .75,
          child: InkWell(
            borderRadius: BorderRadius.circular(16),
            onTap: onTap,
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: minTouch + 8),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(8, 8, 12, 8),
                child: Row(
                  children: [
                    Container(
                      width: 44,
                      height: 44,
                      decoration: BoxDecoration(
                        color: t.surface3,
                        borderRadius: BorderRadius.circular(44 * 16 / 62),
                      ),
                      child: Icon(
                        Icons.folder_outlined,
                        size: 22,
                        color: t.ink2,
                      ),
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
                            ),
                          ),
                          if (host != null)
                            Text(
                              host!,
                              style: TextStyle(fontSize: 13, color: t.ink2),
                            ),
                        ],
                      ),
                    ),
                    if (onTap != null)
                      Icon(Icons.expand_more, size: 22, color: t.ink3),
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

/// Nothing to list: why, in a centred card.
class _StateCard extends StatelessWidget {
  const _StateCard({
    required this.icon,
    required this.title,
    required this.body,
  });
  final IconData icon;
  final String title, body;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Padding(
      padding: const EdgeInsets.only(top: 12),
      child: Glass(
        padding: const EdgeInsets.all(20),
        child: Column(
          children: [
            Container(
              width: 44,
              height: 44,
              decoration: BoxDecoration(
                color: t.surface3,
                shape: BoxShape.circle,
              ),
              child: Icon(icon, size: 22, color: t.ink2),
            ),
            const SizedBox(height: 10),
            Text(
              title,
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 15,
                fontWeight: FontWeight.w700,
                color: t.ink,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              body,
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 13, height: 1.4, color: t.ink2),
            ),
          ],
        ),
      ),
    );
  }
}

/// Choose a repository: All repositories first, then each repository the computer lists (host, how many open issues,
/// and "Not on this computer" for one with no clone). Pops the key of the choice, empty for all.
class _RepoSheet extends ConsumerWidget {
  const _RepoSheet({
    required this.repos,
    required this.current,
    required this.computer,
  });
  final List<StartRepo> repos;
  final StartRepo? current;
  final String computer;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final total = repos.fold<int>(0, (n, r) => n + r.issues.length);
    return FloatingSheet(
      children: [
        Semantics(
          header: true,
          child: Text(
            s('phone.rp.pick'),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 18,
              fontWeight: FontWeight.w700,
              height: 1.25,
              color: t.ink,
            ),
          ),
        ),
        const SizedBox(height: 12),
        _OptionCard(
          title: s('phone.rp.all'),
          subtitle: _nOpen(s, total),
          selected: current == null,
          onTap: () => Navigator.of(context).pop(''),
        ),
        for (final r in repos) ...[
          const SizedBox(height: 8),
          _OptionCard(
            title: r.path,
            subtitle: '${r.host} · ${_nOpen(s, r.issues.length)}',
            warn: r.cloned ? null : s('phone.rp.noclone'),
            selected: current?.key == r.key,
            onTap: () => Navigator.of(context).pop(r.key),
          ),
        ],
        const SizedBox(height: 10),
        FinePrint(s('phone.rp.note', {'c': computer})),
        const SizedBox(height: 10),
        PillButton(
          label: s('phone.a.close'),
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
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
