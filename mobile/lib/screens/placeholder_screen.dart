import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../theme/tokens.dart';
import '../widgets/glass.dart';

/// A tab whose real content comes with a later part of the app: a title and one honest line.
class PlaceholderScreen extends ConsumerWidget {
  const PlaceholderScreen({
    super.key,
    required this.titleKey,
    required this.bodyKey,
    this.detailKey,
  });

  final String titleKey, bodyKey;
  final String? detailKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 96),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 6, 0, 12),
          child: Semantics(
            header: true,
            child: Text(
              s(titleKey),
              style: TextStyle(
                fontSize: 28,
                fontWeight: FontWeight.w700,
                letterSpacing: -.5,
                color: t.ink,
              ),
            ),
          ),
        ),
        Glass(
          padding: const EdgeInsets.all(16),
          child: Column(
            children: [
              Text(
                s(bodyKey),
                textAlign: TextAlign.center,
                style: TextStyle(fontWeight: FontWeight.w700, color: t.ink),
              ),
              if (detailKey != null) ...[
                const SizedBox(height: 4),
                Text(
                  s(detailKey!),
                  textAlign: TextAlign.center,
                  style: TextStyle(color: t.ink2),
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }
}
