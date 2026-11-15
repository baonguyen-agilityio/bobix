// ignore_for_file: sort_child_properties_last
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../link/link_providers.dart';
import '../widgets/detail_scaffold.dart';
import 'pair_chrome.dart';

/// The pairing could not be opened for now. Nothing was erased, and no pairing can start over it: the person tries again.
class UnavailableScreen extends ConsumerWidget {
  const UnavailableScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return PairPage(
      showTitle: false,
      children: [
        PairHero(
          icon: Icons.lock_clock_outlined,
          tone: HeroTone.warn,
          title: s('phone.load.t'),
          text: s('phone.load.d'),
        ),
      ],
      actions: PillButton(
        label: s('phone.load.retry'),
        primary: true,
        onPressed: ref.read(linkControllerProvider.notifier).retry,
      ),
    );
  }
}
