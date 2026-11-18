import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/glass_backdrop.dart';
import '../../data/phone_data.dart';
import '../../l10n/l10n.dart';
import '../../log/activity_log.dart';
import 'action_screen_parts.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'result_screen.dart';
import 'unlock_sheet.dart';

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
  final reduced = MediaQuery.disableAnimationsOf(context);
  final circle = ValueNotifier<Offset?>(null);
  final result = await showModalBottomSheet<GateResult>(
    context: context,
    isScrollControlled: true,
    backgroundColor: Colors.transparent,
    // The sheet rises in 320 ms and leaves the same way; with reduced motion it does not move: it fades in and out.
    sheetAnimationStyle: reduced
        ? const AnimationStyle(
            duration: Duration.zero,
            reverseDuration: Duration.zero,
          )
        : const AnimationStyle(
            duration: Duration(milliseconds: 320),
            reverseDuration: Duration(milliseconds: 320),
            curve: Cubic(.2, .8, .2, 1),
            reverseCurve: Cubic(.4, 0, 1, 1),
          ),
    builder: (_) => UnlockSheet(
      reason: reason,
      warning: warning,
      command: command,
      circle: circle,
    ),
  );
  // Why a check did not work is said in the sheet, which stays until the person cancels: leaving here is a choice.
  if (result != GateResult.confirmed) {
    messenger.showSnackBar(SnackBar(content: Text(s('phone.un.cancelled'))));
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
  // The result comes in while the sheet is still sliding down, its icon growing from where the sheet's circle was.
  navigator.pushReplacement(
    _ResultRoute(
      reduced: reduced,
      builder: (_) => GlassBackdrop(
        child: ResultScreen(
          command: command,
          first: first,
          iconFrom: circle.value,
        ),
      ),
    ),
  );
}

/// The route of the result after a command: it fades in over the screen it replaces (350 ms, 150 ms with reduced motion).
class _ResultRoute extends MaterialPageRoute<void> {
  _ResultRoute({required super.builder, required this.reduced});
  final bool reduced;

  @override
  Duration get transitionDuration =>
      Duration(milliseconds: reduced ? 150 : 350);

  @override
  Widget buildTransitions(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) => FadeTransition(opacity: animation, child: child);
}
