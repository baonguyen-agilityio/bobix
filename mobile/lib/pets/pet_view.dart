import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_svg/flutter_svg.dart';

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
  });

  /// One of [designs]; anything else draws the default pet.
  final String design;
  final int color;

  /// One of [petStates].
  final String state;
  final double size;

  @override
  State<PetView> createState() => _PetViewState();
}

class _PetViewState extends State<PetView> with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_tick);
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
  }

  @override
  void didUpdateWidget(PetView old) {
    super.didUpdateWidget(old);
    if (old.state != widget.state) _pose = poseAt(widget.state);
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ExcludeSemantics(
      child: SizedBox.square(
        dimension: widget.size,
        child: SvgPicture.string(
          flatSvg(widget.design, widget.color, widget.state, _pose),
          width: widget.size,
          height: widget.size,
        ),
      ),
    );
  }
}
