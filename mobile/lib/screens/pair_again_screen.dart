// ignore_for_file: sort_child_properties_last
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../link/link_providers.dart';
import '../widgets/detail_scaffold.dart';
import 'pair_chrome.dart';

/// The pairing cannot be used any more. Either the computer said so (its `revoked`: keys and data are already erased,
/// "disconnected") or the relay says this phone may have been disconnected: the person erases what is left and pairs again.
class PairAgainScreen extends ConsumerWidget {
  const PairAgainScreen({super.key, required this.phase});

  final PairAgain phase;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final gone = phase.removedByComputer;
    final name = phase.computer.isEmpty
        ? s('phone.pair.thecomputer')
        : phase.computer;
    final erase = ref.read(linkControllerProvider.notifier).eraseAndPairAgain;
    return PairPage(
      showTitle: false,
      children: gone
          ? [
              PairHero(
                icon: Icons.cloud_off_outlined,
                tone: HeroTone.err,
                title: s('phone.dc.h'),
                text: s('phone.dc.d', {'c': name}),
              ),
              KeyValueCard([
                (s('phone.dc.k1'), s('phone.dc.v1', {'c': name})),
                (s('phone.dc.k3'), s('phone.dc.v3')),
              ]),
              const SizedBox(height: 12),
              ShieldNote(s('phone.dc.n'), center: false),
            ]
          : [
              PairHero(
                icon: Icons.swap_horiz,
                tone: HeroTone.warn,
                title: s('phone.again.t'),
                text: s('phone.again.d', {'c': name}),
              ),
              const SizedBox(height: 12),
              ShieldNote(s('phone.again.n'), center: false),
            ],
      actions: PillButton(
        label: gone ? s('phone.dc.again') : s('phone.again.erase'),
        primary: true,
        icon: Icons.qr_code_scanner,
        onPressed: erase,
      ),
    );
  }
}
