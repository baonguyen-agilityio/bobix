import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/oklch.dart';
import '../../theme/theme.dart';
import '../../theme/tokens.dart';
import '../../widgets/glass.dart';
import '../../fixtures/fixtures.dart';
import '../../screens/home_screen.dart';
import '../../widgets/status_banner.dart';
import 'hidden_chars.dart';

/// The destructive red of the prototype's `--err-solid`.
final errSolid = oklch(50, .19, 22);

/// The name of the biometric the system asks for on this phone.
String bioName(Strings s) => defaultTargetPlatform == TargetPlatform.iOS
    ? s('phone.bio.face')
    : s('phone.bio.finger');

/// A screen of the action flow: a back button and title on top, a scrolling body, and an action bar that floats
/// at the bottom.
class ActionPage extends ConsumerWidget {
  const ActionPage({
    super.key,
    required this.title,
    required this.children,
    this.actions,
    this.showBack = true,
  });

  final String title;
  final List<Widget> children;
  final Widget? actions;
  final bool showBack;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return Scaffold(
      backgroundColor: t.bg,
      body: SafeArea(
        child: Column(
          children: [
            SizedBox(
              height: 52,
              child: Row(
                children: [
                  SizedBox(
                    width: 96,
                    child: showBack
                        ? Padding(
                            padding: const EdgeInsets.only(left: 8),
                            child: Align(
                              alignment: Alignment.centerLeft,
                              child: Semantics(
                                button: true,
                                label: s('phone.a.back'),
                                child: InkWell(
                                  borderRadius: BorderRadius.circular(22),
                                  onTap: () => Navigator.of(context).maybePop(),
                                  child: Glass(
                                    radius: 22,
                                    child: const SizedBox(
                                      width: minTouch,
                                      height: minTouch,
                                      child: Icon(
                                        Icons.arrow_back_ios_new,
                                        size: 18,
                                      ),
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          )
                        : null,
                  ),
                  Expanded(
                    child: Semantics(
                      header: true,
                      child: Text(
                        title,
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
                  const SizedBox(width: 96),
                ],
              ),
            ),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 20),
                children: children,
              ),
            ),
            if (actions != null)
              Container(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
                decoration: BoxDecoration(
                  color: t.surface.withValues(alpha: .86),
                  border: Border(top: BorderSide(width: .5, color: t.line)),
                ),
                child: actions,
              ),
          ],
        ),
      ),
    );
  }
}

enum BtnKind { primary, normal, danger, dangerFill }

/// A capsule button at least [minTouch] high. A null [onPressed] shows it off.
class ActionButton extends StatelessWidget {
  const ActionButton(
    this.label, {
    super.key,
    required this.onPressed,
    this.kind = BtnKind.normal,
    this.icon,
  });

  final String label;
  final VoidCallback? onPressed;
  final BtnKind kind;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final (Color bg, Color fg, Color? border) = switch (kind) {
      BtnKind.primary => (t.accentSolid, t.onAccent, null),
      BtnKind.dangerFill => (errSolid, Colors.white, null),
      BtnKind.danger => (t.surface, t.err, t.err.withValues(alpha: .45)),
      BtnKind.normal => (t.surface, t.ink, t.lineStrong),
    };
    return Opacity(
      opacity: onPressed == null ? .42 : 1,
      child: Material(
        color: bg,
        shape: StadiumBorder(
          side: border == null
              ? BorderSide.none
              : BorderSide(width: .5, color: border),
        ),
        child: InkWell(
          customBorder: const StadiumBorder(),
          onTap: onPressed,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 50),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  if (icon != null) ...[
                    Icon(icon, size: 18, color: fg),
                    const SizedBox(width: 8),
                  ],
                  Flexible(
                    child: Text(
                      label,
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        fontSize: 16,
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
    );
  }
}

/// The red band of a destructive request or one with hidden characters.
class DangerBand extends StatelessWidget {
  const DangerBand({super.key, required this.title, required this.body});
  final String title, body;

  @override
  Widget build(BuildContext context) => Semantics(
    container: true,
    child: Container(
      margin: const EdgeInsets.only(bottom: 10),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: errSolid,
        borderRadius: BorderRadius.circular(16),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(
            Icons.warning_amber_rounded,
            size: 20,
            color: Colors.white,
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: const TextStyle(
                    fontSize: 14,
                    fontWeight: FontWeight.w700,
                    color: Colors.white,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  body,
                  style: const TextStyle(
                    fontSize: 13,
                    height: 1.4,
                    color: Colors.white,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    ),
  );
}

/// A monospace block. Control and format characters are drawn as visible escapes, never as what they do.
class CodeBlock extends StatelessWidget {
  const CodeBlock(
    this.text, {
    super.key,
    this.danger = false,
    this.maxHeight,
    this.fontSize = 13,
  });

  final String text;
  final bool danger;
  final double? maxHeight;
  final double fontSize;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final mono = TextStyle(
      fontFamily: 'Menlo',
      fontFamilyFallback: const ['Roboto Mono', 'monospace'],
      fontSize: fontSize,
      height: 1.5,
      color: t.ink,
    );
    final body = Text.rich(
      TextSpan(
        children: [
          for (final p in showEscapes(text))
            TextSpan(
              text: p.text,
              style: p.escaped
                  ? TextStyle(
                      fontWeight: FontWeight.w700,
                      backgroundColor: t.err.withValues(alpha: .28),
                    )
                  : null,
            ),
        ],
        style: mono,
      ),
    );
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: danger
            ? Color.alphaBlend(t.err.withValues(alpha: .08), t.surface2)
            : t.surface2,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(
          width: danger ? 1 : .5,
          color: danger ? t.err : t.line,
        ),
      ),
      child: maxHeight == null
          ? body
          : ClipRect(
              child: ConstrainedBox(
                constraints: BoxConstraints(maxHeight: maxHeight!),
                child: SingleChildScrollView(
                  physics: const NeverScrollableScrollPhysics(),
                  child: body,
                ),
              ),
            ),
    );
  }
}

class FinePrint extends StatelessWidget {
  const FinePrint(this.text, {super.key, this.align = TextAlign.center});
  final String text;
  final TextAlign align;

  @override
  Widget build(BuildContext context) => Text(
    text,
    textAlign: align,
    style: TextStyle(fontSize: 12, height: 1.35, color: context.tokens.ink3),
  );
}

/// A small pill (folder, computer, countdown).
class MetaChip extends StatelessWidget {
  const MetaChip(this.text, {super.key, this.icon, this.color});
  final String text;
  final IconData? icon;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final c = color ?? t.ink2;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 3),
      decoration: BoxDecoration(
        color: t.surface3,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: 13, color: c),
            const SizedBox(width: 4),
          ],
          Flexible(
            child: Text(
              text,
              style: TextStyle(
                fontSize: 12,
                fontWeight: FontWeight.w600,
                color: c,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Why sending is off (no network, relay down); nothing when it is on.
class SendOffBanner extends ConsumerWidget {
  const SendOffBanner({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return switch (ref.watch(connProvider)) {
      ConnState.noNetwork => StatusBanner(
        kind: BannerKind.warn,
        icon: Icons.wifi_off,
        title: s('phone.sn.nonet'),
      ),
      ConnState.relayDown => StatusBanner(
        kind: BannerKind.err,
        icon: Icons.cloud_off,
        title: s('phone.sn.relay'),
      ),
      _ => const SizedBox.shrink(),
    };
  }
}

/// A heading and its body in a glass card: the shape of a request's or a session's header.
class TitleCard extends StatelessWidget {
  const TitleCard({
    super.key,
    required this.title,
    required this.subtitle,
    this.chips = const [],
    this.icon = Icons.terminal,
  });
  final String title, subtitle;
  final List<Widget> chips;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Glass(
      padding: const EdgeInsets.all(14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 44,
                height: 44,
                decoration: BoxDecoration(
                  color: t.accentBg,
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Icon(icon, size: 22, color: t.accentInk),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w700,
                        color: t.ink,
                      ),
                    ),
                    Text(
                      subtitle,
                      style: TextStyle(fontSize: 13, color: t.ink2),
                    ),
                  ],
                ),
              ),
            ],
          ),
          if (chips.isNotEmpty) ...[
            const SizedBox(height: 10),
            Wrap(spacing: 6, runSpacing: 6, children: chips),
          ],
        ],
      ),
    );
  }
}
