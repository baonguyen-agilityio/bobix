import 'dart:async';
import 'dart:io';

import 'package:web_socket_channel/web_socket_channel.dart';

import '../src/rust/api/core.dart';
import '../src/rust/api/relay.dart';

/// A text WebSocket to a relay. A seam for tests; [WebSocketRelaySocket] is the real one.
abstract interface class RelaySocket {
  /// Text frames until the connection ends (then the stream is done, or fails).
  Stream<String> get frames;
  void send(String text);
  Future<void> close();
}

typedef RelayConnector = Future<RelaySocket> Function(String url);

class WebSocketRelaySocket implements RelaySocket {
  WebSocketRelaySocket._(this._channel);

  final WebSocketChannel _channel;

  static Future<RelaySocket> connect(String url) async {
    final channel = WebSocketChannel.connect(Uri.parse(url));
    try {
      await channel.ready.timeout(const Duration(seconds: 10));
    } on Object {
      unawaited(channel.sink.close());
      rethrow;
    }
    return WebSocketRelaySocket._(channel);
  }

  @override
  Stream<String> get frames =>
      _channel.stream.where((m) => m is String).cast<String>();

  @override
  void send(String text) => _channel.sink.add(text);

  @override
  Future<void> close() async {
    await _channel.sink.close();
  }
}

/// Why the phone could not reach the relay: the phone has no network at all, or the relay does not answer.
enum Unreachable { noNetwork, relayDown }

Unreachable whyUnreachable(Object error) {
  // "Failed host lookup" and "Network is unreachable" are what a phone without a network reports.
  final text = '$error'.toLowerCase();
  final offline =
      error is SocketException &&
      (text.contains('failed host lookup') ||
          text.contains('network is unreachable') ||
          text.contains('no route to host') ||
          text.contains('not connected to the internet'));
  return offline ? Unreachable.noNetwork : Unreachable.relayDown;
}

/// One open connection to a relay, logged in with the `auth` key: the relay's challenge is answered with a signature over
/// the address the phone connected to (the relay refuses a signature made for another address). Frames are built and read
/// by the crate (through the bridge); this moves their text.
class RelaySession {
  RelaySession._(this._socket, this.frames, this.relayId);

  final RelaySocket _socket;

  /// The id the relay says its push grants must name (`RELAY_ID`), from its challenge. Null from an older relay.
  final String? relayId;

  /// Frames after the login. Ends when the connection does. A text that is not a frame is skipped.
  final Stream<RelayFrame> frames;

  static Future<RelaySession> open({
    required String url,
    required DeviceIdentity identity,
    required ProtocolVersions versions,
    required RelayConnector connect,
    Duration challengeWithin = const Duration(seconds: 10),
  }) async {
    final socket = await connect(url);
    // In order: each text is parsed before the next one is looked at.
    final parsed = socket.frames
        .asyncMap<RelayFrame?>((text) async {
          try {
            return await parseRelayFrame(text: text);
          } on CoreError {
            return null; // not a frame: ignored, as the relay would ignore ours
          }
        })
        .where((f) => f != null)
        .cast<RelayFrame>();
    final it = StreamIterator<RelayFrame>(parsed);
    String? relayId;
    try {
      if (!await it.moveNext().timeout(challengeWithin) ||
          it.current is! RelayFrame_Challenge) {
        throw const SocketException('the relay sent no challenge');
      }
      relayId = (it.current as RelayFrame_Challenge).relayId;
      socket.send(
        await relayAuthFrame(
          authSeed: identity.authSeed,
          boxSecret: identity.boxSecret,
          relayUrl: url,
          challenge: (it.current as RelayFrame_Challenge).nonce,
          versionMin: versions.min,
          versionMax: versions.max,
        ),
      );
    } on Object {
      await it.cancel();
      await socket.close();
      rethrow;
    }
    // The iterator took the first frame; the rest is read from the same stream.
    final rest = StreamController<RelayFrame>();
    unawaited(() async {
      try {
        while (await it.moveNext()) {
          rest.add(it.current);
        }
      } on Object catch (e) {
        rest.addError(e);
      }
      await rest.close();
    }());
    return RelaySession._(socket, rest.stream, relayId);
  }

  /// Sends a frame the crate built.
  void send(String frame) => _socket.send(frame);

  Future<void> close() => _socket.close();
}
