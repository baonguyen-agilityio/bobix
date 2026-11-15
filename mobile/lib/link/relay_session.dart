import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:web_socket_channel/web_socket_channel.dart';

import '../src/rust/api/core.dart';
import 'link_state.dart';

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
/// the address the phone connected to (the relay refuses a signature made for another address).
class RelaySession {
  RelaySession._(this._socket, this.frames);

  final RelaySocket _socket;

  /// Frames after the login, as decoded JSON. Ends when the connection does.
  final Stream<Map<String, dynamic>> frames;

  static Future<RelaySession> open({
    required String url,
    required DeviceIdentity identity,
    required ProtocolVersions versions,
    required RelayConnector connect,
    Duration challengeWithin = const Duration(seconds: 10),
  }) async {
    final socket = await connect(url);
    final queue = StreamController<Map<String, dynamic>>();
    final sub = socket.frames.listen(
      (text) {
        try {
          final j = jsonDecode(text);
          if (j is Map<String, dynamic>) queue.add(j);
        } on FormatException {
          // Not a frame: ignored, as the relay would ignore ours.
        }
      },
      onError: queue.addError,
      onDone: queue.close,
    );
    final it = StreamIterator<Map<String, dynamic>>(queue.stream);
    Future<void> fail() async {
      await sub.cancel();
      await socket.close();
    }

    try {
      if (!await it.moveNext().timeout(challengeWithin) ||
          it.current['type'] != 'challenge') {
        throw const SocketException('the relay sent no challenge');
      }
      final nonce = it.current['nonce'] as String;
      final message = await authBytes(relayUrl: url, challenge: nonce);
      if (message == null) throw const SocketException('bad relay address');
      final sig = await signWithAuth(
        authSeed: identity.authSeed,
        boxSecret: identity.boxSecret,
        message: message,
      );
      socket.send(
        jsonEncode({
          'type': 'auth',
          'id': identity.deviceId,
          'key': b64(identity.authPublic),
          'relay': url,
          'sig': b64(sig),
          'versions': {'min': versions.min, 'max': versions.max},
        }),
      );
    } on Object {
      await fail();
      rethrow;
    }
    // The iterator took the first frame; the rest is read from the same queue.
    final rest = StreamController<Map<String, dynamic>>();
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
    return RelaySession._(socket, rest.stream);
  }

  void send(Map<String, Object?> frame) => _socket.send(jsonEncode(frame));

  Future<void> close() => _socket.close();
}
