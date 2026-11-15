import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'models.dart';

/// Where the app gets what it shows. The link to the computer implements this (`LinkPhoneData`); tests give it made-up data.
abstract interface class PhoneData {
  /// The last `home` and every later one.
  Stream<HomeData> watchHome();

  /// The held permission request of a session, or null when it has none.
  Future<RequestDetail?> request(String sessionId);

  /// A session with its last lines, or null when the computer does not know it.
  Future<SessionDetail?> session(String sessionId);
}

/// Nothing, while no computer is paired: the screens that read it are not shown then. `LinkPhoneData` replaces it once
/// paired (see `phoneDataOverride` in `link_providers.dart`).
final phoneDataProvider = Provider<PhoneData>((ref) => const NoPhoneData());

class NoPhoneData implements PhoneData {
  const NoPhoneData();

  @override
  Stream<HomeData> watchHome() => const Stream.empty();

  @override
  Future<RequestDetail?> request(String sessionId) async => null;

  @override
  Future<SessionDetail?> session(String sessionId) async => null;
}

final homeProvider = StreamProvider<HomeData>(
  (ref) => ref.watch(phoneDataProvider).watchHome(),
);

final requestProvider = FutureProvider.family<RequestDetail?, String>(
  (ref, id) => ref.watch(phoneDataProvider).request(id),
);

final sessionProvider = FutureProvider.family<SessionDetail?, String>(
  (ref, id) => ref.watch(phoneDataProvider).session(id),
);
