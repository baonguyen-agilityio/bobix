import 'dart:typed_data';

import 'push_platform.dart';

/// The receive state the app shares with the notification extension. The extension and the app are two processes: both
/// must take and commit counters through this one file, under its lock. If the app kept its own copy and wrote it back,
/// it would undo what the extension took and a pushed notice would show as new again.
class PushLinkStore {
  PushLinkStore(this._platform);
  final PushPlatform _platform;

  /// A computer was confirmed (pairing), or its key or version range changed.
  Future<void> setComputer(
    String id,
    Uint8List boxPublic, {
    int versionMin = 1,
    int versionMax = 1,
  }) => _platform.setComputer(
    id: id,
    boxPublic: boxPublic,
    versionMin: versionMin,
    versionMax: versionMax,
  );

  /// A computer was disconnected or revoked.
  Future<void> removeComputer(String id) => _platform.removeComputer(id);

  /// Opens any packet from a paired computer: the counter is checked, the packet opened and the counter committed under
  /// the lock. [PacketRefused] with `replayed` is normal and is dropped quietly.
  Future<OpenedPacket> openPacket(Uint8List packet) =>
      _platform.openPacket(packet);
}
