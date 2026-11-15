import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'models.dart';

/// Where the app gets what it shows. The link to the computer implements this (`LinkPhoneData`); tests give it made-up data.
/// The action screens read their own interface (`ActionsSource`).
abstract interface class PhoneData {
  /// The last `home` and every later one.
  Stream<HomeData> watchHome();
}

/// Nothing, while no computer is paired: the screens that read it are not shown then. `LinkPhoneData` replaces it once
/// paired (see `linkPhoneData` in `link_providers.dart`).
final phoneDataProvider = Provider<PhoneData>((ref) => const NoPhoneData());

class NoPhoneData implements PhoneData {
  const NoPhoneData();

  @override
  Stream<HomeData> watchHome() => const Stream.empty();
}

final homeProvider = StreamProvider<HomeData>(
  (ref) => ref.watch(phoneDataProvider).watchHome(),
);
