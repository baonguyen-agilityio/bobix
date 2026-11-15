import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import 'system_screen.dart';
import 'widget_preview_data.dart';

const _cell = 68.0;
const _gap = 18.0;
const _fill = Color(0x24FFFFFF);

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
                    size: 22,
                    color: n > 0 ? t.accentSolid : Colors.white,
                  ),
                ),
                if (n > 0)
                  Positioned(
                    top: 2,
                    right: 2,
                    child: Container(
                      constraints: const BoxConstraints(
                        minWidth: 20,
                        minHeight: 20,
                      ),
                      alignment: Alignment.center,
                      padding: const EdgeInsets.symmetric(horizontal: 5),
                      decoration: BoxDecoration(
                        color: t.accentSolid,
                        borderRadius: BorderRadius.circular(10),
                      ),
                      child: Text(
                        '$n',
                        style: TextStyle(
                          fontSize: 11,
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
            padding: const EdgeInsets.symmetric(horizontal: 16),
            decoration: BoxDecoration(
              color: _fill,
              borderRadius: BorderRadius.circular(_cell / 2),
            ),
            child: Row(
              children: [
                const Icon(
                  Icons.play_arrow_outlined,
                  size: 20,
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
                          fontSize: 13,
                          fontWeight: FontWeight.w700,
                          color: Colors.white,
                        ),
                      ),
                      Text(
                        line,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontSize: 11,
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
  const _SystemTile();

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: Container(
      width: _cell,
      height: _cell,
      decoration: const BoxDecoration(
        color: Color(0x1AFFFFFF),
        shape: BoxShape.circle,
      ),
    ),
  );
}

/// The media block: two grey lines on a rounded tile.
class _MediaBlock extends StatelessWidget {
  const _MediaBlock();

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: Container(
      width: _cell * 2 + _gap,
      height: _cell * 2 + 12,
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: _fill,
        borderRadius: BorderRadius.circular(28),
      ),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [const _Bar(1), const SizedBox(height: 8), const _Bar(.6)],
      ),
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
      height: _cell * 2 + 12,
      decoration: BoxDecoration(
        color: _fill,
        borderRadius: BorderRadius.circular(24),
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

/// Settings > Widgets > Control Center: the Bobix control and the Start control among neutral system controls, on the
/// phone's darkened wallpaper, laid out as the design draws it: four small tiles and the media block, the Bobix and Start
/// controls, then a tile and two tall sliders.
class ControlCenterScreen extends ConsumerWidget {
  const ControlCenterScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    const small = SizedBox(width: 12);
    const smallV = SizedBox(height: 12);
    return SystemScreen(
      title: s('phone.wgs.cc'),
      top: 46,
      overlay: const Color(0x8C0A0A10),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 24),
        child: Column(
          children: [
            SizedBox(
              width: _cell * 4 + _gap * 3,
              child: const Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      SizedBox(
                        width: _cell * 2 + 12,
                        child: Column(
                          children: [
                            Row(
                              children: [_SystemTile(), small, _SystemTile()],
                            ),
                            smallV,
                            Row(
                              children: [_SystemTile(), small, _SystemTile()],
                            ),
                          ],
                        ),
                      ),
                      SizedBox(width: _gap + 6),
                      _MediaBlock(),
                    ],
                  ),
                  SizedBox(height: _gap),
                  Row(
                    children: [
                      BobixControl(),
                      SizedBox(width: _gap),
                      StartControl(),
                      SizedBox(width: _gap),
                      _SystemTile(),
                    ],
                  ),
                  SizedBox(height: _gap),
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _SystemTile(),
                      SizedBox(width: _gap),
                      _Slider(.62),
                      SizedBox(width: _gap),
                      _Slider(.4),
                    ],
                  ),
                ],
              ),
            ),
            const Spacer(),
            Text(
              s('phone.wgs.cc.n'),
              textAlign: TextAlign.center,
              style: const TextStyle(fontSize: 12, color: Color(0xB3FFFFFF)),
            ),
          ],
        ),
      ),
    );
  }
}

class _Bar extends StatelessWidget {
  const _Bar(this.fraction);
  final double fraction;

  @override
  Widget build(BuildContext context) => FractionallySizedBox(
    widthFactor: fraction,
    child: Container(
      height: 8,
      decoration: BoxDecoration(
        color: const Color(0x59FFFFFF),
        borderRadius: BorderRadius.circular(4),
      ),
    ),
  );
}
