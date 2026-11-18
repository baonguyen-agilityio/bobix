import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../lock/own_prompt.dart';
import '../../theme/tokens.dart';
import '../../widgets/face_id_icon.dart';
import '../../widgets/floating_sheet.dart';
import 'action_widgets.dart';
import 'actions_models.dart';
import 'actions_source.dart';

/// The pause between the sheet being fully shown and the system's biometric prompt, so the reason can be read first.
const _readPause = Duration(milliseconds: 250);

/// What the check shows when it worked: the glyph turns into a check (250 ms), then it holds (400 ms) before the sheet
/// leaves.
const _morph = Duration(milliseconds: 250);
const _hold = Duration(milliseconds: 400);

/// The length of the cross-fades that replace every movement when the system asks for reduced motion.
const _fadeTime = Duration(milliseconds: 150);

enum _Phase { read, asking, ok, failed }

/// The designed reason shown around the system's biometric prompt. The sheet is fully shown first, then the check is
/// asked over it. A check that worked turns the glyph into a check and the sheet leaves with
/// [GateResult.confirmed] (the middle of its circle goes to [circle] first, so the result screen can grow its icon from
/// there). A check that did not work shakes the circle and says why; the sheet stays, with "Try again" when trying again
/// can help. Cancel, or dismissing the sheet, leaves with [GateResult.cancelled].
class UnlockSheet extends ConsumerStatefulWidget {
  const UnlockSheet({
    super.key,
    required this.reason,
    required this.command,
    this.warning,
    this.circle,
  });
  final String reason;
  final Command command;
  final String? warning;
  final ValueNotifier<Offset?>? circle;

  @override
  ConsumerState<UnlockSheet> createState() => _UnlockSheetState();
}

class _UnlockSheetState extends ConsumerState<UnlockSheet>
    with TickerProviderStateMixin {
  _Phase _phase = _Phase.read;
  GateResult? _failure;
  final _circleKey = GlobalKey();

  late final _shake = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 400),
  );
  late final _pulse = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 700),
  );
  late final _fade = AnimationController(vsync: this, duration: _fadeTime);

  bool get _reduced => MediaQuery.disableAnimationsOf(context);
  bool get _waiting => _phase == _Phase.read || _phase == _Phase.asking;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) unawaited(_run());
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _syncPulse();
  }

  @override
  void dispose() {
    _shake.dispose();
    _pulse.dispose();
    _fade.dispose();
    super.dispose();
  }

  void _syncPulse() {
    if (_waiting && !_reduced) {
      if (!_pulse.isAnimating) _pulse.repeat(reverse: true);
    } else {
      _pulse.stop();
    }
  }

  void _set(_Phase phase) {
    setState(() => _phase = phase);
    _syncPulse();
  }

  /// Completes when the sheet has fully appeared: its slide is done, or (reduced motion) its fade.
  Future<void> _shown() async {
    final slide = ModalRoute.of(context)?.animation;
    if (slide != null && !slide.isCompleted) {
      final done = Completer<void>();
      void watch(AnimationStatus s) {
        if (s == AnimationStatus.completed && !done.isCompleted) {
          done.complete();
        }
      }

      slide.addStatusListener(watch);
      await done.future;
      slide.removeStatusListener(watch);
    }
    if (mounted && _reduced) await _fade.forward();
  }

  Future<void> _run() async {
    await _shown();
    if (!mounted) return;
    await Future<void>.delayed(_readPause);
    unawaited(_ask());
  }

  Future<void> _ask() async {
    if (!mounted) return;
    _set(_Phase.asking);
    final result = await duringOwnPrompt(
      () => ref
          .read(biometricGateProvider)
          .confirm(widget.reason, widget.command),
    );
    if (!mounted) return;
    if (result != GateResult.confirmed) {
      _failure = result;
      _set(_Phase.failed);
      if (!_reduced) unawaited(_shake.forward(from: 0));
      return;
    }
    _set(_Phase.ok);
    await Future<void>.delayed((_reduced ? _fadeTime : _morph) + _hold);
    if (!mounted) return;
    final box = _circleKey.currentContext?.findRenderObject() as RenderBox?;
    if (box != null && box.attached) {
      widget.circle?.value = box.localToGlobal(box.size.center(Offset.zero));
    }
    if (_reduced) await _fade.reverse();
    if (mounted) Navigator.of(context).pop(GateResult.confirmed);
  }

  Offset _shakeOffset() {
    final v = TweenSequence<double>([
      for (final (a, b) in [
        (0.0, -7.0),
        (-7.0, 6.0),
        (6.0, -4.0),
        (-4.0, 3.0),
        (3.0, 0.0),
      ])
        TweenSequenceItem(
          tween: Tween(begin: a, end: b),
          weight: 1,
        ),
    ]).transform(_shake.value);
    return Offset(v, 0);
  }

  String _failureLine(Strings s) {
    final os = {'os': bioName(s)};
    return switch (_failure) {
      GateResult.failed => s('phone.un.err.fail', os),
      GateResult.lockedOut => s('phone.un.err.lock', os),
      GateResult.unavailable => s('phone.un.err.none', os),
      GateResult.keyLost => s('phone.un.keylost'),
      _ => s('phone.un.err.cancel', os),
    };
  }

  /// Asking again can help after a check that did not recognise the person, or was cancelled.
  bool get _canRetry =>
      _failure == GateResult.failed || _failure == GateResult.cancelled;

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final ios = Theme.of(context).platform == TargetPlatform.iOS;
    final reduced = _reduced;
    final ok = _phase == _Phase.ok;
    final failed = _phase == _Phase.failed;
    final fg = ok
        ? t.ok
        : failed
        ? t.err
        : t.accentInk;
    final bg = ok
        ? t.okBg
        : failed
        ? t.err.withValues(alpha: .14)
        : t.accentBg;
    final glyph = ok
        ? Icon(Icons.check_rounded, size: 40, color: fg)
        : bioGlyph(size: 40, color: fg);
    final sheet = FloatingSheet(
      horizontal: 20,
      children: [
        const SizedBox(height: 6),
        Center(
          child: AnimatedBuilder(
            animation: Listenable.merge([_shake, _pulse]),
            builder: (context, child) {
              final spread = Curves.easeInOut.transform(_pulse.value) * 10;
              return Transform.translate(
                offset: _shake.isAnimating ? _shakeOffset() : Offset.zero,
                child: SizedBox(
                  width: 76,
                  height: 76,
                  child: Stack(
                    alignment: Alignment.center,
                    clipBehavior: Clip.none,
                    children: [
                      if (_waiting && !reduced)
                        Container(
                          width: 76 + 2 * spread,
                          height: 76 + 2 * spread,
                          decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            border: Border.all(
                              color: t.accentBg,
                              width: spread,
                            ),
                          ),
                        ),
                      child!,
                    ],
                  ),
                ),
              );
            },
            child: AnimatedContainer(
              key: _circleKey,
              duration: reduced ? _fadeTime : _morph,
              width: 76,
              height: 76,
              decoration: BoxDecoration(color: bg, shape: BoxShape.circle),
              child: AnimatedSwitcher(
                duration: reduced ? _fadeTime : _morph,
                transitionBuilder: (child, a) => FadeTransition(
                  opacity: a,
                  child: reduced
                      ? child
                      : ScaleTransition(
                          scale: Tween(begin: .4, end: 1.0).animate(a),
                          child: child,
                        ),
                ),
                child: Center(key: ValueKey(ok), child: glyph),
              ),
            ),
          ),
        ),
        const SizedBox(height: 4),
        Semantics(
          header: true,
          child: Text(
            ok ? s('phone.un.ok') : s('phone.un.h'),
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
        if (failed) ...[
          const SizedBox(height: 6),
          Semantics(
            liveRegion: true,
            child: Text(
              _failureLine(s),
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: t.err,
              ),
            ),
          ),
        ],
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
        if (!ok) ...[
          const SizedBox(height: 12),
          if (failed && _canRetry) ...[
            SizedBox(
              width: double.infinity,
              child: ActionButton(
                s('phone.un.try'),
                kind: BtnKind.primary,
                onPressed: _ask,
              ),
            ),
            const SizedBox(height: 8),
          ],
          SizedBox(
            width: double.infinity,
            child: ActionButton(
              s('phone.a.cancel'),
              onPressed: () => Navigator.of(context).pop(GateResult.cancelled),
            ),
          ),
        ],
      ],
    );
    return reduced ? FadeTransition(opacity: _fade, child: sheet) : sheet;
  }
}
