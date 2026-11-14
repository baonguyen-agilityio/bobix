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
  });

  final String petDesign;
  final int petColor;
  final String petState;
  final String title, subtitle, meta;
  final bool needsYou, muted;
  final Color? subtitleColor;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final tint = needsYou
        ? Color.alphaBlend(t.accent.withValues(alpha: .10), t.surface)
        : null;
    return Opacity(
      opacity: muted ? .7 : 1,
      child: Semantics(
        button: onTap != null,
        label: '$title. $subtitle. $meta',
        excludeSemantics: true,
        child: Glass(
          tint: tint,
          border: needsYou ? t.accent.withValues(alpha: .35) : null,
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
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
