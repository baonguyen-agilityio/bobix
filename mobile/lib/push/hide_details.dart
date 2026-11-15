import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import 'push_providers.dart';

/// The "Hide details" switch: with it on, a notification says only that a session needs you (no project, no command).
/// It lives in the file the notification extension reads, so it takes effect for the next push without opening the app.
/// Off by default.
class HideDetails extends AsyncNotifier<bool> {
  @override
  Future<bool> build() => ref.read(pushPlatformProvider).hideDetails();

  Future<void> set(bool hide) async {
    await ref.read(pushPlatformProvider).setHideDetails(hide);
    state = AsyncData(hide);
  }
}

final hideDetailsProvider = AsyncNotifierProvider<HideDetails, bool>(
  HideDetails.new,
);

/// For the Settings screen.
class HideDetailsSwitch extends ConsumerWidget {
  const HideDetailsSwitch({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final on = ref.watch(hideDetailsProvider).value ?? false;
    return SwitchListTile(
      title: Text(s('phone.push.hide.title')),
      subtitle: Text(s('phone.push.hide.hint')),
      value: on,
      onChanged: (v) => ref.read(hideDetailsProvider.notifier).set(v),
    );
  }
}
