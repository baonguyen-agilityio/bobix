import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../theme/theme.dart';
import '../theme/tokens.dart';
import '../features/actions/action_widgets.dart' show HeaderPill;
import 'header_bar.dart';
import 'press_feedback.dart';

/// A pushed screen: a back button and a title on top, the content scrolling, and an optional bar of actions at the bottom.
class DetailScaffold extends ConsumerWidget {
  const DetailScaffold({
    super.key,
    required this.title,
    required this.children,
    this.actions,
  });

  /// What a screen shows while its data is on the way.
  const DetailScaffold.loading({super.key, required this.title})
    : children = const [
        Center(
          child: Padding(
            padding: EdgeInsets.all(32),
            child: CircularProgressIndicator(),
          ),
        ),
      ],
      actions = null;

  final String title;
  final List<Widget> children;
  final Widget? actions;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: SafeArea(
        child: Column(
          children: [
            HeaderBar(
              title: title,
              leading: HeaderPill(
                label: s('phone.a.back'),
                icon: Icons.chevron_left,
                onTap: () => Navigator.of(context).maybePop(),
              ),
            ),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 20),
                children: children,
              ),
            ),
            if (actions != null)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
                child: actions,
              ),
          ],
        ),
      ),
    );
  }
}

/// A capsule button of the prototype. [onPressed] null draws it disabled.
class PillButton extends StatelessWidget {
  const PillButton({
    super.key,
    required this.label,
    this.onPressed,
    this.primary = false,
    this.danger = false,
    this.icon,
    this.iconBuilder,
    this.compact = false,
  });

  final String label;
  final VoidCallback? onPressed;
  final bool primary, danger, compact;
  final IconData? icon;

  /// A drawn glyph in place of [icon] (the Face ID mark); gets the label's colour.
  final Widget Function(Color color)? iconBuilder;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final fg = primary ? t.onAccent : (danger ? t.err : t.ink);
    final shape = RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(999),
      side: BorderSide(
        width: .5,
        color: danger ? t.err.withValues(alpha: .35) : t.line,
      ),
    );
    return Opacity(
      opacity: onPressed == null ? .42 : 1,
      child: Material(
        color: primary
            ? t.accentSolid
            : (t.isDark ? t.surface2 : const Color(0xB3FFFFFF)),
        shape: shape,
        elevation: 0,
        shadowColor: t.accent,
        child: PressFeedback(
          child: InkWell(
            customBorder: shape,
            onTap: onPressed,
            child: DecoratedBox(
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(999),
                gradient: primary
                    ? const LinearGradient(
                        begin: Alignment.topCenter,
                        end: Alignment.bottomCenter,
                        colors: [Color(0x38FFFFFF), Color(0x00FFFFFF)],
                        stops: [0, .55],
                      )
                    : null,
              ),
              child: ConstrainedBox(
                constraints: BoxConstraints(
                  minHeight: compact ? minTouch : 50,
                  minWidth: minTouch,
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 6,
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      if (iconBuilder != null) ...[
                        iconBuilder!(fg),
                        const SizedBox(width: 8),
                      ] else if (icon != null) ...[
                        Icon(icon, size: 18, color: fg),
                        const SizedBox(width: 8),
                      ],
                      Flexible(
                        child: Text(
                          label,
                          textAlign: TextAlign.center,
                          style: TextStyle(
                            fontSize: compact ? 14 : 16,
                            fontWeight: FontWeight.w600,
                            color: fg,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// A one-line note under the actions: small, grey, centred.
class FineNote extends StatelessWidget {
  const FineNote(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: 8),
    child: Text(
      text,
      textAlign: TextAlign.center,
      style: TextStyle(fontSize: 12, height: 1.35, color: context.tokens.ink3),
    ),
  );
}
