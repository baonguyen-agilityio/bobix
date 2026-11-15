import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import '../../widgets/status_banner.dart';
import 'action_screen_parts.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'command_flow.dart';
import 'hidden_chars.dart';

/// An input longer than this (characters or lines) is collapsed, and Allow stays locked until it is opened.
const collapseChars = 400, collapseLines = 8;

final requestProvider = FutureProvider.autoDispose.family<RequestView, String>(
  (ref, id) => ref.watch(actionsSourceProvider).request(id),
);

/// A permission request: the whole tool input, then Allow or Deny. An id the app does not know yet shows "loading";
/// "closed" shows only when the computer says so.
class RequestScreen extends ConsumerWidget {
  const RequestScreen({super.key, required this.id});
  final String id;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final c = ref.watch(actionsSourceProvider).computerName;
    final view = ref.watch(requestProvider(id));
    return view.when(
      loading: () => _Status(
        icon: Icons.hourglass_empty,
        title: s('phone.rq.loading'),
        body: s('phone.rq.loading.d', {'c': c}),
        spinner: true,
      ),
      error: (e, _) => _Status(
        icon: Icons.error_outline,
        title: s('phone.rq.loading'),
        body: '$e',
      ),
      data: (v) => switch (v) {
        RequestLoading() => _Status(
          icon: Icons.hourglass_empty,
          title: s('phone.rq.loading'),
          body: s('phone.rq.loading.d', {'c': c}),
          spinner: true,
        ),
        RequestClosed() => _ClosedView(id: id, closed: v),
        RequestOpen() => RequestBody(request: v.request),
      },
    );
  }
}

/// A request that closed without this phone: the request's own view stays, under a banner that says who answered.
class _ClosedView extends ConsumerWidget {
  const _ClosedView({required this.id, required this.closed});
  final String id;
  final RequestClosed closed;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final c = ref.watch(actionsSourceProvider).computerName;
    final by = closed.by;
    return ActionPage(
      title: s('phone.rq.title'),
      onLog: taskLogOpener(context, ref, id),
      banner: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 0),
        child: StatusBanner(
          kind: by == ClosedBy.expired ? BannerKind.err : BannerKind.warn,
          icon: by == ClosedBy.expired ? Icons.schedule : Icons.error_outline,
          title: s(
            switch (by) {
              ClosedBy.computer => 'phone.cl.pc',
              ClosedBy.otherPhone => 'phone.cl.ph',
              ClosedBy.expired => 'phone.cl.ex',
            },
            {'c': c, 'p': closed.phone ?? ''},
          ),
          body: s(
            switch (by) {
              ClosedBy.computer => 'phone.cl.pc.d',
              ClosedBy.otherPhone => 'phone.cl.ph.d',
              ClosedBy.expired => 'phone.cl.ex.d',
            },
            {'tm': closed.at, 'c': c, 'p': closed.phone ?? ''},
          ),
        ),
      ),
      actions: ActionButtons(
        primary: ActionButton(
          s('phone.a.home'),
          onPressed: () => Navigator.of(context).popUntil((r) => r.isFirst),
        ),
      ),
      children: [
        ActionSessionCard(id: id, showIssue: false),
        const SizedBox(height: 12),
        ShieldNote(s('phone.rq.never')),
      ],
    );
  }
}

class _Status extends ConsumerWidget {
  const _Status({
    required this.icon,
    required this.title,
    required this.body,
    this.spinner = false,
  });
  final IconData icon;
  final String title, body;
  final bool spinner;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return ActionPage(
      title: s('phone.rq.title'),
      actions: SizedBox(
        width: double.infinity,
        child: ActionButton(
          s('phone.a.home'),
          onPressed: () => Navigator.of(context).popUntil((r) => r.isFirst),
        ),
      ),
      children: [
        const SizedBox(height: 60),
        Center(
          child: spinner
              ? const CircularProgressIndicator()
              : Container(
                  width: 76,
                  height: 76,
                  decoration: BoxDecoration(
                    color: t.infoBg,
                    shape: BoxShape.circle,
                  ),
                  child: Icon(icon, size: 36, color: t.info),
                ),
        ),
        const SizedBox(height: 16),
        Semantics(
          header: true,
          child: Text(
            title,
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 22,
              fontWeight: FontWeight.w700,
              color: t.ink,
            ),
          ),
        ),
        const SizedBox(height: 8),
        Text(
          body,
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 14, height: 1.5, color: t.ink2),
        ),
      ],
    );
  }
}

/// The open request. Stateful for two things only: whether a long input was opened, and the countdown.
class RequestBody extends ConsumerStatefulWidget {
  const RequestBody({super.key, required this.request});
  final ToolRequest request;

  @override
  ConsumerState<RequestBody> createState() => _RequestBodyState();
}

class _RequestBodyState extends ConsumerState<RequestBody> {
  bool _open = false;
  int? _left;
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _left = widget.request.secondsLeft;
    // Counted from "seconds left", never from the two clocks.
    if (_left != null) {
      _timer = Timer.periodic(const Duration(seconds: 1), (_) {
        if (!mounted) return;
        setState(() => _left = (_left! - 1).clamp(0, 1 << 30));
        if (_left == 0) _timer?.cancel();
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  bool get _long =>
      widget.request.input.length > collapseChars ||
      '\n'.allMatches(widget.request.input).length >= collapseLines;

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final r = widget.request;
    final source = ref.watch(actionsSourceProvider);
    final c = source.computerName;
    final canSend = ref.watch(canSendProvider);
    final odd = r.odd || hasHiddenChars(r.input);
    final destructive = r.destructive || odd;
    final expired = _left == 0;
    final collapsed = _long && !_open;
    final locked = collapsed || r.cut || expired;
    final label = s(switch (r.kind) {
      InputKind.bash => 'phone.rq.k.bash',
      InputKind.write => 'phone.rq.k.write',
      InputKind.edit => 'phone.rq.k.edit',
      InputKind.mcp => 'phone.rq.k.mcp',
    });

    String countdown() {
      final n = _left!;
      if (n == 0) return s('phone.rq.expd');
      return n < 60
          ? s('phone.rq.exp.s', {'n': n})
          : s('phone.rq.exp', {'n': (n / 60).ceil()});
    }

    VoidCallback? send(CommandKind kind) => !canSend || expired
        ? null
        : () => runCommand(
            context,
            ref,
            kind: kind,
            target: r.id,
            shown: r.shownHash,
            tool: r.tool,
            // A long input is cut here: the log shows a line, the computer holds the rest.
            detail: r.input.length > 300 ? r.input.substring(0, 300) : r.input,
            reason: s(
              kind == CommandKind.allow ? 'phone.un.allow' : 'phone.un.deny',
              {'tool': r.tool.isEmpty ? s('phone.ask.tool0') : r.tool, 'c': c},
            ),
            warning: kind == CommandKind.allow && destructive
                ? s(odd ? 'phone.rq.hid' : 'phone.rq.destr')
                : null,
          );

    final deny = ActionButton(
      s('phone.ask.deny'),
      onPressed: send(CommandKind.deny),
      kind: destructive ? BtnKind.dangerFill : BtnKind.normal,
      icon: Icons.lock_outline,
    );
    final allow = ActionButton(
      s(destructive ? 'phone.rq.allow.d' : 'phone.ask.allow'),
      onPressed: locked ? null : send(CommandKind.allow),
      kind: destructive ? BtnKind.danger : BtnKind.primary,
      icon: Icons.lock_outline,
    );
    final conn = ref.watch(connProvider);
    final fine = r.cut
        ? s('phone.rq.cut.s')
        : collapsed
        ? s('phone.rq.openfirst')
        : '${s('phone.rq.unlock', {'os': bioName(s)})} · ${sendNote(s, conn, c)}';

    return ActionPage(
      title: s('phone.rq.title'),
      onLog: taskLogOpener(context, ref, r.id),
      actions: r.cut
          ? ActionButtons(primary: deny, fine: fine)
          : ActionButtons(
              secondary: deny,
              primary: allow,
              fine: fine,
              lock: !collapsed,
            ),
      children: [
        const SendOffBanner(),
        if (r.released)
          StatusBanner(
            kind: BannerKind.info,
            icon: Icons.computer,
            title: s('phone.rq.rel'),
            body: s('phone.rq.rel.d', {'c': c}),
          ),
        ActionSessionCard(
          id: r.id,
          name: r.sessionName,
          showIssue: false,
          chip: waitedText(s, r.waitedMinutes),
          extraChips: [
            if (_left != null)
              MetaChip(
                countdown(),
                icon: Icons.timer_outlined,
                color: _left! < 60 ? t.err : null,
              ),
          ],
        ),
        ScreenHeading(
          r.tool.isEmpty
              ? s('phone.ask.h0')
              : s('phone.ask.h', {'tool': r.tool}),
        ),
        if (destructive)
          DangerBand(
            title: s(odd ? 'phone.rq.hid' : 'phone.rq.destr'),
            body: s(odd ? 'phone.rq.hid.d' : 'phone.rq.destr.d'),
          ),
        if (r.cut)
          StatusBanner(
            kind: BannerKind.warn,
            icon: Icons.content_cut,
            title: s('phone.rq.cut'),
            body: s('phone.rq.cut.d'),
          ),
        Padding(
          padding: const EdgeInsets.only(bottom: 6),
          child: Text(
            label,
            style: TextStyle(
              fontSize: 13,
              fontWeight: FontWeight.w600,
              color: t.ink3,
            ),
          ),
        ),
        CodeBlock(
          r.input,
          danger: destructive,
          maxHeight: collapsed ? 150 : null,
        ),
        if (_long)
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton(
              onPressed: () => setState(() => _open = !_open),
              style: TextButton.styleFrom(minimumSize: const Size(44, 44)),
              child: Text(
                _open
                    ? s('phone.rq.less')
                    : s('phone.rq.full', {'n': r.input.length}),
              ),
            ),
          ),
        const SizedBox(height: 12),
        Row(
          crossAxisAlignment: CrossAxisAlignment.baseline,
          textBaseline: TextBaseline.alphabetic,
          children: [
            SizedBox(
              width: 104,
              child: Text(
                s('phone.rq.cwd'),
                style: TextStyle(fontSize: 13, color: t.ink3),
              ),
            ),
            Expanded(
              child: Text(
                r.folder,
                style: TextStyle(
                  fontFamily: 'Menlo',
                  fontFamilyFallback: const ['Roboto Mono', 'monospace'],
                  fontSize: 13,
                  color: t.ink,
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 14),
        ShieldNote(s('phone.rq.never')),
      ],
    );
  }
}
