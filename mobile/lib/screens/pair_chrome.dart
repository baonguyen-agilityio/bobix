import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../features/actions/action_widgets.dart' show HeaderPill;
import '../l10n/l10n.dart';
import '../theme/tokens.dart';
import '../widgets/glass.dart';

/// The shield with a check that stands for "no Always allow" and for the small print about what stays safe: one icon
/// for the pair-name row, the permissions rows and every [ShieldNote].
const shieldCheckIcon = Icons.verified_user_outlined;

/// The frame of every pairing page and of the pages that end a pairing (disconnected, pair again): a centred title with
/// a "Back" pill when [onBack] is set, step dots under it, a body that scrolls, and the actions in a glass bar at the
/// bottom.
class PairPage extends ConsumerWidget {
  const PairPage({
    super.key,
    required this.children,
    this.actions,
    this.onBack,
    this.step,
    this.showTitle = true,
  });

  final List<Widget> children;
  final Widget? actions;
  final VoidCallback? onBack;

  /// Which of the three steps this page is (1 to 3), or null when the page is not one of them.
  final int? step;
  final bool showTitle;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            ConstrainedBox(
              constraints: const BoxConstraints(minHeight: 52),
              child: Stack(
                alignment: Alignment.center,
                children: [
                  if (showTitle)
                    Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 100),
                      child: Semantics(
                        header: true,
                        child: Text(
                          s('phone.pair.title'),
                          textAlign: TextAlign.center,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 17,
                            height: 1.15,
                            fontWeight: FontWeight.w600,
                            color: t.ink,
                          ),
                        ),
                      ),
                    ),
                  if (onBack != null)
                    Positioned(
                      left: 8,
                      top: 0,
                      bottom: 0,
                      child: Align(
                        alignment: Alignment.centerLeft,
                        child: HeaderPill(
                          label: s('phone.a.back'),
                          icon: Icons.chevron_left,
                          onTap: onBack!,
                        ),
                      ),
                    ),
                ],
              ),
            ),
            if (step != null) StepDots(step: step!),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 20),
                children: children,
              ),
            ),
            if (actions != null) PairActionBar(child: actions!),
          ],
        ),
      ),
    );
  }
}

/// The actions at the bottom of a pairing page. It has no fill and no edge, so the buttons seem to rest on the page
/// itself; the list above ends where the bar begins, so nothing scrolls under it.
class PairActionBar extends StatelessWidget {
  const PairActionBar({super.key, required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) =>
      Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 12), child: child);
}

/// An icon that sits beside wrapped text: it is centred on the text's first line, not on the whole block. [style] is
/// the text's style (it must set `fontSize` and `height`); the icon's box is as tall as one line at the user's text size.
class FirstLineIcon extends StatelessWidget {
  const FirstLineIcon({
    super.key,
    required this.icon,
    required this.size,
    required this.style,
  });
  final Widget icon;

  /// The icon's width and height.
  final double size;
  final TextStyle style;

  /// The height of one line of [style] at the text scale in [context].
  static double lineHeight(BuildContext context, TextStyle style) =>
      MediaQuery.textScalerOf(context).scale(style.fontSize!) *
      (style.height ?? 1.2);

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: SizedBox(
      width: size,
      height: lineHeight(context, style),
      child: OverflowBox(
        maxWidth: size,
        maxHeight: double.infinity,
        child: icon,
      ),
    ),
  );
}

/// Three dots: the step the person is on is a long accent pill, the steps behind it are accent, the ones ahead are grey.
class StepDots extends StatelessWidget {
  const StepDots({super.key, required this.step});
  final int step;

  static const count = 3;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return ExcludeSemantics(
      child: Padding(
        padding: const EdgeInsets.only(top: 2, bottom: 14),
        child: Row(
          key: const Key('pair-step-dots'),
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            for (var i = 1; i <= count; i++)
              Container(
                key: Key('pair-dot-$i'),
                margin: const EdgeInsets.symmetric(horizontal: 3),
                width: i == step ? 26 : 8,
                height: 8,
                decoration: BoxDecoration(
                  color: i == step
                      ? t.accent
                      : (i < step ? t.accent.withValues(alpha: .4) : t.n300),
                  borderRadius: BorderRadius.circular(4),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// How a result is told: a round icon, a heading and a line under it.
enum HeroTone { ok, warn, err, info }

class PairHero extends StatelessWidget {
  const PairHero({
    super.key,
    required this.icon,
    required this.tone,
    required this.title,
    this.text,
    this.iconSize = 40,
  });

  final IconData icon;
  final HeroTone tone;
  final String title;
  final String? text;
  final double iconSize;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final (Color fg, Color bg) = switch (tone) {
      HeroTone.ok => (t.ok, t.okBg),
      HeroTone.warn => (t.warn, t.warnBg),
      HeroTone.err => (t.err, t.err.withValues(alpha: .14)),
      HeroTone.info => (t.info, t.infoBg),
    };
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 26, 4, 14),
      child: Column(
        children: [
          Container(
            width: 76,
            height: 76,
            decoration: BoxDecoration(color: bg, shape: BoxShape.circle),
            child: ExcludeSemantics(
              child: Icon(icon, size: iconSize, color: fg),
            ),
          ),
          const SizedBox(height: 10),
          Semantics(
            header: true,
            child: Text(
              title,
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 22,
                fontWeight: FontWeight.w700,
                height: 1.25,
                letterSpacing: -.2,
                color: t.ink,
              ),
            ),
          ),
          if (text != null) ...[
            const SizedBox(height: 10),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 300),
              child: Text(
                text!,
                textAlign: TextAlign.center,
                style: TextStyle(fontSize: 14, height: 1.45, color: t.ink2),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// A numbered list: a small grey circle with the number, then the line. The circle is centred on a one-line item; on
/// an item that wraps its top lines up with the first line's top.
class NumberedSteps extends StatelessWidget {
  const NumberedSteps(this.lines, {super.key});
  final List<String> lines;

  static const circle = 26.0;
  static const gap = 10.0;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final style = TextStyle(fontSize: 14, height: 1.45, color: t.ink);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < lines.length; i++)
          Padding(
            padding: EdgeInsets.only(top: i == 0 ? 0 : 10),
            child: LayoutBuilder(
              builder: (context, box) {
                final painter = TextPainter(
                  text: TextSpan(text: lines[i], style: style),
                  textDirection: Directionality.of(context),
                  textScaler: MediaQuery.textScalerOf(context),
                )..layout(maxWidth: box.maxWidth - circle - gap);
                final wraps = painter.computeLineMetrics().length > 1;
                painter.dispose();
                return Row(
                  crossAxisAlignment: wraps
                      ? CrossAxisAlignment.start
                      : CrossAxisAlignment.center,
                  children: [
                    Container(
                      key: Key('numbered-step-circle-$i'),
                      width: circle,
                      height: circle,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: t.surface3,
                        shape: BoxShape.circle,
                      ),
                      child: Text(
                        '${i + 1}',
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                          color: t.ink2,
                        ),
                      ),
                    ),
                    const SizedBox(width: gap),
                    Expanded(child: Text(lines[i], style: style)),
                  ],
                );
              },
            ),
          ),
      ],
    );
  }
}

/// A glass card of label and value rows.
class KeyValueCard extends StatelessWidget {
  const KeyValueCard(this.rows, {super.key});
  final List<(String, String)> rows;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Glass(
      radius: 16,
      padding: const EdgeInsets.all(14),
      child: Table(
        columnWidths: const {0: IntrinsicColumnWidth(), 1: FlexColumnWidth()},
        children: [
          for (var i = 0; i < rows.length; i++)
            TableRow(
              children: [
                Padding(
                  padding: EdgeInsets.only(top: i == 0 ? 0 : 8, right: 14),
                  child: Text(
                    rows[i].$1,
                    style: TextStyle(fontSize: 13, color: t.ink3),
                  ),
                ),
                Padding(
                  padding: EdgeInsets.only(top: i == 0 ? 0 : 8),
                  child: Text(
                    rows[i].$2,
                    style: TextStyle(fontSize: 13, color: t.ink),
                  ),
                ),
              ],
            ),
        ],
      ),
    );
  }
}

/// Small print with a shield in front of it. The shield is centred on the first line, however many lines follow.
class ShieldNote extends StatelessWidget {
  const ShieldNote(this.text, {super.key, this.center = true});
  final String text;
  final bool center;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final style = TextStyle(fontSize: 12, height: 1.35, color: t.ink3);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisAlignment: center
          ? MainAxisAlignment.center
          : MainAxisAlignment.start,
      children: [
        FirstLineIcon(
          icon: Icon(shieldCheckIcon, size: 16, color: t.ink3),
          size: 16,
          style: style,
        ),
        const SizedBox(width: 6),
        Flexible(
          child: Text(
            text,
            textAlign: center ? TextAlign.center : TextAlign.start,
            style: style,
          ),
        ),
      ],
    );
  }
}
