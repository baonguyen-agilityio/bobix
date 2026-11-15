import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../link/link_providers.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';

/// The pairing could not be opened for now. Nothing was erased, and no pairing can start over it: the person tries again.
class UnavailableScreen extends ConsumerWidget {
  const UnavailableScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return Scaffold(
      backgroundColor: t.bg,
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.fromLTRB(20, 24, 20, 32),
          children: [
            Icon(Icons.lock_clock_outlined, size: 40, color: t.accent),
            const SizedBox(height: 12),
            Semantics(
              header: true,
              child: Text(
                s('phone.load.t'),
                style: TextStyle(
                  fontSize: 28,
                  fontWeight: FontWeight.w700,
                  letterSpacing: -.5,
                  color: t.ink,
                ),
              ),
            ),
            const SizedBox(height: 12),
            Text(
              s('phone.load.d'),
              style: TextStyle(fontSize: 16, height: 1.4, color: t.ink2),
            ),
            const SizedBox(height: 24),
            PillButton(
              label: s('phone.load.retry'),
              primary: true,
              onPressed: ref.read(linkControllerProvider.notifier).retry,
            ),
          ],
        ),
      ),
    );
  }
}
