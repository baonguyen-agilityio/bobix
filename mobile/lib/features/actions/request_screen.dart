import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../data/models.dart';
import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass_backdrop.dart';
import '../../widgets/status_banner.dart';
import 'action_screen_parts.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'command_flow.dart';
import 'hidden_chars.dart';
import 'session_screen.dart';

/// An input longer than this (characters or lines) is collapsed, and Allow stays locked until it is opened.
const collapseChars = 400, collapseLines = 8;

final requestProvider = FutureProvider.autoDispose.family<RequestView, String>((ref, id) {
  // asked again once the link is back up: a screen opened while it was down gets the item then
  ref.watch(connProvider.select((c) => c == ConnState.ok));
  return ref.watch(actionsSourceProvider).request(id);
});

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
    final last = closed.last;
    // This phone had loaded the request: it stays on screen, read-only. Otherwise only the card and note remain.
    if (last != null) return RequestBody(request: last, closed: closed);
    return ActionPage(
      title: s('phone.rq.title'),
      onLog: taskLogOpener(context, ref, id),
      banner: _ClosedBanner(closed),
      actions: const _BackHome(),
      children: [
        ActionSessionCard(id: id, showIssue: false),
        const SizedBox(height: 12),
        ShieldNote(s('phone.rq.never')),
      ],
    );
  }
}

class _BackHome extends ConsumerWidget {
  const _BackHome();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return ActionButtons(
      primary: ActionButton(
        s('phone.a.home'),
        onPressed: () => Navigator.of(context).popUntil((r) => r.isFirst),
      ),
    );
  }
}

class _ClosedBanner extends ConsumerWidget {
  const _ClosedBanner(this.closed);
  final RequestClosed closed;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final c = ref.watch(actionsSourceProvider).computerName;
    final by = closed.by;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 0),
      child: StatusBanner(
        kind: by == ClosedBy.expired ? BannerKind.err : BannerKind.warn,
        icon: by == ClosedBy.expired ? Icons.schedule : Icons.error_outline,
        title: s(
          switch (by) {
            ClosedBy.computer => 'phone.cl.pc',
            ClosedBy.thisPhone => 'phone.cl.me',
            ClosedBy.otherPhone => 'phone.cl.ph',
            ClosedBy.expired => 'phone.cl.ex',
          },
          {'c': c, 'p': closed.phone ?? ''},
        ),
        body: s(
          switch (by) {
            ClosedBy.computer => 'phone.cl.pc.d',
            ClosedBy.thisPhone => 'phone.cl.me.d',
            // The computer names the other phone by id only: without a name the line does not use one.
            ClosedBy.otherPhone when (closed.phone ?? '').isEmpty =>
              'phone.cl.ph.d0',
            ClosedBy.otherPhone => 'phone.cl.ph.d',
            ClosedBy.expired => 'phone.cl.ex.d',
          },
          {'tm': closed.at, 'c': c, 'p': closed.phone ?? ''},
        ),
      ),
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
  const RequestBody({super.key, required this.request, this.closed});
  final ToolRequest request;

  /// Set when the request closed after this phone loaded it: the same view, read-only, under the closed banner.
  final RequestClosed? closed;

  @override
  ConsumerState<RequestBody> createState() => _RequestBodyState();
}

class _RequestBodyState extends ConsumerState<RequestBody> {
  bool _open = false;
  // The seconds left tick in this notifier, so only the chip that shows them rebuilds each second. The page rebuilds
  // once more when it runs out (the buttons lock).
  final _left = ValueNotifier<int?>(null);
  bool _expired = false;
  Timer? _timer;
  // Read from the input once, not on every build: it can be 20 000 characters.
  late bool _long;
  late bool _hidden;

  @override
  void initState() {
    super.initState();
    _scan();
    _left.value = widget.closed == null ? widget.request.secondsLeft : null;
    _expired = _left.value == 0;
    // Counted from "seconds left", never from the two clocks.
    if (_left.value != null) {
      _timer = Timer.periodic(const Duration(seconds: 1), (_) {
        if (!mounted) return;
        final n = (_left.value! - 1).clamp(0, 1 << 30);
        _left.value = n;
        if (n == 0) {
          _timer?.cancel();
          setState(() => _expired = true);
        }
      });
    }
  }

  void _scan() {
    final input = widget.request.input;
    _long =
        input.length > collapseChars ||
        '\n'.allMatches(input).length >= collapseLines;
    _hidden = hasHiddenChars(input);
  }

  @override
  void didUpdateWidget(RequestBody old) {
    super.didUpdateWidget(old);
    if (old.request.input != widget.request.input) _scan();
  }

  @override
  void dispose() {
    _timer?.cancel();
    _left.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final r = widget.request;
    final source = ref.watch(actionsSourceProvider);
    final c = source.computerName;
    final canSend = ref.watch(canSendProvider);
    final closed = widget.closed;
    final odd = r.odd || _hidden;
    final destructive = closed == null && (r.destructive || odd);
    final expired = _expired;
    final collapsed = _long && !_open;
    final locked = collapsed || r.cut || expired;
    final label = s(switch (r.kind) {
      InputKind.bash => 'phone.rq.k.bash',
      InputKind.write => 'phone.rq.k.write',
      InputKind.edit => 'phone.rq.k.edit',
      InputKind.mcp => 'phone.rq.k.mcp',
    });

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
      banner: closed == null ? null : _ClosedBanner(closed),
      actions: closed != null
          ? const _BackHome()
          : r.cut
          ? ActionButtons(primary: deny, fine: fine)
          : ActionButtons(
              secondary: deny,
              primary: allow,
              fine: fine,
              lock: !collapsed,
            ),
      children: [
        if (closed == null) const SendOffBanner(),
        if (closed == null && r.released)
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
          chip: closed == null ? waitedText(s, r.waitedMinutes) : null,
          extraChips: [
            if (_left.value != null)
              ValueListenableBuilder<int?>(
                valueListenable: _left,
                builder: (_, left, _) {
                  final n = left ?? 0;
                  return MetaChip(
                    n == 0
                        ? s('phone.rq.expd')
                        : n < 60
                        ? s('phone.rq.exp.s', {'n': n})
                        : s('phone.rq.exp', {'n': (n / 60).ceil()}),
                    icon: Icons.timer_outlined,
                    color: n < 60 ? t.err : null,
                  );
                },
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
        if (r.cut && closed == null)
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
            Padding(
              padding: const EdgeInsets.only(right: 14),
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
        // Stop lives on the session's screen (with what it does); a held request is where the person meets it.
        if (closed == null &&
            ref.watch(homeSessionProvider(r.id))?.canStop == true) ...[
          const SizedBox(height: 12),
          ActionButton(
            s('phone.rq.stop'),
            icon: Icons.chevron_right,
            onPressed: () => Navigator.of(context).push(
              pageRoute<void>(
                SessionScreen(
                  id: ref.read(homeSessionProvider(r.id))!.id,
                  fromItem: true,
                ),
              ),
            ),
          ),
        ],
        const SizedBox(height: 14),
        ShieldNote(s('phone.rq.never')),
      ],
    );
  }
}
