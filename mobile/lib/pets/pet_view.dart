import 'dart:async';
import 'dart:ui' show ImageFilter;

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../theme/tokens.dart';
import 'creature.dart';

/// A session's pet, drawn from the SVG of [flatSvg] and animated by frame steps. The animation stops when the pet is not
/// on screen (a hidden tab) and when the system asks for reduced motion; it then shows the state's first frame.
class PetView extends StatefulWidget {
  const PetView({
    super.key,
    required this.design,
    required this.color,
    required this.state,
    this.size = 44,
    this.halo = false,
  });

  /// One of [designs]; anything else draws the default pet.
  final String design;
  final int color;

  /// One of [petStates].
  final String state;
  final double size;

  /// A pet that waits for the person: a thin warm halo hugging its outline, breathing over [haloCycle] (still under
  /// reduced motion).
  final bool halo;

  static const haloCycle = Duration(milliseconds: 3200);

  /// How many pets are stepping their pose right now: none on a hidden tab, none under reduced motion.
  @visibleForTesting
  static int get stepping => _stepping;
  static int _stepping = 0;

  @override
  State<PetView> createState() => _PetViewState();
}

class _PetViewState extends State<PetView> with SingleTickerProviderStateMixin, WidgetsBindingObserver {
  late final AnimationController _breath = AnimationController(
    vsync: this,
    duration: PetView.haloCycle ~/ 2,
  );
  // The pose steps on a timer that fires only when the frame changes (every 150 ms to 1.2 s), so a pet that is on
  // screen does not keep the engine drawing at the display's rate between steps.
  final _clock = Stopwatch();
  Timer? _timer;
  Pose _pose = ('', 0);

  bool _still = false;
  bool _shown = true;
  // The timer is not a Ticker, so the engine's own pause does not stop it: a pet steps only while the app is in front
  // (a background wake of the app runs Dart too).
  bool _foreground = true;

  @override
  void initState() {
    super.initState();
    _pose = poseAt(widget.state);
    WidgetsBinding.instance.addObserver(this);
    final life = WidgetsBinding.instance.lifecycleState;
    _foreground = life == null || life == AppLifecycleState.resumed;
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _foreground = state == AppLifecycleState.resumed;
    _syncClock();
  }

  // Schedules the next frame boundary of the current pose, counted from when the clock started.
  void _schedule() {
    final (_, ms) = loops[_pose.$1] ?? (1, 1000);
    final into = _clock.elapsedMilliseconds;
    _timer = Timer(Duration(milliseconds: ms - into % ms), _step);
  }

  void _step() {
    if (!mounted) return;
    final next = poseAt(widget.state, _clock.elapsedMilliseconds);
    if (next != _pose) setState(() => _pose = next);
    _schedule();
  }

  void _stopClock() {
    if (_timer != null) PetView._stepping--;
    _timer?.cancel();
    _timer = null;
    _clock
      ..stop()
      ..reset();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _still = MediaQuery.disableAnimationsOf(context);
    // As a Ticker did: a pet under a disabled TickerMode (a hidden tab) does not step.
    _shown = TickerMode.valuesOf(context).enabled;
    _syncClock();
    _syncBreath(_still);
  }

  void _syncClock() {
    final run = !_still && _shown && _foreground;
    if (!run && _timer != null) {
      _stopClock();
      if (!_foreground && !_still && _shown) return;
      _pose = poseAt(widget.state);
    } else if (run && _timer == null) {
      PetView._stepping++;
      _clock.start();
      _schedule();
    }
  }

  // Half a cycle each way, ease-in-out, so the whole cycle is 3.2 s.
  void _syncBreath(bool still) {
    if (widget.halo && !still) {
      if (!_breath.isAnimating) _breath.repeat(reverse: true);
    } else {
      _breath.stop();
      _breath.value = 0.5;
    }
  }

  @override
  void didUpdateWidget(PetView old) {
    super.didUpdateWidget(old);
    if (old.state != widget.state) {
      _pose = poseAt(widget.state);
      if (_timer != null) {
        _timer!.cancel();
        _clock
          ..reset()
          ..start();
        _schedule();
      }
    }
    if (old.halo != widget.halo) {
      _syncBreath(MediaQuery.disableAnimationsOf(context));
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _stopClock();
    _breath.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final svg = SvgPicture.string(
      flatSvg(widget.design, widget.color, widget.state, _pose, ring: false),
      width: widget.size,
      height: widget.size,
    );
    return ExcludeSemantics(
      child: RepaintBoundary(
        child: SizedBox.square(
          dimension: widget.size,
          child: widget.halo
              ? Stack(
                  clipBehavior: Clip.none,
                  children: [
                    Positioned.fill(
                      child: _Halo(breath: _breath, child: svg),
                    ),
                    svg,
                  ],
                )
              : svg,
        ),
      ),
    );
  }
}

/// Two soft shadows of the pet's own outline in the warm accent, blurred, as the prototype's two `drop-shadow`s.
class _Halo extends StatelessWidget {
  const _Halo({required this.breath, required this.child});
  final Animation<double> breath;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final dark = context.tokens.isDark;
    // (blur radius at rest, at full breath, alpha at rest, at full breath); a CSS blur radius is two sigmas.
    final layers = dark
        ? const [
            (Color(0xFFFF8C64), 2.0, 3.0, .35, .60),
            (Color(0xFFFF7A4D), 7.0, 12.0, .15, .28),
          ]
        : const [
            (Color(0xFFFF7A4D), 1.5, 2.5, .35, .55),
            (Color(0xFFFF7A4D), 5.0, 9.0, .12, .22),
          ];
    return AnimatedBuilder(
      animation: breath,
      builder: (_, _) {
        final k = Curves.easeInOut.transform(breath.value);
        double lerp(double a, double b) => a + (b - a) * k;
        return Stack(
          clipBehavior: Clip.none,
          children: [
            for (final (color, r0, r1, a0, a1) in layers)
              Positioned.fill(
                child: ImageFiltered(
                  imageFilter: ImageFilter.blur(
                    sigmaX: lerp(r0, r1) / 2,
                    sigmaY: lerp(r0, r1) / 2,
                    tileMode: TileMode.decal,
                  ),
                  child: ColorFiltered(
                    colorFilter: ColorFilter.mode(
                      color.withValues(alpha: lerp(a0, a1)),
                      BlendMode.srcIn,
                    ),
                    child: child,
                  ),
                ),
              ),
          ],
        );
      },
    );
  }
}
