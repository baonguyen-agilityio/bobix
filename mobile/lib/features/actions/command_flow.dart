import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';
import 'result_screen.dart';

/// The one command path of the action screens: the confirm step (the system's biometric prompt, which is also the
/// signature), then send, then the result screen. A cancelled confirm sends nothing and leaves the screen as it was.
/// A command is built here with a fresh id and sent once; no screen offers to send it again.
Future<void> runCommand(
  BuildContext context,
  WidgetRef ref, {
  required CommandKind kind,
  required String target,
  required String shown,
  required String reason,
  String? warning,
  String? text,
  StartMode? mode,
}) async {
  if (!ref.read(canSendProvider)) return;
  final navigator = Navigator.of(context);
  final messenger = ScaffoldMessenger.of(context);
  final s = ref.read(stringsProvider);
  final source = ref.read(actionsSourceProvider);
  final confirmed = await showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    backgroundColor: Colors.transparent,
    builder: (_) => UnlockSheet(reason: reason, warning: warning),
  );
  if (confirmed != true) {
    messenger.showSnackBar(SnackBar(content: Text(s('phone.un.cancelled'))));
    return;
  }
  final command = Command(
    id: source.newCommandId(),
    kind: kind,
    target: target,
    shown: shown,
    text: text,
    mode: mode,
  );
  navigator.pushReplacement(
    MaterialPageRoute<void>(
      builder: (_) =>
          ResultScreen(command: command, first: source.send(command)),
    ),
  );
}

/// The designed reason shown while the system asks for the biometric. Dismissing it cancels.
class UnlockSheet extends ConsumerStatefulWidget {
  const UnlockSheet({super.key, required this.reason, this.warning});
  final String reason;
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
    final yes = await ref.read(biometricGateProvider).confirm(widget.reason);
    if (!mounted) return;
    if (!yes) return Navigator.of(context).pop(false);
    setState(() => _ok = true);
    await Future<void>.delayed(const Duration(milliseconds: 350));
    if (mounted) Navigator.of(context).pop(true);
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final ios = Theme.of(context).platform == TargetPlatform.iOS;
    final c = _ok ? t.ok : t.accentInk;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(8),
        child: Container(
          padding: const EdgeInsets.fromLTRB(20, 12, 20, 16),
          decoration: BoxDecoration(
            color: t.surface,
            borderRadius: BorderRadius.circular(32),
            border: Border.all(width: .5, color: t.line),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 38,
                height: 5,
                decoration: BoxDecoration(
                  color: t.lineStrong,
                  borderRadius: BorderRadius.circular(3),
                ),
              ),
              const SizedBox(height: 14),
              Container(
                width: 76,
                height: 76,
                decoration: BoxDecoration(
                  color: _ok ? t.okBg : t.accentBg,
                  shape: BoxShape.circle,
                ),
                child: Icon(
                  _ok
                      ? Icons.check_rounded
                      : (ios ? Icons.face : Icons.fingerprint),
                  size: 40,
                  color: c,
                ),
              ),
              const SizedBox(height: 10),
              Semantics(
                header: true,
                child: Text(
                  _ok ? s('phone.un.ok') : s('phone.un.h'),
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
                  onPressed: () => Navigator.of(context).pop(false),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
