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
    final link = ref.read(linkControllerProvider.notifier);
    final at = phase.removedAt;
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
                if (at != null)
                  (s('phone.dc.k2'), _when(s, at, ref.read(clockProvider)())),
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
      actions: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          PillButton(
            label: s('phone.dc.again'),
            primary: true,
            icon: Icons.qr_code_scanner,
            onPressed: link.eraseAndPairAgain,
          ),
          if (!gone) ...[
            const SizedBox(height: 8),
            PillButton(
              label: s('phone.again.forget'),
              danger: true,
              onPressed: link.forgetComputer,
            ),
          ],
        ],
      ),
    );
  }
}

/// "21:14 · today", or "21:14 · 11 Nov" when it was another day.
String _when(Strings s, DateTime at, DateTime now) {
  final time =
      '${at.hour.toString().padLeft(2, '0')}:${at.minute.toString().padLeft(2, '0')}';
  final today =
      at.year == now.year && at.month == now.month && at.day == now.day;
  return today
      ? s('phone.dc.when.today', {'t': time})
      : s('phone.dc.when.on', {
          't': time,
          'd': '${at.day} ${s('phone.mon.${at.month}')}',
        });
}
