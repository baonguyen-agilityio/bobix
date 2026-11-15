import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../features/actions/action_widgets.dart'
    show GlassActionBar, HeaderPill;
import '../l10n/l10n.dart';
import '../theme/tokens.dart';
import '../widgets/glass.dart';

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
            SizedBox(
              height: 52,
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
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 17,
                            fontWeight: FontWeight.w600,
                            color: t.ink,
                          ),
                        ),
                      ),
                    ),
                  if (onBack != null)
                    Positioned(
                      left: 8,
                      child: HeaderPill(
                        label: s('phone.a.back'),
                        icon: Icons.chevron_left,
                        onTap: onBack!,
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
            if (actions != null) GlassActionBar(child: actions!),
          ],
        ),
      ),
    );
  }
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

/// A numbered list: a small grey circle with the number, then the line.
class NumberedSteps extends StatelessWidget {
  const NumberedSteps(this.lines, {super.key});
  final List<String> lines;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < lines.length; i++)
          Padding(
            padding: EdgeInsets.only(top: i == 0 ? 0 : 10),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Container(
                  width: 26,
                  height: 26,
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
                const SizedBox(width: 10),
                Expanded(
                  child: Padding(
                    padding: const EdgeInsets.only(top: 3),
                    child: Text(
                      lines[i],
                      style: TextStyle(
                        fontSize: 14,
                        height: 1.45,
                        color: t.ink,
                      ),
                    ),
                  ),
                ),
              ],
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

/// Small print with a shield in front of it.
class ShieldNote extends StatelessWidget {
  const ShieldNote(this.text, {super.key, this.center = true});
  final String text;
  final bool center;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Row(
      mainAxisAlignment: center
          ? MainAxisAlignment.center
          : MainAxisAlignment.start,
      children: [
        ExcludeSemantics(
          child: Icon(Icons.shield_outlined, size: 16, color: t.ink3),
        ),
        const SizedBox(width: 6),
        Flexible(
          child: Text(
            text,
            textAlign: center ? TextAlign.center : TextAlign.start,
            style: TextStyle(fontSize: 12, height: 1.35, color: t.ink3),
          ),
        ),
      ],
    );
  }
}
