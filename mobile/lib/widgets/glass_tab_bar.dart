import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme/tokens.dart';
import 'glass.dart';
import 'press_feedback.dart';

class TabSpec {
  const TabSpec(this.icon, this.label, {this.badge = 0});
  final IconData icon;
  final String label;
  final int badge;
}

/// The floating tab bar: a capsule with Home, Pets and Settings, and Start as a separate round button.
class GlassTabBar extends StatefulWidget {
  const GlassTabBar({
    super.key,
    required this.tabs,
    required this.start,
    required this.index,
    required this.startIndex,
    required this.onSelect,
  });

  final List<TabSpec> tabs; // in the order of their indexes, skipping Start
  final TabSpec start;
  final int index;
  final int startIndex;
  final ValueChanged<int> onSelect;

  static const double capsulePad = 3, itemW = 56, itemH = 42;

  /// The capsule's height; also the height of an item's touch target.
  static const double barH = itemH + capsulePad * 2;

  /// The gap under the bar. Over a system inset (the iPhone's home indicator, Android's gesture or button bar) the bar
  /// sits just above the inset and never into it; without one it keeps a little more air.
  static double bottomGap(BuildContext context) =>
      MediaQuery.viewPaddingOf(context).bottom > 0 ? 4 : 10;

  @override
  State<GlassTabBar> createState() => _GlassTabBarState();
}

class _GlassTabBarState extends State<GlassTabBar>
    with SingleTickerProviderStateMixin {
  static const capsulePad = GlassTabBar.capsulePad,
      itemW = GlassTabBar.itemW,
      itemH = GlassTabBar.itemH,
      barH = GlassTabBar.barH;

  /// How long a finger rests on the bar before the lens lifts, how far it must move sideways to count as a drag, and how
  /// far above or below the capsule a release cancels.
  static const _holdDelay = Duration(milliseconds: 120);
  static const double _dragSlop = 4, _cancelReach = 40;

  /// The lens's left edge, from the first slot's left edge. It is driven by hand: a spring toward a slot, or the finger.
  late final AnimationController _lensX = AnimationController.unbounded(
    vsync: this,
    value: _slotX(_slotOf(widget.index)),
  );
  double? _lensTarget;

  Timer? _hold;
  int? _pointer;
  Offset _downAt = Offset.zero;

  /// The slot a finger went down on, and the slot the lifted lens is over now.
  int _pressedSlot = 0, _previewSlot = 0;
  bool _lifted = false, _dragging = false;

  List<TabSpec> get tabs => widget.tabs;
  TabSpec get start => widget.start;
  int get index => widget.index;
  int get startIndex => widget.startIndex;
  ValueChanged<int> get onSelect => widget.onSelect;

  /// The tabs in the capsule, each with its index in the whole bar (Start is not one of them).
  List<(TabSpec, int)> get _slots => [
    for (var i = 0, n = 0; i < tabs.length + 1; i++)
      if (i != startIndex) (tabs[n++], i),
  ];

  /// The slot of a bar index, or null for Start, which has none.
  int? _slotOf(int i) {
    if (i == startIndex) return null;
    return i < startIndex ? i : i - 1;
  }

  double _slotX(int? slot) => (slot ?? 0) * itemW;

  bool get _ios => PressFeedback.dims(context);
  bool get _still => MediaQuery.disableAnimationsOf(context);

  /// Moves the lens to [x]: a spring on iOS, Material's standard easing on Android, a jump under reduced motion.
  void _slideTo(double x) {
    _lensTarget = x;
    if (_still) {
      _lensX.value = x;
    } else if (_ios) {
      _lensX.animateTo(
        x,
        duration: const Duration(milliseconds: 320),
        curve: Curves.easeOutBack,
      );
    } else {
      _lensX.animateTo(
        x,
        duration: const Duration(milliseconds: 260),
        curve: const Cubic(.2, 0, 0, 1),
      );
    }
  }

  @override
  void didUpdateWidget(GlassTabBar old) {
    super.didUpdateWidget(old);
    if (_pointer != null) return; // the release decides where the lens goes
    final slot = _slotOf(index);
    if (slot == null) return;
    final x = _slotX(slot);
    if (_slotOf(old.index) == null) {
      _lensX.value = x; // it was hidden: appear in place instead of sliding in from the old spot
      _lensTarget = x;
    } else if (_lensTarget != x) {
      _slideTo(x);
    }
  }

  @override
  void dispose() {
    _hold?.cancel();
    _lensX.dispose();
    super.dispose();
  }

  int _slotAt(double dx, int count) =>
      ((dx - capsulePad) ~/ itemW).clamp(0, count - 1);

  void _lift(int slot) {
    _hold?.cancel();
    final hidden = _slotOf(index) == null;
    setState(() {
      _lifted = true;
      _previewSlot = slot;
    });
    if (hidden) _lensX.value = _slotX(slot);
    _slideTo(_slotX(slot));
  }

  void _onDown(PointerDownEvent e, int count) {
    if (_pointer != null) return;
    _pointer = e.pointer;
    _downAt = e.localPosition;
    _pressedSlot = _previewSlot = _slotAt(e.localPosition.dx, count);
    _hold = Timer(_holdDelay, () {
      if (mounted && _pointer != null) _lift(_pressedSlot);
    });
  }

  void _onMove(PointerMoveEvent e, int count) {
    if (e.pointer != _pointer) return;
    if (!_dragging) {
      if ((e.localPosition.dx - _downAt.dx).abs() < _dragSlop) return;
      _dragging = true;
      if (!_lifted) _lift(_previewSlot);
    }
    final max = (count - 1) * itemW;
    final x = (e.localPosition.dx - capsulePad - itemW / 2).clamp(0.0, max);
    _lensTarget = null;
    _lensX.value = x;
    final slot = (x / itemW).round();
    if (slot != _previewSlot) {
      HapticFeedback.selectionClick();
      setState(() => _previewSlot = slot);
    }
  }

  void _onEnd(
    PointerEvent e,
    List<(TabSpec, int)> slots, {
    bool cancel = false,
  }) {
    if (e.pointer != _pointer) return;
    _pointer = null;
    _hold?.cancel();
    final wasLifted = _lifted;
    final dy = e.localPosition.dy;
    final out = cancel || dy < -_cancelReach || dy > barH + _cancelReach;
    final current = _slotOf(index);
    final slot = out
        ? current
        : (wasLifted || _dragging ? _previewSlot : _pressedSlot);
    _dragging = false;
    if (wasLifted) setState(() => _lifted = false);
    if (slot == null) {
      setState(() {}); // back to Start: the lens hides again
      return;
    }
    if (wasLifted || slot != current) _slideTo(_slotX(slot));
    if (slot != current) onSelect(slots[slot].$2);
  }

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final ios = _ios, still = _still;
    final slots = _slots;
    final curSlot = _slotOf(index);
    // While the lens is lifted, the tab under it is the one that looks selected.
    final activeSlot = _lifted ? _previewSlot : curSlot;
    final lensHidden = curSlot == null && !_lifted;

    Widget cell(TabSpec s, int slot) {
      final on = slot == activeSlot;
      return SizedBox(
        key: ValueKey('tab-pill-$slot'),
        width: itemW,
        height: itemH,
        child: Stack(
          alignment: Alignment.center,
          children: [
            AnimatedScale(
              scale: ios && _lifted && !still && on ? 1.14 : 1,
              duration: const Duration(milliseconds: 150),
              child: Icon(s.icon, size: 24, color: on ? t.accentInk : t.ink2),
            ),
            if (s.badge > 0)
              Positioned(
                top: 1,
                right: 4,
                child: Container(
                  constraints: const BoxConstraints(
                    minWidth: 16,
                    minHeight: 16,
                  ),
                  padding: const EdgeInsets.symmetric(horizontal: 4),
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: t.accentSolid,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Text(
                    '${s.badge}',
                    style: TextStyle(
                      fontSize: 10,
                      fontWeight: FontWeight.w700,
                      color: t.onAccent,
                    ),
                  ),
                ),
              ),
          ],
        ),
      );
    }

    // One lens under the selected tab. On iOS it is frosted glass that lifts while held; on Android it is Material's
    // tinted indicator.
    final lifted = ios && _lifted;
    final lensColor = !ios
        ? t.accentBg
        : t.isDark
        ? Color(lifted ? 0x33FFFFFF : 0x1AFFFFFF)
        : Color(lifted ? 0xF2FFFFFF : 0xB3FFFFFF);
    final lensShadows = !lifted
        ? const <BoxShadow>[]
        : [
            BoxShadow(
              color: t.isDark
                  ? const Color(0x80000000)
                  : const Color(0x33141828),
              blurRadius: 24,
              offset: const Offset(0, 10),
            ),
          ];
    final fade = still ? Duration.zero : const Duration(milliseconds: 200);
    final lens = AnimatedBuilder(
      animation: _lensX,
      builder: (context, child) => Positioned(
        left: capsulePad + _lensX.value,
        top: capsulePad,
        width: itemW,
        height: itemH,
        child: child!,
      ),
      child: IgnorePointer(
        child: AnimatedOpacity(
          opacity: lensHidden ? 0 : 1,
          duration: fade,
          child: AnimatedScale(
            key: const ValueKey('tab-lens'),
            scale: lifted && !still ? 1.15 : 1,
            duration: still ? Duration.zero : const Duration(milliseconds: 250),
            curve: Curves.easeOutBack,
            child: AnimatedContainer(
              duration: fade,
              decoration: BoxDecoration(
                color: lensColor,
                borderRadius: BorderRadius.circular(itemH / 2),
                boxShadow: lensShadows,
              ),
              child: AnimatedOpacity(
                opacity: lifted ? 1 : 0,
                duration: fade,
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(itemH / 2),
                    gradient: const LinearGradient(
                      begin: Alignment.topCenter,
                      end: Alignment.bottomCenter,
                      colors: [Color(0x8CFFFFFF), Color(0x00FFFFFF)],
                      stops: [0, .55],
                    ),
                  ),
                  child: const SizedBox.expand(),
                ),
              ),
            ),
          ),
        ),
      ),
    );

    // The touch target of an item: the lens's width and a full 44 pt in height, laid over the capsule, which is shorter.
    Widget target(TabSpec s, int slot, int i) => Semantics(
      button: true,
      selected: i == index,
      label: s.label,
      onTap: () => onSelect(i),
      excludeSemantics: true,
      child: ios
          ? const SizedBox(width: itemW, height: barH)
          : InkResponse(
              onTap: () => onSelect(i),
              radius: 30,
              child: const SizedBox(width: itemW, height: barH),
            ),
    );

    Widget targets() {
      final row = Padding(
        padding: const EdgeInsets.symmetric(horizontal: capsulePad),
        child: Row(
          children: [
            for (var n = 0; n < slots.length; n++)
              target(slots[n].$1, n, slots[n].$2),
          ],
        ),
      );
      // On iOS the capsule is one touch surface: a tap selects, a hold lifts the lens, a sideways drag carries it. The
      // listener only watches, so nothing else is held back from the gesture arena.
      return ios
          ? Listener(
              behavior: HitTestBehavior.opaque,
              onPointerDown: (e) => _onDown(e, slots.length),
              onPointerMove: (e) => _onMove(e, slots.length),
              onPointerUp: (e) => _onEnd(e, slots),
              onPointerCancel: (e) => _onEnd(e, slots, cancel: true),
              child: row,
            )
          : row;
    }

    return Padding(
      padding: EdgeInsets.fromLTRB(
        20,
        0,
        20,
        MediaQuery.viewPaddingOf(context).bottom +
            GlassTabBar.bottomGap(context),
      ),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          SizedBox(
            width: itemW * slots.length + capsulePad * 2,
            height: barH,
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                const Positioned.fill(
                  child: Glass(
                    key: ValueKey('tab-capsule'),
                    blur: true,
                    radius: (itemH + capsulePad * 2) / 2,
                    child: SizedBox.expand(),
                  ),
                ),
                lens,
                Positioned(
                  left: capsulePad,
                  top: capsulePad,
                  child: ExcludeSemantics(
                    child: Row(
                      children: [
                        for (var n = 0; n < slots.length; n++)
                          cell(slots[n].$1, n),
                      ],
                    ),
                  ),
                ),
                targets(),
              ],
            ),
          ),
          Semantics(
            button: true,
            selected: index == startIndex,
            label: start.label,
            excludeSemantics: true,
            child: PressFeedback(
              scale: .92,
              opacity: .85,
              child: InkResponse(
                onTap: () => onSelect(startIndex),
                // Solid accent like a primary button, with a glass sheen and a soft accent shadow; the icon is the
                // token's on-accent (light in light mode, dark in dark mode).
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: t.accentSolid,
                    boxShadow: [
                      BoxShadow(
                        color: t.accent.withValues(alpha: .30),
                        blurRadius: 16,
                        offset: const Offset(0, 6),
                      ),
                      const BoxShadow(
                        color: Color(0x14000000),
                        blurRadius: 2,
                        offset: Offset(0, 1),
                      ),
                    ],
                  ),
                  // A gradient replaces the colour in a BoxDecoration, so the sheen is its own layer over the solid fill.
                  child: DecoratedBox(
                    decoration: const BoxDecoration(
                      shape: BoxShape.circle,
                      gradient: LinearGradient(
                        begin: Alignment.topCenter,
                        end: Alignment.bottomCenter,
                        colors: [Color(0x38FFFFFF), Color(0x00FFFFFF)],
                        stops: [0, .55],
                      ),
                    ),
                    child: SizedBox(
                      width: barH,
                      height: barH,
                      child: Icon(start.icon, size: 26, color: t.onAccent),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
