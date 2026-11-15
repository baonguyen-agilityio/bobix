import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import '../../widgets/status_banner.dart';
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
        RequestClosed() => _Status(
          icon: switch (v.by) {
            ClosedBy.computer => Icons.computer,
            ClosedBy.otherPhone => Icons.phone_iphone,
            ClosedBy.expired => Icons.hourglass_disabled,
          },
          title: s(switch (v.by) {
            ClosedBy.computer => 'phone.cl.pc',
            ClosedBy.otherPhone => 'phone.cl.ph',
            ClosedBy.expired => 'phone.cl.ex',
          }),
          body: s(
            switch (v.by) {
              ClosedBy.computer => 'phone.cl.pc.d',
              ClosedBy.otherPhone => 'phone.cl.ph.d',
              ClosedBy.expired => 'phone.cl.ex.d',
            },
            {'tm': v.at, 'c': c, 'p': v.phone ?? ''},
          ),
        ),
        RequestOpen() => RequestBody(request: v.request),
      },
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
    final odd = hasHiddenChars(r.input);
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
            reason: s(
              kind == CommandKind.allow ? 'phone.un.allow' : 'phone.un.deny',
              {'tool': r.tool, 'c': c},
            ),
            warning: kind == CommandKind.allow && destructive
                ? s(odd ? 'phone.rq.hid' : 'phone.rq.destr')
                : null,
          );

    final deny = ActionButton(
      s('phone.ask.deny'),
      onPressed: send(CommandKind.deny),
      kind: destructive ? BtnKind.dangerFill : BtnKind.normal,
    );
    final allow = ActionButton(
      s(destructive ? 'phone.rq.allow.d' : 'phone.ask.allow'),
      onPressed: locked ? null : send(CommandKind.allow),
      kind: destructive ? BtnKind.danger : BtnKind.primary,
      icon: Icons.lock_outline,
    );

    return ActionPage(
      title: s('phone.rq.title'),
      actions: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (r.cut)
            SizedBox(width: double.infinity, child: deny)
          else
            Row(
              children: [
                // A destructive request puts Deny first.
                Expanded(child: destructive ? deny : allow),
                const SizedBox(width: 10),
                Expanded(child: destructive ? allow : deny),
              ],
            ),
          const SizedBox(height: 8),
          FinePrint(
            r.cut
                ? s('phone.rq.cut.s')
                : collapsed
                ? s('phone.rq.openfirst')
                : s('phone.rq.unlock', {'os': bioName(s)}),
          ),
        ],
      ),
      children: [
        const SendOffBanner(),
        TitleCard(
          title: s('phone.ask.h', {'tool': r.tool}),
          subtitle: r.sessionName,
          chips: [
            MetaChip(s('phone.rq.on', {'c': c}), icon: Icons.computer),
            MetaChip(
              '${s('phone.rq.cwd')}: ${r.folder}',
              icon: Icons.folder_outlined,
            ),
            MetaChip(
              s('phone.home.waited', {
                'n': s('phone.min', {'n': r.waitedMinutes}),
              }),
              icon: Icons.schedule,
            ),
            if (_left != null)
              MetaChip(
                countdown(),
                icon: Icons.timer_outlined,
                color: _left! < 60 ? t.err : null,
              ),
          ],
        ),
        const SizedBox(height: 12),
        if (r.released)
          StatusBanner(
            kind: BannerKind.info,
            icon: Icons.computer,
            title: s('phone.rq.rel'),
            body: s('phone.rq.rel.d', {'c': c}),
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
          padding: const EdgeInsets.only(bottom: 6, top: 4),
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
        const SizedBox(height: 8),
        FinePrint(s('phone.rq.never'), align: TextAlign.left),
      ],
    );
  }
}
