import 'package:flutter/material.dart';

import '../theme/tokens.dart';

enum BannerKind { info, warn, err }

class StatusBanner extends StatelessWidget {
  const StatusBanner({
    super.key,
    required this.kind,
    required this.icon,
    required this.title,
    this.body,
    this.action,
  });

  final BannerKind kind;
  final IconData icon;
  final String title;
  final String? body;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final (Color fg, Color bg) = switch (kind) {
      BannerKind.info => (t.info, t.infoBg),
      BannerKind.warn => (t.warn, t.warnBg),
      BannerKind.err => (t.err, t.err.withValues(alpha: .11)),
    };
    return Semantics(
      container: true,
      child: Container(
        margin: const EdgeInsets.only(bottom: 12),
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: bg,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(width: .5, color: fg.withValues(alpha: .45)),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 1),
              child: Icon(icon, size: 20, color: fg),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    title,
                    style: TextStyle(
                      fontSize: 14,
                      fontWeight: FontWeight.w700,
                      color: t.ink,
                    ),
                  ),
                  if (body != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 2),
                      child: Text(
                        body!,
                        style: TextStyle(
                          fontSize: 13,
                          height: 1.4,
                          color: t.ink,
                        ),
                      ),
                    ),
                  if (action != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 8),
                      child: action,
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
