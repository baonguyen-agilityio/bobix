import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/oklch.dart';
import '../../theme/theme.dart';
import '../../theme/tokens.dart';
import '../../pets/pet_view.dart';
import '../../widgets/glass.dart';
import '../../widgets/header_bar.dart';
import '../../data/models.dart';
import '../../lock/secure_scope.dart';
import 'actions_source.dart';
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
    this.onLog,
    this.banner,
  });

  final String title;
  final List<Widget> children;
  final Widget? actions;
  final bool showBack;

  /// Opens this task's log; when set, a "Log" pill sits at the right of the header.
  final VoidCallback? onLog;

  /// Pinned above the content (a notice that is about the whole screen).
  final Widget? banner;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return SecureScope(
      child: Scaffold(
        backgroundColor: Colors.transparent,
        body: SafeArea(
          child: Column(
            children: [
              HeaderBar(
                title: title,
                leading: showBack
                    ? HeaderPill(
                        label: s('phone.a.back'),
                        icon: Icons.chevron_left,
                        onTap: () => Navigator.of(context).maybePop(),
                      )
                    : null,
                trailing: onLog != null
                    ? HeaderPill(
                        label: s('phone.tab.log'),
                        icon: Icons.format_list_bulleted,
                        onTap: onLog!,
                      )
                    : null,
              ),
              ?banner,
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
      ),
    );
  }
}

/// The bar of actions at the bottom of a page. It has no fill, blur or border of its own: the page's wash shows
/// through, so no edge can be seen (the content ends above it, it does not scroll under it).
class GlassActionBar extends StatelessWidget {
  const GlassActionBar({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) =>
      Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 12), child: child);
}

/// A glass capsule in a screen's header: "‹ Back" on the left, "Log" on the right.
class HeaderPill extends StatelessWidget {
  const HeaderPill({
    super.key,
    required this.label,
    required this.icon,
    required this.onTap,
  });

  final String label;
  final IconData icon;
  final VoidCallback onTap;

  /// The label's text scale is capped so the pills cannot push the title out.
  static const maxTextScale = 1.3;

  /// Everything of the pill but its label: padding, border, the icon and the gap after it.
  static const chrome = 10 + 10 + 1 + 20 + 2.0;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      button: true,
      label: label,
      excludeSemantics: true,
      child: InkWell(
        borderRadius: BorderRadius.circular(22),
        onTap: onTap,
        child: Glass(
          radius: 22,
          padding: const EdgeInsets.symmetric(horizontal: 10),
          child: SizedBox(
            height: 38,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(icon, size: 20, color: t.accentInk),
                const SizedBox(width: 2),
                // The pills keep a readable size but do not grow without limit, or they would push the title out.
                MediaQuery.withClampedTextScaling(
                  maxScaleFactor: maxTextScale,
                  child: Text(
                    label,
                    style: TextStyle(fontSize: 16, color: t.accentInk),
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
    final shape = StadiumBorder(
      side: border == null
          ? BorderSide.none
          : BorderSide(width: .5, color: border),
    );
    final primary = kind == BtnKind.primary;
    final button = Material(
      color: primary ? Colors.transparent : bg,
      shape: shape,
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
    );
    return Opacity(
      opacity: onPressed == null ? .42 : 1,
      // The primary button of the prototype: a glass sheen on the accent and a soft accent shadow.
      child: primary
          ? DecoratedBox(
              decoration: ShapeDecoration(
                shape: const StadiumBorder(),
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [
                    Color.alphaBlend(const Color(0x38FFFFFF), bg),
                    bg,
                    bg,
                  ],
                  stops: const [0, .55, 1],
                ),
                shadows: [
                  BoxShadow(
                    color: t.accent.withValues(alpha: .28),
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
              child: button,
            )
          : button,
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
            : (t.isDark
                  ? t.surface2.withValues(alpha: .6)
                  : const Color(0x73FFFFFF)),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(
          width: danger ? 1 : .5,
          color: danger
              ? t.err
              : (t.isDark ? const Color(0x17FFFFFF) : const Color(0xE6FFFFFF)),
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

/// The header card of a request, plan, question or session: the session's pet, its name, where it belongs, and a chip
/// that says what it is doing now.
class SessionCard extends StatelessWidget {
  const SessionCard({
    super.key,
    required this.petDesign,
    required this.petColor,
    required this.petState,
    required this.title,
    required this.subtitle,
    this.chip,
    this.chipColor,
    this.chipBg,
    this.halo = false,
    this.extraChips = const [],
  });

  final String petDesign, petState, title, subtitle;
  final int petColor;
  final String? chip;
  final Color? chipColor, chipBg;
  final bool halo;

  /// More chips beside [chip] (a countdown).
  final List<Widget> extraChips;

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
              PetView(
                design: petDesign,
                color: petColor,
                state: petState,
                size: 60,
                halo: halo,
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
                        height: 1.3,
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
          if (chip != null || extraChips.isNotEmpty) ...[
            const SizedBox(height: 10),
            Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                if (chip != null)
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 1,
                    ),
                    constraints: const BoxConstraints(minHeight: 22),
                    decoration: BoxDecoration(
                      color: chipBg ?? t.accentBg,
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: Text(
                      chip!,
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                        color: chipColor ?? t.accentInk,
                      ),
                    ),
                  ),
                ...extraChips,
              ],
            ),
          ],
        ],
      ),
    );
  }
}
