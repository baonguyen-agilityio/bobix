import 'package:flutter/material.dart';

import '../pets/pet_view.dart';
import '../theme/theme.dart';
import '../theme/tokens.dart';
import 'glass.dart';

/// A task card of Home: pet, name, one line of status, and where it belongs.
class SessionRow extends StatelessWidget {
  const SessionRow({
    super.key,
    required this.petDesign,
    required this.petColor,
    required this.petState,
    required this.title,
    required this.subtitle,
    required this.meta,
    this.needsYou = false,
    this.muted = false,
    this.subtitleColor,
    this.onTap,
    this.logCount,
    this.logLabel = '',
    this.onLog,
  });

  final String petDesign;
  final int petColor;
  final String petState;
  final String title, subtitle, meta;
  final bool needsYou, muted;
  final Color? subtitleColor;
  final VoidCallback? onTap;

  /// The entries of this session's log, shown as a pill before the chevron; [onLog] opens that log.
  final int? logCount;
  final String logLabel;
  final VoidCallback? onLog;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final tint = needsYou
        ? Color.alphaBlend(t.accent.withValues(alpha: .10), t.surface)
        : null;
    final hasLog = logCount != null && onLog != null;
    return Opacity(
      opacity: muted ? .7 : 1,
      child: Glass(
        tint: tint,
        child: Stack(
          children: [
            Semantics(
              button: onTap != null,
              label: '$title. $subtitle. $meta',
              excludeSemantics: true,
              child: InkWell(
                borderRadius: BorderRadius.circular(16),
                onTap: onTap,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(minHeight: minTouch + 17),
                  child: Padding(
                    padding: const EdgeInsets.fromLTRB(8, 8, 12, 8),
                    child: Row(
                      children: [
                        PetView(
                          design: petDesign,
                          color: petColor,
                          state: petState,
                          size: 44,
                          halo: needsYou,
                        ),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                title,
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 15,
                                  fontWeight: FontWeight.w600,
                                  color: t.ink,
                                  height: 1.3,
                                ),
                              ),
                              Text(
                                subtitle,
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 13,
                                  color: subtitleColor ?? t.ink2,
                                  height: 1.35,
                                ),
                              ),
                              Text(
                                meta,
                                style: TextStyle(fontSize: 12, color: t.ink3),
                              ),
                            ],
                          ),
                        ),
                        if (hasLog) const SizedBox(width: 50),
                        Icon(Icons.chevron_right, size: 18, color: t.ink3),
                      ],
                    ),
                  ),
                ),
              ),
            ),
            if (hasLog)
              Positioned(
                right: 30,
                top: 0,
                bottom: 0,
                child: Center(
                  child: LogPill(
                    count: logCount!,
                    label: logLabel,
                    onTap: onLog!,
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// The small pill of a task card: a list icon and how many entries the task's log holds. It opens that log.
class LogPill extends StatelessWidget {
  const LogPill({
    super.key,
    required this.count,
    required this.label,
    required this.onTap,
  });

  final int count;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      button: true,
      label: label,
      excludeSemantics: true,
      child: InkResponse(
        onTap: onTap,
        radius: 28,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minWidth: 56, minHeight: minTouch),
          child: Center(
            child: Container(
              height: 26,
              padding: const EdgeInsets.symmetric(horizontal: 9),
              decoration: BoxDecoration(
                color: t.isDark
                    ? const Color(0x0FFFFFFF)
                    : t.surface.withValues(alpha: .6),
                borderRadius: BorderRadius.circular(13),
                border: Border.all(
                  width: .5,
                  color: t.isDark
                      ? const Color(0x1AFFFFFF)
                      : t.ink.withValues(alpha: .10),
                ),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.format_list_bulleted, size: 14, color: t.ink2),
                  const SizedBox(width: 3),
                  Text(
                    '$count',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: t.ink2,
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
