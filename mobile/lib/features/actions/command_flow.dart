import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';
import '../../widgets/face_id_icon.dart';

import '../../data/phone_data.dart';
import '../../l10n/l10n.dart';
import '../../log/activity_log.dart';
import '../../theme/tokens.dart';
import '../../widgets/floating_sheet.dart';
import 'action_screen_parts.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'result_screen.dart';

/// The one command path of the action screens: the confirm step (the system's biometric prompt, which is also the
/// signature), then send, then the result screen. A cancelled confirm sends nothing and leaves the screen as it was.
/// A command is built here with a fresh id and sent once; no screen offers to send it again. When its result comes the
/// log (in memory only) gets a line: [tool] and [detail] say what it was about (the command, the issue), [project] the
/// project when the session is not one Home knows.
Future<void> runCommand(
  BuildContext context,
  WidgetRef ref, {
  required CommandKind kind,
  required String target,
  required String shown,
  required String reason,
  String? warning,
  String? text,
  List<String>? answers,
  StartMode? mode,
  String tool = '',
  String detail = '',
  String project = '',
}) async {
  if (!ref.read(canSendProvider)) return;
  final navigator = Navigator.of(context);
  final messenger = ScaffoldMessenger.of(context);
  final s = ref.read(stringsProvider);
  final source = ref.read(actionsSourceProvider);
  final home = ref.read(homeProvider).value;
  final command = Command(
    id: source.newCommandId(),
    kind: kind,
    target: target,
    shown: shown,
    text: text,
    answers: answers,
    mode: mode,
  );
  final result = await showModalBottomSheet<GateResult>(
    context: context,
    isScrollControlled: true,
    backgroundColor: Colors.transparent,
    builder: (_) =>
        UnlockSheet(reason: reason, warning: warning, command: command),
  );
  if (result != GateResult.confirmed) {
    messenger.showSnackBar(
      SnackBar(
        content: Text(switch (result) {
          GateResult.lockedOut => s('phone.un.locked'),
          GateResult.unavailable => s('phone.un.nobio'),
          GateResult.failed => s('phone.un.failed'),
          GateResult.keyLost => s('phone.un.keylost'),
          _ => s('phone.un.cancelled'),
        }),
      ),
    );
    return;
  }
  final first = source.send(command);
  // A ticket belongs to no session yet: its line is in the app log.
  final session = kind == CommandKind.start ? null : sessionFor(home, target);
  final log = ref.read(activityLogProvider.notifier);
  unawaited(
    first.then(
      (o) => log.record(
        LogEntry(
          at: DateTime.now(),
          kind: switch (kind) {
            CommandKind.allow => LogKind.allow,
            CommandKind.deny => LogKind.deny,
            CommandKind.approve => LogKind.approve,
            CommandKind.back => LogKind.back,
            CommandKind.answer => LogKind.answer,
            CommandKind.stop => LogKind.stop,
            CommandKind.start =>
              mode == StartMode.quick ? LogKind.startQuick : LogKind.start,
          },
          sessionId: kind == CommandKind.start ? null : session?.id ?? target,
          project: session?.project ?? project,
          tool: tool,
          detail: detail,
          applied: o.kind == OutcomeKind.applied,
        ),
      ),
      onError: (_) {},
    ),
  );
  navigator.pushReplacement(
    pageRoute<void>(ResultScreen(command: command, first: first)),
  );
}

/// The designed reason shown while the system asks for the biometric. Dismissing it cancels.
class UnlockSheet extends ConsumerStatefulWidget {
  const UnlockSheet({
    super.key,
    required this.reason,
    required this.command,
    this.warning,
  });
  final String reason;
  final Command command;
  final String? warning;

  @override
  ConsumerState<UnlockSheet> createState() => _UnlockSheetState();
}

class _UnlockSheetState extends ConsumerState<UnlockSheet> {
  bool _ok = false;

  @override
  void initState() {
    super.initState();
    unawaited(_ask());
  }

  Future<void> _ask() async {
    final result = await ref
        .read(biometricGateProvider)
        .confirm(widget.reason, widget.command);
    if (!mounted) return;
    if (result != GateResult.confirmed) {
      return Navigator.of(context).pop(result);
    }
    setState(() => _ok = true);
    await Future<void>.delayed(const Duration(milliseconds: 350));
    if (mounted) Navigator.of(context).pop(GateResult.confirmed);
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final ios = Theme.of(context).platform == TargetPlatform.iOS;
    final c = _ok ? t.ok : t.accentInk;
    return FloatingSheet(
      horizontal: 20,
      children: [
        const SizedBox(height: 4),
        Center(
          child: Container(
            width: 76,
            height: 76,
            decoration: BoxDecoration(
              color: _ok ? t.okBg : t.accentBg,
              shape: BoxShape.circle,
            ),
            child: _ok
                ? Icon(Icons.check_rounded, size: 40, color: c)
                : bioGlyph(size: 40, color: c),
          ),
        ),
        const SizedBox(height: 10),
        Semantics(
          header: true,
          child: Text(
            _ok ? s('phone.un.ok') : s('phone.un.h'),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 18,
              fontWeight: FontWeight.w700,
              color: t.ink,
            ),
          ),
        ),
        const SizedBox(height: 6),
        Text(
          widget.reason,
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 14, color: t.ink2),
        ),
        if (widget.warning != null) ...[
          const SizedBox(height: 6),
          Text(
            widget.warning!,
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 13,
              fontWeight: FontWeight.w600,
              color: t.err,
            ),
          ),
        ],
        const SizedBox(height: 8),
        FinePrint(
          ios ? s('phone.un.ios', {'os': bioName(s)}) : s('phone.un.and'),
        ),
        const SizedBox(height: 12),
        SizedBox(
          width: double.infinity,
          child: ActionButton(
            s('phone.a.cancel'),
            onPressed: () => Navigator.of(context).pop(GateResult.cancelled),
          ),
        ),
      ],
    );
  }
}
