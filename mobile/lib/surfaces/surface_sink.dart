import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Where the list and the pet images for the surfaces outside the app go. Android only: it is a platform channel
/// (`dev.bobix.phone/surfaces`, Kotlin in `surfaces/SurfacesChannel.kt`); elsewhere nothing is written.
abstract interface class SurfaceSink {
  Future<void> writeList(String json);
  Future<void> writePet(String name, Uint8List png);

  /// Nothing is paired any more: every surface shows that.
  Future<void> clear();
}

class ChannelSurfaceSink implements SurfaceSink {
  ChannelSurfaceSink({MethodChannel? channel})
    : _channel = channel ?? const MethodChannel('dev.bobix.phone/surfaces');

  final MethodChannel _channel;

  @override
  Future<void> writeList(String json) =>
      _channel.invokeMethod<void>('writeList', {'json': json});

  @override
  Future<void> writePet(String name, Uint8List png) =>
      _channel.invokeMethod<void>('writePet', {'name': name, 'png': png});

  @override
  Future<void> clear() => _channel.invokeMethod<void>('clear');
}

class NoSurfaceSink implements SurfaceSink {
  const NoSurfaceSink();

  @override
  Future<void> writeList(String json) async {}

  @override
  Future<void> writePet(String name, Uint8List png) async {}

  @override
  Future<void> clear() async {}
}

/// The channel sink on Android, nothing elsewhere (and in tests, which give their own).
SurfaceSink platformSurfaceSink() =>
    defaultTargetPlatform == TargetPlatform.android
    ? ChannelSurfaceSink()
    : const NoSurfaceSink();
