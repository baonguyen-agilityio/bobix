import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'fixture_phone_data.dart';
import 'models.dart';

/// Where the app gets what it shows. The link to the computer implements this; until then [FixturePhoneData] does.
abstract interface class PhoneData {
  /// The last `home` and every later one.
  Stream<HomeData> watchHome();

  /// The held permission request of a session, or null when it has none.
  Future<RequestDetail?> request(String sessionId);

  /// A session with its last lines, or null when the computer does not know it.
  Future<SessionDetail?> session(String sessionId);
}

final phoneDataProvider = Provider<PhoneData>(
  (ref) => const FixturePhoneData(),
);

final homeProvider = StreamProvider<HomeData>(
  (ref) => ref.watch(phoneDataProvider).watchHome(),
);

final requestProvider = FutureProvider.family<RequestDetail?, String>(
  (ref, id) => ref.watch(phoneDataProvider).request(id),
);

final sessionProvider = FutureProvider.family<SessionDetail?, String>(
  (ref, id) => ref.watch(phoneDataProvider).session(id),
);
