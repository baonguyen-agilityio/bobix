import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../lock/own_prompt.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/floating_sheet.dart';
import '../widgets/status_banner.dart';
import 'push_platform.dart';
import 'push_providers.dart';

/// Whether notifications are allowed, refused or not asked yet. Read again when the app comes back to the front (the person
/// may have changed it in iOS Settings). Null while it is not known, or when there is no platform (a test).
final pushPermissionProvider = FutureProvider<PushPermission?>((ref) async {
  try {
    return await ref.read(pushRegistrationProvider).permissionStatus();
  } on Object {
    return null;
  }
});

/// Right after a pairing: says why notifications help, and on "Turn on" asks iOS (its own dialog, once). A person who was
/// already asked, or who refused, is not asked again: the system would not show its dialog, and Settings says how to turn
/// them on. Leaving with "Not now" asks nothing; Settings offers it later. Nothing here can fail the pairing.
Future<void> offerNotifications(BuildContext context, WidgetRef ref) async {
  final reg = ref.read(pushRegistrationProvider);
  try {
    if (await reg.permissionStatus() != PushPermission.notDetermined) return;
    if (!context.mounted) return;
    final allow = await showModalBottomSheet<bool>(
      context: context,
      isScrollControlled: true,
      backgroundColor: Colors.transparent,
      builder: (_) => const _ReasonSheet(),
    );
    if (allow == true) {
      await duringOwnPrompt(reg.requestPermissionAndRegister);
    }
  } on Object {
    // Push is not available here (no platform channel): the pairing goes on without it.
  }
}

class _ReasonSheet extends ConsumerWidget {
  const _ReasonSheet();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return FloatingSheet(
      children: [
        Semantics(
          header: true,
          child: Text(
            s('phone.push.ask.title'),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 18,
              fontWeight: FontWeight.w700,
              height: 1.25,
              color: t.ink,
            ),
          ),
        ),
        const SizedBox(height: 10),
        Text(
          s('phone.push.ask.reason'),
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 14, height: 1.4, color: t.ink2),
        ),
        const SizedBox(height: 16),
        PillButton(
          label: s('phone.push.ask.allow'),
          primary: true,
          onPressed: () => Navigator.of(context).pop(true),
        ),
        const SizedBox(height: 8),
        PillButton(
          label: s('phone.push.ask.later'),
          onPressed: () => Navigator.of(context).pop(false),
        ),
      ],
    );
  }
}

/// In Settings: nothing while notifications are on. Refused: says so and opens iOS Settings (the only place to turn them
/// on again). Not asked yet (a person who chose "Not now", or who paired before this existed): offers to turn them on.
class NotificationsPermissionNotice extends ConsumerStatefulWidget {
  const NotificationsPermissionNotice({super.key});

  @override
  ConsumerState<NotificationsPermissionNotice> createState() =>
      _NotificationsPermissionNoticeState();
}

class _NotificationsPermissionNoticeState
    extends ConsumerState<NotificationsPermissionNotice> {
  late final AppLifecycleListener _lifecycle;

  @override
  void initState() {
    super.initState();
    _lifecycle = AppLifecycleListener(
      onResume: () => ref.invalidate(pushPermissionProvider),
    );
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    super.dispose();
  }

  Future<void> _ask() async {
    try {
      await duringOwnPrompt(
        ref.read(pushRegistrationProvider).requestPermissionAndRegister,
      );
    } on Object {
      // No platform: nothing to ask.
    }
    ref.invalidate(pushPermissionProvider);
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    return switch (ref.watch(pushPermissionProvider).value) {
      PushPermission.denied => StatusBanner(
        kind: BannerKind.info,
        icon: Icons.notifications_off_outlined,
        title: s('phone.push.denied'),
        action: PillButton(
          label: s('phone.push.denied.open'),
          compact: true,
          onPressed: () =>
              ref.read(pushRegistrationProvider).openSystemSettings(),
        ),
      ),
      PushPermission.notDetermined => StatusBanner(
        kind: BannerKind.info,
        icon: Icons.notifications_outlined,
        title: s('phone.push.ask.title'),
        body: s('phone.push.ask.reason'),
        action: PillButton(
          label: s('phone.push.ask.allow'),
          compact: true,
          primary: true,
          onPressed: _ask,
        ),
      ),
      _ => const SizedBox.shrink(),
    };
  }
}
