import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../data/models.dart';
import '../../l10n/l10n.dart';
import '../../theme/theme.dart' show minTouch;
import '../../theme/tokens.dart';
import 'system_screen.dart';
import 'widget_preview_data.dart';

const _white = Colors.white;
const _white70 = Color(0xB3FFFFFF);
const _glass = Color(0x2EFFFFFF);

/// The one-line Lock Screen widget: the count and the computer.
class LockInlineWidget extends ConsumerWidget {
  const LockInlineWidget({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final d = ref.watch(widgetDataProvider);
    final text = d.need.isEmpty
        ? s('phone.home.none')
        : '${d.need.length} ${s('phone.wgs.need')}';
    final line = d.computer.isEmpty ? text : '$text · ${d.computer}';
    return Opacity(
      opacity: d.off ? .55 : 1,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.pets_outlined, size: 13, color: _white),
          const SizedBox(width: 4),
          Flexible(
            child: Text(
              line,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w600,
                color: _white,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _RingPainter extends CustomPainter {
  const _RingPainter(this.fraction);
  final double fraction;

  @override
  void paint(Canvas canvas, Size size) {
    final r = Rect.fromLTWH(0, 0, size.width, size.height).deflate(4);
    final base = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 5
      ..color = const Color(0x4DFFFFFF);
    canvas.drawArc(r, 0, math.pi * 2, false, base);
    canvas.drawArc(
      r,
      -math.pi / 2,
      math.pi * 2 * fraction,
      false,
      base
        ..color = _white
        ..strokeCap = StrokeCap.round,
    );
  }

  @override
  bool shouldRepaint(_RingPainter old) => old.fraction != fraction;
}

/// The circular Lock Screen widget: a ring that fills with the sessions that wait (five fill it), their number inside. 72 x 72.
class LockCircleWidget extends ConsumerWidget {
  const LockCircleWidget({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final d = ref.watch(widgetDataProvider);
    final n = d.need.length;
    return Semantics(
      label: 'Bobix: $n ${s('phone.wgs.need')}',
      excludeSemantics: true,
      child: Opacity(
        opacity: d.off ? .55 : 1,
        child: SizedBox.square(
          dimension: 72,
          child: CustomPaint(
            painter: _RingPainter(math.min(1, n / 5)),
            child: Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    '$n',
                    style: const TextStyle(
                      fontSize: 24,
                      height: 1.05,
                      fontWeight: FontWeight.w600,
                      color: _white,
                    ),
                  ),
                  const Icon(Icons.pets_outlined, size: 11, color: _white70),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// The rectangular Lock Screen widget: the count, the first session's name, and how long it waits. 160 x 72.
class LockRectWidget extends ConsumerWidget {
  const LockRectWidget({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final d = ref.watch(widgetDataProvider);
    final x = d.need.isEmpty ? null : d.need.first;
    return Opacity(
      opacity: d.off ? .55 : 1,
      child: Container(
        width: 160,
        height: 72,
        padding: const EdgeInsets.fromLTRB(12, 8, 10, 8),
        decoration: BoxDecoration(
          color: _glass,
          borderRadius: BorderRadius.circular(18),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Row(
              children: [
                const Icon(Icons.pets_outlined, size: 12, color: _white),
                const SizedBox(width: 4),
                Flexible(
                  child: Text(
                    x == null
                        ? 'Bobix'
                        : '${d.need.length} ${s('phone.wgs.need')}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      color: _white,
                    ),
                  ),
                ),
              ],
            ),
            Text(
              x?.name ?? s('phone.home.none'),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w700,
                color: _white,
              ),
            ),
            Text(
              x == null
                  ? d.computer
                  : '${x.project} · ${s('phone.min', {'n': x.waitingMinutes})}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontSize: 11, color: _white70),
            ),
          ],
        ),
      ),
    );
  }
}

/// The "Open to review" capsule of the Live Activity and the island: it opens the app, never answers.
class _OpenButton extends ConsumerWidget {
  const _OpenButton(this.x);
  final Session x;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final label = s('phone.wg.open');
    return Semantics(
      button: true,
      label: label,
      excludeSemantics: true,
      child: InkWell(
        borderRadius: BorderRadius.circular(22),
        onTap: () => openFromWidget(context, x),
        child: Container(
          constraints: const BoxConstraints(minHeight: 44),
          padding: const EdgeInsets.symmetric(horizontal: 14),
          decoration: BoxDecoration(
            color: t.accentSolid,
            borderRadius: BorderRadius.circular(22),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.lock_outline, size: 13, color: t.onAccent),
              const SizedBox(width: 6),
              Text(
                label,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w700,
                  color: t.onAccent,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// What the Live Activity says about nothing to show: the session it follows is missing.
class _NoSession extends ConsumerWidget {
  const _NoSession();

  @override
  Widget build(BuildContext context, WidgetRef ref) => Text(
    ref.watch(stringsProvider)('phone.home.none'),
    style: const TextStyle(
      fontSize: 13,
      fontWeight: FontWeight.w600,
      color: _white70,
    ),
  );
}

/// The Live Activity on the Lock Screen: the session it follows, with "Open to review" while it waits.
class LiveActivityCard extends ConsumerWidget {
  const LiveActivityCard({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final d = ref.watch(widgetDataProvider);
    final x = d.live;
    final wait = d.need.isNotEmpty;
    return Opacity(
      opacity: d.off ? .55 : 1,
      child: Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: const Color(0x73141428),
          borderRadius: BorderRadius.circular(26),
        ),
        child: x == null
            ? const Center(child: _NoSession())
            : Row(
                children: [
                  previewPet(x, 44),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Row(
                          children: [
                            Expanded(
                              child: Text(
                                x.project.toUpperCase(),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 11,
                                  fontWeight: FontWeight.w700,
                                  letterSpacing: .4,
                                  color: t.accent,
                                ),
                              ),
                            ),
                            if (wait)
                              Text(
                                s('phone.min', {
                                  'n': x.waitingMinutes,
                                }).toUpperCase(),
                                style: const TextStyle(
                                  fontSize: 11,
                                  fontWeight: FontWeight.w700,
                                  color: _white70,
                                ),
                              ),
                          ],
                        ),
                        Text(
                          x.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontSize: 15,
                            fontWeight: FontWeight.w700,
                            color: _white,
                          ),
                        ),
                        Text(
                          wait
                              ? '${sessionLine(s, x)} · ${s('phone.min', {'n': x.waitingMinutes})}'
                              : sessionLine(s, x),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(fontSize: 12, color: _white70),
                        ),
                      ],
                    ),
                  ),
                  if (wait) ...[const SizedBox(width: 8), _OpenButton(x)],
                ],
              ),
      ),
    );
  }
}

const _island = Color(0xFF000000);

enum IslandKind { compact, minimal, expanded }

/// The Dynamic Island: compact (the pet and the count), minimal (the pet), or expanded (the session and the button).
class IslandPreview extends ConsumerWidget {
  const IslandPreview(this.kind, {super.key, this.onTap});
  final IslandKind kind;

  /// Compact only: a tap expands it.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final d = ref.watch(widgetDataProvider);
    final x = d.live;
    final n = d.need.length;
    final body = switch (kind) {
      IslandKind.minimal => Container(
        width: 36,
        height: 36,
        decoration: const BoxDecoration(color: _island, shape: BoxShape.circle),
        child: Center(child: x == null ? null : previewPet(x, 24)),
      ),
      IslandKind.compact => Container(
        width: 180,
        height: 36,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(
          color: _island,
          borderRadius: BorderRadius.circular(18),
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            if (x != null) previewPet(x, 24) else const SizedBox(width: 24),
            n > 0
                ? Text(
                    '$n',
                    style: TextStyle(
                      fontSize: 15,
                      fontWeight: FontWeight.w700,
                      color: t.accent,
                    ),
                  )
                : Icon(Icons.check, size: 14, color: t.ok),
          ],
        ),
      ),
      IslandKind.expanded => Container(
        width: 364,
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: _island,
          borderRadius: BorderRadius.circular(36),
        ),
        child: x == null
            ? const Center(child: _NoSession())
            : Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      previewPet(x, 44),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              x.name,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                fontSize: 15,
                                fontWeight: FontWeight.w700,
                                color: _white,
                              ),
                            ),
                            Text(
                              '${x.project} · ${sessionLine(s, x)}',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                fontSize: 12,
                                color: _white70,
                              ),
                            ),
                          ],
                        ),
                      ),
                      Text(
                        '$n',
                        style: TextStyle(
                          fontSize: 22,
                          fontWeight: FontWeight.w700,
                          color: t.accent,
                        ),
                      ),
                    ],
                  ),
                  if (n > 0) ...[
                    const SizedBox(height: 10),
                    Align(
                      alignment: Alignment.centerLeft,
                      child: _OpenButton(x),
                    ),
                  ],
                ],
              ),
      ),
    };
    if (kind == IslandKind.compact && onTap != null) {
      return Semantics(
        button: true,
        label: 'Bobix',
        excludeSemantics: true,
        child: InkWell(
          borderRadius: BorderRadius.circular(18),
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: minTouch),
            child: Center(child: body),
          ),
        ),
      );
    }
    return Opacity(opacity: d.off ? .55 : 1, child: body);
  }
}

/// Settings > Widgets > Lock Screen: the island over a lock screen with the date line, the clock, the circular and rectangular
/// widgets and the Live Activity. A tap on the island expands it.
class LockWidgetsScreen extends ConsumerStatefulWidget {
  const LockWidgetsScreen({super.key});

  @override
  ConsumerState<LockWidgetsScreen> createState() => _LockWidgetsScreenState();
}

class _LockWidgetsScreenState extends ConsumerState<LockWidgetsScreen> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    return SystemScreen(
      title: s('phone.wgs.lk'),
      top: 46,
      topOverlay: _expanded
          ? GestureDetector(
              onTap: () => setState(() => _expanded = false),
              child: const IslandPreview(IslandKind.expanded),
            )
          : IslandPreview(
              IslandKind.compact,
              onTap: () => setState(() => _expanded = true),
            ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14),
        child: Column(
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(
                  '${s('phone.wgs.date')} ·',
                  style: const TextStyle(
                    fontSize: 17,
                    fontWeight: FontWeight.w600,
                    color: Color(0xEBFFFFFF),
                  ),
                ),
                const SizedBox(width: 8),
                const Flexible(child: LockInlineWidget()),
              ],
            ),
            const SizedBox(height: 10),
            const Text(
              previewClock,
              style: TextStyle(
                fontSize: 76,
                height: 1,
                letterSpacing: -1.5,
                fontWeight: FontWeight.w300,
                color: _white,
              ),
            ),
            const SizedBox(height: 14),
            const Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                LockCircleWidget(),
                SizedBox(width: 10),
                LockRectWidget(),
              ],
            ),
            const Spacer(),
            const LiveActivityCard(),
            const SizedBox(height: 10),
            Text(
              s('phone.wgs.di.tap'),
              textAlign: TextAlign.center,
              style: const TextStyle(fontSize: 12, color: _white70),
            ),
            const SizedBox(height: 4),
          ],
        ),
      ),
    );
  }
}
