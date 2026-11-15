import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../data/models.dart';
import '../../l10n/l10n.dart';
import '../../theme/tokens.dart';
import 'system_screen.dart';
import 'widget_preview_data.dart';

const _saturate40 = ColorFilter.matrix(<double>[
  .5, .4, .1, 0, 0, //
  .1, .8, .1, 0, 0,
  .1, .4, .5, 0, 0,
  0, 0, 0, 1, 0,
]);

/// The card of a Home Screen widget: rounded, light or dark, with the offline look (dimmed, washed out) when [off].
class _WidgetCard extends StatelessWidget {
  const _WidgetCard({
    required this.width,
    required this.height,
    required this.child,
    this.off = false,
  });

  final double width, height;
  final bool off;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final body = Padding(padding: const EdgeInsets.all(14), child: child);
    return LayoutBuilder(
      builder: (context, box) => Container(
        width: box.maxWidth < width ? box.maxWidth : width,
        height: height,
        decoration: BoxDecoration(
          color: t.isDark
              ? t.surface2.withValues(alpha: .94)
              : const Color(0xEBFFFFFF),
          borderRadius: BorderRadius.circular(22),
          border: Border.all(
            width: .5,
            color: t.isDark ? const Color(0x17FFFFFF) : const Color(0xE6FFFFFF),
          ),
          boxShadow: const [
            BoxShadow(
              color: Color(0x1F000000),
              blurRadius: 16,
              offset: Offset(0, 4),
            ),
          ],
        ),
        child: ClipRRect(
          borderRadius: BorderRadius.circular(22),
          child: off
              ? ColorFiltered(
                  colorFilter: _saturate40,
                  child: Opacity(opacity: .55, child: body),
                )
              : body,
        ),
      ),
    );
  }
}

/// "Bobix" with its paw, and on the right the computer's name (or, offline, when the widget was last up to date).
class _Head extends ConsumerWidget {
  const _Head({required this.data, this.short = false});
  final WidgetData data;

  /// Only the clock of the last update, and no computer name: for the small and medium widgets.
  final bool short;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final small = TextStyle(
      fontSize: 11,
      color: t.ink3,
      fontWeight: FontWeight.w500,
    );
    final String? status = data.off
        ? (data.updated.isEmpty
              ? s('phone.wg.offline')
              : (short
                    ? data.updated
                    : s('phone.wg.updated', {'tm': data.updated})))
        : (short || data.computer.isEmpty ? null : data.computer);
    return Row(
      children: [
        Icon(Icons.pets_outlined, size: 13, color: t.accentInk),
        const SizedBox(width: 4),
        Text(
          'Bobix',
          style: TextStyle(
            fontSize: 12,
            fontWeight: FontWeight.w700,
            color: t.accentInk,
          ),
        ),
        const Spacer(),
        if (status != null) ...[
          if (!data.off) ...[
            Container(
              width: 6,
              height: 6,
              decoration: BoxDecoration(color: t.ok, shape: BoxShape.circle),
            ),
            const SizedBox(width: 5),
          ],
          Flexible(
            child: Text(
              status,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: small,
            ),
          ),
        ],
      ],
    );
  }
}

class _Big extends ConsumerWidget {
  const _Big(this.n);
  final int n;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          '$n',
          style: TextStyle(
            fontSize: 44,
            height: 1,
            fontWeight: FontWeight.w700,
            color: t.ink,
          ),
        ),
        Text(
          s('phone.wgs.need'),
          style: TextStyle(
            fontSize: 13,
            fontWeight: FontWeight.w700,
            color: t.accentInk,
          ),
        ),
      ],
    );
  }
}

class _Empty extends ConsumerWidget {
  const _Empty();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.check, size: 22, color: t.ok),
          const SizedBox(height: 4),
          Text(
            s('phone.home.none'),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 13,
              fontWeight: FontWeight.w600,
              color: t.ink2,
            ),
          ),
        ],
      ),
    );
  }
}

class _PetDot extends StatelessWidget {
  const _PetDot(this.x);
  final Session x;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return SizedBox.square(
      dimension: 30,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          previewPet(x, 26),
          Positioned(
            right: 0,
            bottom: 0,
            child: Container(
              width: 9,
              height: 9,
              decoration: BoxDecoration(
                color: petDotColor(x),
                shape: BoxShape.circle,
                border: Border.all(
                  width: 1.5,
                  color: t.isDark ? t.surface2 : Colors.white,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _PetStrip extends StatelessWidget {
  const _PetStrip(this.pets, {this.limit = 4});
  final List<Session> pets;
  final int limit;

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: 4,
    children: [for (final x in pets.take(limit)) _PetDot(x)],
  );
}

/// A tappable row of a widget: the pet, the name over a line, a chevron. It opens the session.
class _NeedRow extends StatelessWidget {
  const _NeedRow({required this.x, required this.line, required this.label});
  final Session x;
  final String line, label;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      button: true,
      label: label,
      excludeSemantics: true,
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: () => openFromWidget(context, x),
        child: Container(
          constraints: const BoxConstraints(minHeight: 44),
          padding: const EdgeInsets.fromLTRB(6, 4, 8, 4),
          decoration: BoxDecoration(
            color: t.isDark ? t.surface3 : const Color(0x0F1A1F3A),
            borderRadius: BorderRadius.circular(12),
          ),
          child: Row(
            children: [
              previewPet(x, 30),
              const SizedBox(width: 8),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      x.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w700,
                        color: t.ink,
                      ),
                    ),
                    Text(
                      line,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 11, color: t.ink3),
                    ),
                  ],
                ),
              ),
              Icon(Icons.chevron_right, size: 16, color: t.ink3),
            ],
          ),
        ),
      ),
    );
  }
}

class _SectionLabel extends StatelessWidget {
  const _SectionLabel(this.text, [this.count]);
  final String text;
  final int? count;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Padding(
      padding: const EdgeInsets.only(top: 8, bottom: 4),
      child: Text.rich(
        TextSpan(
          text: text.toUpperCase(),
          children: [
            if (count != null)
              TextSpan(
                text: '  $count',
                style: TextStyle(color: t.accentInk),
              ),
          ],
        ),
        style: TextStyle(
          fontSize: 11,
          fontWeight: FontWeight.w700,
          letterSpacing: .3,
          color: t.ink3,
        ),
      ),
    );
  }
}

/// Small widget: how many wait, the first one's pet and name. 170 x 170.
class SmallHomeWidget extends ConsumerWidget {
  const SmallHomeWidget({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final d = ref.watch(widgetDataProvider);
    final x = d.need.isEmpty ? null : d.need.first;
    return Semantics(
      button: true,
      container: true,
      label: x == null
          ? 'Bobix: ${s('phone.home.none')}'
          : 'Bobix: ${d.need.length} ${s('phone.wgs.need')}, ${x.name}',
      excludeSemantics: true,
      child: GestureDetector(
        onTap: () => openFromWidget(context, x),
        child: _WidgetCard(
          width: 170,
          height: 170,
          off: d.off,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _Head(data: d, short: true),
              if (x == null)
                const Expanded(child: _Empty())
              else ...[
                const SizedBox(height: 6),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(child: _Big(d.need.length)),
                    previewPet(x, 44),
                  ],
                ),
                const Spacer(),
                Text(
                  x.name,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 13,
                    height: 1.25,
                    fontWeight: FontWeight.w700,
                    color: t.ink,
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// Medium widget: the count and the pets on the left, up to two waiting sessions on the right. 364 x 170.
class MediumHomeWidget extends ConsumerWidget {
  const MediumHomeWidget({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final d = ref.watch(widgetDataProvider);
    return _WidgetCard(
      width: 364,
      height: 170,
      off: d.off,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(
            flex: 5,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _Head(data: d, short: true),
                const SizedBox(height: 6),
                _Big(d.need.length),
                const Spacer(),
                _PetStrip(d.pets),
              ],
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            flex: 7,
            child: d.need.isEmpty
                ? const _Empty()
                : Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      for (final x in d.need.take(2)) ...[
                        _NeedRow(
                          x: x,
                          label:
                              '${x.name}, ${x.project}, ${sessionLine(s, x)}',
                          line: '${x.project} · ${sessionLine(s, x)}',
                        ),
                        const SizedBox(height: 6),
                      ],
                    ],
                  ),
          ),
        ],
      ),
    );
  }
}

/// Large widget: what needs you, the pets and the tickets to start. 364 x 382.
class LargeHomeWidget extends ConsumerWidget {
  const LargeHomeWidget({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final d = ref.watch(widgetDataProvider);
    String minutes(int n) => s('phone.min', {'n': n});
    return _WidgetCard(
      width: 364,
      height: 382,
      off: d.off,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _Head(data: d),
          _SectionLabel(s('phone.home.need'), d.need.length),
          if (d.need.isEmpty)
            const SizedBox(height: 56, child: _Empty())
          else
            for (final x in d.need.take(2)) ...[
              _NeedRow(
                x: x,
                label: '${x.name}, ${x.project}, ${sessionLine(s, x)}',
                line:
                    '${x.project} · ${sessionLine(s, x)} · ${minutes(x.waitingMinutes)}',
              ),
              const SizedBox(height: 4),
            ],
          _SectionLabel(s('phone.tab.pets')),
          Align(
            alignment: Alignment.centerLeft,
            child: _PetStrip(d.pets, limit: 6),
          ),
          _SectionLabel(s('phone.wgs.todo'), d.todo.length),
          for (final x in d.todo.take(3))
            Container(
              constraints: const BoxConstraints(minHeight: 44),
              margin: const EdgeInsets.only(bottom: 4),
              padding: const EdgeInsets.fromLTRB(6, 4, 6, 4),
              decoration: BoxDecoration(
                color: t.isDark ? t.surface3 : const Color(0x0F1A1F3A),
                borderRadius: BorderRadius.circular(12),
              ),
              child: Row(
                children: [
                  Container(
                    width: 28,
                    height: 28,
                    decoration: BoxDecoration(
                      color: t.surface3,
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: Icon(
                      Icons.confirmation_number_outlined,
                      size: 15,
                      color: t.ink2,
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          x.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w700,
                            color: t.ink,
                          ),
                        ),
                        Text(
                          x.ref,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 11, color: t.ink3),
                        ),
                      ],
                    ),
                  ),
                  Container(
                    width: 30,
                    height: 30,
                    decoration: BoxDecoration(
                      color: t.accentSolid,
                      shape: BoxShape.circle,
                    ),
                    child: Icon(
                      Icons.play_arrow_rounded,
                      size: 18,
                      color: t.onAccent,
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// A square app icon of the simulated home screen: a flat colour, no name of a real app.
class _AppIcon extends StatelessWidget {
  const _AppIcon(this.color);
  final Color color;

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: Container(
      width: 62,
      height: 62,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(16),
      ),
    ),
  );
}

/// Settings > Widgets > Home Screen: a phone home screen with a medium, a small and a large widget in place, and the app's
/// own icon in the dock.
class HomeScreenPreviewScreen extends ConsumerWidget {
  const HomeScreenPreviewScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    const colors = [
      Color(0xFF5B9BF0),
      Color(0xFF6FCF97),
      Color(0xFFF2C94C),
      Color(0xFFEB5770),
    ];
    return SystemScreen(
      title: s('phone.wgs.hs'),
      child: ListView(
        padding: const EdgeInsets.fromLTRB(0, 8, 0, 16),
        children: [
          const Center(child: MediumHomeWidget()),
          const SizedBox(height: 16),
          Center(
            child: SizedBox(
              width: 364,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const SmallHomeWidget(),
                  const Spacer(),
                  SizedBox(
                    width: 164,
                    child: Wrap(
                      spacing: 40,
                      runSpacing: 26,
                      children: [for (final c in colors) _AppIcon(c)],
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
          const Center(child: LargeHomeWidget()),
          const SizedBox(height: 16),
          Center(
            child: Container(
              width: 364,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: const Color(0x33FFFFFF),
                borderRadius: BorderRadius.circular(32),
              ),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Semantics(
                    button: true,
                    label: 'Bobix',
                    excludeSemantics: true,
                    child: InkWell(
                      borderRadius: BorderRadius.circular(16),
                      onTap: () => openFromWidget(context, null),
                      child: Container(
                        width: 62,
                        height: 62,
                        decoration: BoxDecoration(
                          color: context.tokens.accentSolid,
                          borderRadius: BorderRadius.circular(16),
                        ),
                        child: Icon(
                          Icons.pets_outlined,
                          size: 28,
                          color: context.tokens.onAccent,
                        ),
                      ),
                    ),
                  ),
                  for (final c in colors.skip(1))
                    _AppIcon(c.withValues(alpha: .8)),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
