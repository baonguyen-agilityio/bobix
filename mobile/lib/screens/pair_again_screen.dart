import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../link/link_providers.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';

/// The pairing cannot be used any more. Either the computer said so (its `revoked`: keys and data are already erased) or
/// the relay says this phone may have been disconnected: the person erases what is left and pairs again.
class PairAgainScreen extends ConsumerWidget {
  const PairAgainScreen({super.key, required this.phase});

  final PairAgain phase;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final gone = phase.removedByComputer;
    final name = phase.computer.isEmpty
        ? s('phone.pair.thecomputer')
        : phase.computer;
    return Scaffold(
      backgroundColor: t.bg,
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.fromLTRB(20, 24, 20, 32),
          children: [
            Icon(Icons.link_off, size: 40, color: t.accent),
            const SizedBox(height: 12),
            Semantics(
              header: true,
              child: Text(
                gone ? s('phone.again.gone', {'c': name}) : s('phone.again.t'),
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
              gone ? s('phone.again.gone.d') : s('phone.again.d', {'c': name}),
              style: TextStyle(fontSize: 16, height: 1.4, color: t.ink2),
            ),
            const SizedBox(height: 24),
            PillButton(
              label: s('phone.again.erase'),
              primary: true,
              onPressed: ref
                  .read(linkControllerProvider.notifier)
                  .eraseAndPairAgain,
            ),
          ],
        ),
      ),
    );
  }
}
