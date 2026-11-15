/// The host of the relay Bobix itself runs. A pairing code that names any other relay shows its host on the scan screen
/// before the person goes on. Empty until the hosted relay exists (set with `--dart-define=BOBIX_DEFAULT_RELAY_HOST=...`),
/// so until then every relay is named.
const defaultRelayHost = String.fromEnvironment('BOBIX_DEFAULT_RELAY_HOST');

/// The host of a relay address (`wss://relay.example:443/ws` is `relay.example`), or null when there is none.
String? relayHost(String url) {
  final host = Uri.tryParse(url)?.host;
  return host == null || host.isEmpty ? null : host.toLowerCase();
}

/// Whether [url] is not the relay Bobix runs: the person must see which relay a code points at.
bool isCustomRelay(String url) {
  final host = relayHost(url);
  return host != null && (defaultRelayHost.isEmpty || host != defaultRelayHost);
}
