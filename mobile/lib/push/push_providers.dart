import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'push_link_store.dart';
import 'push_platform.dart';
import 'push_registration.dart';

final pushPlatformProvider = Provider<PushPlatform>(
  (ref) => ChannelPushPlatform(),
);

final pushRegistrationProvider = Provider<PushRegistration>((ref) {
  final r = PushRegistration(ref.watch(pushPlatformProvider));
  ref.onDispose(r.dispose);
  return r;
});

final pushLinkStoreProvider = Provider<PushLinkStore>(
  (ref) => PushLinkStore(ref.watch(pushPlatformProvider)),
);
