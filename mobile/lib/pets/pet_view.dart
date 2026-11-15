import 'dart:ui' show ImageFilter;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
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

  @override
  State<PetView> createState() => _PetViewState();
}

class _PetViewState extends State<PetView> with TickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_tick);
  late final AnimationController _breath = AnimationController(
    vsync: this,
    duration: PetView.haloCycle ~/ 2,
  );
  Pose _pose = ('', 0);

  @override
  void initState() {
    super.initState();
    _pose = poseAt(widget.state);
  }

  void _tick(Duration elapsed) {
    final next = poseAt(widget.state, elapsed.inMilliseconds);
    if (next != _pose) setState(() => _pose = next);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final still = MediaQuery.disableAnimationsOf(context);
    if (still && _ticker.isActive) {
      _ticker.stop();
      _pose = poseAt(widget.state);
    } else if (!still && !_ticker.isActive) {
      _ticker.start();
    }
    _syncBreath(still);
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
    if (old.state != widget.state) _pose = poseAt(widget.state);
    if (old.halo != widget.halo) {
      _syncBreath(MediaQuery.disableAnimationsOf(context));
    }
  }

  @override
  void dispose() {
    _ticker.dispose();
    _breath.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final svg = SvgPicture.string(
      flatSvg(
        widget.design,
        widget.color,
        widget.state,
        _pose,
        ring: !widget.halo,
      ),
      width: widget.size,
      height: widget.size,
    );
    return ExcludeSemantics(
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
