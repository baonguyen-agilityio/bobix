import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../src/rust/api/core.dart';

/// Which kind of build this is: `dev` talks plain WebSocket to a relay on this computer (`relay --dev`), `prod` only
/// ever connects over TLS. Chosen at build time with `--dart-define=BOBIX_FLAVOR=dev|prod`; the default, and anything
/// else, is `prod`, so a build made without the flag cannot talk plain.
enum Flavor { dev, prod }

const _flavorName = String.fromEnvironment(
  'BOBIX_FLAVOR',
  defaultValue: 'prod',
);

Flavor flavorOf(String name) => name == 'dev' ? Flavor.dev : Flavor.prod;

final flavorProvider = Provider<Flavor>((ref) => flavorOf(_flavorName));

/// Whether the app may connect to [url] in [flavor]: the shared crate's rule first (`wss://`, or `ws://` to localhost,
/// no userinfo or odd characters), then `prod` refuses anything that is not `wss://`.
Future<bool> relayAllowed(String url, Flavor flavor) async {
  if (!await relayUrlOk(url: url)) return false;
  return flavor == Flavor.dev || url.trim().toLowerCase().startsWith('wss://');
}
