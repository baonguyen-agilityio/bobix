import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';
import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import '../../widgets/status_banner.dart';
import '../../data/phone_data.dart';
import '../../screens/app_log_screen.dart';
import '../../screens/task_log_screen.dart';
import 'action_screen_parts.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';

/// Minutes a command is held on the relay for a computer that is away.
const heldMinutes = 10;

/// The result of one command: what happened, said in words. A command is never sent again from here: a result that
/// did not come is asked for ("Check status"), and a held command can be withdrawn.
class ResultScreen extends ConsumerStatefulWidget {
  const ResultScreen({super.key, required this.command, required this.first});

  final Command command;

  /// The command's result as it arrives; the screen says "sending" until then.
  final Future<CommandOutcome> first;

  @override
  ConsumerState<ResultScreen> createState() => _ResultScreenState();
}

class _ResultScreenState extends ConsumerState<ResultScreen> {
  CommandOutcome? _outcome;
  bool _busy = false;
  bool _stillHeld = false;

  @override
  void initState() {
    super.initState();
    widget.first.then((o) {
      if (mounted) setState(() => _outcome = o);
    });
  }

  Future<void> _run(Future<CommandOutcome> Function() ask) async {
    setState(() {
      _busy = true;
      _stillHeld = false;
    });
    final o = await ask();
    if (!mounted) return;
    setState(() {
      _busy = false;
      // Still held: nothing changed, and the screen says so.
      if (o.kind == OutcomeKind.queued) {
        _stillHeld = true;
      } else {
        _outcome = o;
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final source = ref.watch(actionsSourceProvider);
    final canSend = ref.watch(canSendProvider);
    final o = _outcome;
    final c = source.computerName;
    final id = widget.command.id;

    if (o == null) {
      return ActionPage(
        title: s('phone.res.title'),
        showBack: false,
        children: [
          const SizedBox(height: 80),
          const Center(child: CircularProgressIndicator()),
          const SizedBox(height: 16),
          Center(
            child: Text(
              s('phone.res.sending'),
              style: TextStyle(color: t.ink2, fontSize: 15),
            ),
          ),
        ],
      );
    }

    final v = _view(s, o, c);
    final held = o.kind == OutcomeKind.queued;
    final lost = o.kind == OutcomeKind.notReceived;
    return ActionPage(
      title: s('phone.res.title'),
      showBack: false,
      actions: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (held)
            SizedBox(
              width: double.infinity,
              child: ActionButton(
                _busy ? s('phone.res.checking') : s('phone.res.check'),
                onPressed: canSend && !_busy
                    ? () => _run(() => source.checkStatus(id))
                    : null,
                icon: Icons.sync,
              ),
            ),
          if (held || lost) ...[
            if (held) const SizedBox(height: 8),
            SizedBox(
              width: double.infinity,
              child: ActionButton(
                s('phone.res.withdraw'),
                onPressed: canSend && !_busy
                    ? () => _run(() => source.withdraw(id))
                    : null,
                kind: BtnKind.danger,
              ),
            ),
            const SizedBox(height: 8),
          ],
          SizedBox(
            width: double.infinity,
            child: ActionButton(
              s('phone.a.home'),
              kind: held || lost ? BtnKind.normal : BtnKind.primary,
              onPressed: () => Navigator.of(context).popUntil((r) => r.isFirst),
            ),
          ),
          if (!held && !lost && o.kind != OutcomeKind.clock) ...[
            const SizedBox(height: 8),
            SizedBox(
              width: double.infinity,
              child: ActionButton(
                s('phone.res.seelog'),
                icon: Icons.format_list_bulleted,
                onPressed: () => _openLog(context),
              ),
            ),
          ],
          if (!canSend && (held || lost)) ...[
            const SizedBox(height: 8),
            FinePrint(s('phone.sn.nonet')),
          ],
        ],
      ),
      children: [
        const SendOffBanner(),
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 22, 4, 14),
          child: Column(
            children: [
              Container(
                width: 76,
                height: 76,
                decoration: BoxDecoration(
                  color: v.tone.bg(t),
                  shape: BoxShape.circle,
                ),
                child: Icon(v.icon, size: 38, color: v.tone.fg(t)),
              ),
              const SizedBox(height: 12),
              Semantics(
                header: true,
                liveRegion: true,
                child: Text(
                  v.title,
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
                v.body,
                textAlign: TextAlign.center,
                style: TextStyle(fontSize: 14, height: 1.5, color: t.ink2),
              ),
              if (v.note != null) ...[
                const SizedBox(height: 8),
                Text(
                  v.note!,
                  textAlign: TextAlign.center,
                  style: TextStyle(fontSize: 13, height: 1.5, color: t.ink3),
                ),
              ],
            ],
          ),
        ),
        if (_stillHeld)
          StatusBanner(
            kind: BannerKind.info,
            icon: Icons.schedule,
            title: s('phone.res.check.r'),
          ),
        if (o.kind == OutcomeKind.applied)
          Glass(
            padding: const EdgeInsets.all(14),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _kv(context, s('phone.res.k.what'), _what(s, widget.command)),
                _kv(context, s('phone.res.k.to'), c),
                _kv(
                  context,
                  s('phone.res.k.by'),
                  [?ref.watch(phoneNameProvider), bioName(s)].join(' · '),
                ),
                _kv(
                  context,
                  s('phone.res.k.log'),
                  s('phone.res.logged', {'c': c}),
                ),
              ],
            ),
          ),
      ],
    );
  }

  /// The task's log, or the app log for a command that belongs to no task yet (a ticket).
  void _openLog(BuildContext context) {
    final cmd = widget.command;
    final id = cmd.kind == CommandKind.start
        ? null
        : sessionFor(ref.read(homeProvider).value, cmd.target)?.id ??
              cmd.target;
    Navigator.of(context).push(
      pageRoute<void>(
        id == null ? const AppLogScreen() : TaskLogScreen(sessionId: id),
      ),
    );
  }

  Widget _kv(BuildContext context, String k, String v) {
    final t = context.tokens;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 104,
            child: Text(k, style: TextStyle(fontSize: 13, color: t.ink3)),
          ),
          Expanded(
            child: Text(v, style: TextStyle(fontSize: 13, color: t.ink)),
          ),
        ],
      ),
    );
  }

  String _what(Strings s, Command c) => s(switch (c.kind) {
    CommandKind.allow => 'phone.res.what.allow',
    CommandKind.deny => 'phone.res.what.deny',
    CommandKind.approve => 'phone.res.what.approve',
    CommandKind.back => 'phone.res.what.back',
    CommandKind.answer => 'phone.res.what.answer',
    CommandKind.stop => 'phone.res.what.stop',
    CommandKind.start =>
      c.mode == StartMode.quick
          ? 'phone.res.what.start-quick'
          : 'phone.res.what.start',
  });

  _View _view(Strings s, CommandOutcome o, String c) {
    final cmd = widget.command;
    switch (o.kind) {
      case OutcomeKind.applied:
        final k = switch (cmd.kind) {
          CommandKind.allow => 'allow',
          CommandKind.deny => 'deny',
          CommandKind.approve => 'approve',
          CommandKind.back => 'back',
          CommandKind.answer => 'answer',
          CommandKind.stop => 'stop',
          CommandKind.start =>
            cmd.mode == StartMode.quick ? 'start-quick' : 'start',
        };
        return _View(
          Icons.check_rounded,
          _Tone.ok,
          s('phone.res.ok.$k.t'),
          s('phone.res.ok.$k.d', {'c': c}),
        );
      case OutcomeKind.queued:
        return _View(
          Icons.schedule,
          _Tone.warn,
          s('phone.res.queued.t'),
          s('phone.res.queued.d', {'c': c, 'n': heldMinutes}),
          note: s('phone.res.queued.n'),
        );
      case OutcomeKind.notReceived:
        return _View(
          Icons.help_outline,
          _Tone.warn,
          s('phone.res.notrecv.t'),
          s('phone.res.notrecv.d', {'c': c, 'n': heldMinutes}),
          note: s('phone.res.notrecv.n', {'n': heldMinutes}),
        );
      case OutcomeKind.latePc:
        return _View(
          Icons.timer_off_outlined,
          _Tone.info,
          s('phone.res.late-pc.t'),
          s('phone.res.late-pc.d', {'c': c}),
        );
      case OutcomeKind.latePhone:
        return _View(
          Icons.phone_iphone,
          _Tone.info,
          s('phone.res.late-phone.t'),
          s('phone.res.late-phone.d', {'p': o.phone ?? ''}),
        );
      case OutcomeKind.expired:
        return _View(
          Icons.hourglass_disabled,
          _Tone.warn,
          s('phone.res.expired.t'),
          s('phone.res.expired.d'),
        );
      case OutcomeKind.released:
        return _View(
          Icons.undo,
          _Tone.info,
          s('phone.res.released.t'),
          switch (o.why) {
            ReleasedWhy.restarted => s('phone.res.released.restart.d', {
              'c': c,
            }),
            ReleasedWhy.settings => s('phone.res.released.settings.d', {
              'c': c,
            }),
            _ => s('phone.res.released.pc.d', {'c': c}),
          },
        );
      case OutcomeKind.stale:
        return _View(
          Icons.edit_note,
          _Tone.warn,
          s('phone.res.stale.t'),
          s('phone.res.stale.d', {'c': c}),
        );
      case OutcomeKind.clock:
        return _View(
          Icons.schedule,
          _Tone.err,
          s('phone.res.clk.t'),
          s('phone.res.clk.d', {'c': c}),
        );
      case OutcomeKind.refused:
        return _View(
          Icons.block,
          _Tone.err,
          s('phone.res.refused.t', {'c': c}),
          s('phone.res.refused.d', {'r': o.reason ?? ''}),
        );
      case OutcomeKind.withdrawn:
        return _View(
          Icons.undo,
          _Tone.info,
          s('phone.res.wd.t'),
          s('phone.res.withdrawn'),
        );
      case OutcomeKind.withdrawTooLate:
        return _View(
          Icons.timer_off_outlined,
          _Tone.warn,
          s('phone.res.wd.late.t'),
          s('phone.res.wd.late.d', {'c': c}),
        );
      case OutcomeKind.sessionEnded:
        return _View(
          Icons.stop_circle_outlined,
          _Tone.info,
          s('phone.res.ended.t'),
          s('phone.res.ended.d'),
        );
    }
  }
}

enum _Tone {
  ok,
  warn,
  info,
  err;

  Color fg(BobixTokens t) => switch (this) {
    ok => t.ok,
    warn => t.warn,
    info => t.info,
    err => t.err,
  };
  Color bg(BobixTokens t) => switch (this) {
    ok => t.okBg,
    warn => t.warnBg,
    info => t.infoBg,
    err => t.err.withValues(alpha: .14),
  };
}

class _View {
  const _View(this.icon, this.tone, this.title, this.body, {this.note});
  final IconData icon;
  final _Tone tone;
  final String title, body;
  final String? note;
}
