import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import 'system_screen.dart';
import 'widget_preview_data.dart';

const _cell = 68.0;
const _gap = 12.0;
const _fill = Color(0x2EFFFFFF);

/// The Bobix control: a round button with the paw and, while sessions wait, their number. A tap opens the first.
class BobixControl extends ConsumerWidget {
  const BobixControl({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final d = ref.watch(widgetDataProvider);
    final n = d.need.length;
    return Semantics(
      button: true,
      label: 'Bobix: $n ${s('phone.wgs.need')}',
      excludeSemantics: true,
      child: GestureDetector(
        onTap: () => openFromWidget(context, n > 0 ? d.need.first : null),
        child: Opacity(
          opacity: d.off ? .55 : 1,
          child: SizedBox.square(
            dimension: _cell,
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                Container(
                  width: _cell,
                  height: _cell,
                  decoration: BoxDecoration(
                    color: n > 0 ? Colors.white : _fill,
                    shape: BoxShape.circle,
                  ),
                  child: Icon(
                    Icons.pets_outlined,
                    size: 24,
                    color: n > 0 ? t.accentSolid : Colors.white,
                  ),
                ),
                if (n > 0)
                  Positioned(
                    top: -2,
                    right: -2,
                    child: Container(
                      constraints: const BoxConstraints(
                        minWidth: 22,
                        minHeight: 22,
                      ),
                      alignment: Alignment.center,
                      padding: const EdgeInsets.symmetric(horizontal: 5),
                      decoration: BoxDecoration(
                        color: t.accentSolid,
                        borderRadius: BorderRadius.circular(11),
                      ),
                      child: Text(
                        '$n',
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                          color: t.onAccent,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The Start control: a wide button with the number of tickets that wait to be started. A tap opens the app.
class StartControl extends ConsumerWidget {
  const StartControl({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final d = ref.watch(widgetDataProvider);
    final line = '${d.todo.length} ${s('phone.wgs.tickets')}';
    return Semantics(
      button: true,
      label: '${s('phone.wg.control.start')}, $line',
      excludeSemantics: true,
      child: GestureDetector(
        onTap: () => openFromWidget(context, null),
        child: Opacity(
          opacity: d.off ? .55 : 1,
          child: Container(
            width: _cell * 2 + _gap,
            height: _cell,
            padding: const EdgeInsets.symmetric(horizontal: 18),
            decoration: BoxDecoration(
              color: _fill,
              borderRadius: BorderRadius.circular(_cell / 2),
            ),
            child: Row(
              children: [
                const Icon(
                  Icons.play_arrow_outlined,
                  size: 22,
                  color: Colors.white,
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        s('phone.wg.control.start'),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontSize: 14,
                          fontWeight: FontWeight.w700,
                          color: Colors.white,
                        ),
                      ),
                      Text(
                        line,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontSize: 12,
                          color: Color(0xB3FFFFFF),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// A control of the system itself, drawn as a neutral placeholder: Bobix draws nothing of the phone's own.
class _SystemTile extends StatelessWidget {
  const _SystemTile({
    this.width = _cell,
    this.height = _cell,
    this.radius = _cell / 2,
    this.child,
  });
  final double width, height, radius;
  final Widget? child;

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: Container(
      width: width,
      height: height,
      decoration: BoxDecoration(
        color: const Color(0x26FFFFFF),
        borderRadius: BorderRadius.circular(radius),
      ),
      child: child,
    ),
  );
}

class _Slider extends StatelessWidget {
  const _Slider(this.fraction);
  final double fraction;

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: Container(
      width: _cell,
      height: _cell * 2 + _gap,
      decoration: BoxDecoration(
        color: const Color(0x33FFFFFF),
        borderRadius: BorderRadius.circular(_cell / 2),
      ),
      clipBehavior: Clip.antiAlias,
      alignment: Alignment.bottomCenter,
      child: FractionallySizedBox(
        heightFactor: fraction,
        widthFactor: 1,
        child: const ColoredBox(color: Colors.white),
      ),
    ),
  );
}

/// The in-between of two cells of the grid.
const _h = SizedBox(width: _gap);
const _v = SizedBox(height: _gap);

/// Settings > Widgets > Control Center: the Bobix control and the Start control among neutral system controls.
class ControlCenterScreen extends ConsumerWidget {
  const ControlCenterScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return SystemScreen(
      title: s('phone.wgs.cc'),
      colors: const [Color(0xFF1E2140), Color(0xFF47305A), Color(0xFF603B3B)],
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
        children: [
          Center(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Column(
                      children: [
                        Row(
                          children: [
                            const _SystemTile(),
                            _h,
                            const _SystemTile(),
                          ],
                        ),
                        _v,
                        Row(
                          children: [
                            const _SystemTile(),
                            _h,
                            const _SystemTile(),
                          ],
                        ),
                      ],
                    ),
                    _h,
                    _SystemTile(
                      width: _cell * 2 + _gap,
                      height: _cell * 2 + _gap,
                      radius: 28,
                      child: const Padding(
                        padding: EdgeInsets.all(16),
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [_Bar(120), SizedBox(height: 8), _Bar(70)],
                        ),
                      ),
                    ),
                  ],
                ),
                _v,
                const Row(
                  children: [
                    BobixControl(),
                    _h,
                    StartControl(),
                    _h,
                    _SystemTile(),
                  ],
                ),
                _v,
                const Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [_SystemTile(), _h, _Slider(.62), _h, _Slider(.4)],
                ),
              ],
            ),
          ),
          const SizedBox(height: 28),
          Text(
            s('phone.wgs.cc.n'),
            textAlign: TextAlign.center,
            style: const TextStyle(fontSize: 13, color: Color(0xB3FFFFFF)),
          ),
        ],
      ),
    );
  }
}

class _Bar extends StatelessWidget {
  const _Bar(this.width);
  final double width;

  @override
  Widget build(BuildContext context) => Container(
    width: width,
    height: 8,
    decoration: BoxDecoration(
      color: const Color(0x66FFFFFF),
      borderRadius: BorderRadius.circular(4),
    ),
  );
}
