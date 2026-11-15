import 'dart:typed_data';

import '../src/rust/api/core.dart';
import '../src/rust/api/messages.dart';

/// What opening a packet from the computer came to.
sealed class Opened {
  const Opened();
}

class OpenedMessage extends Opened {
  const OpenedMessage(this.message);
  final DesktopMessage message;
}

/// A late, replayed, reflected, forged or unreadable packet: no reply, nothing changes, the pairing stays.
class OpenedDropped extends Opened {
  const OpenedDropped();
}

/// The packet's version is outside what this app understands. [phoneMustUpdate] says which side is to update.
class OpenedVersion extends Opened {
  const OpenedVersion({required this.phoneMustUpdate});
  final bool phoneMustUpdate;
}

/// The two directions between this phone and one computer: seals what the phone sends (on the `event` stream) and opens
/// what the computer sends, keeping both counters. [onChange] is awaited whenever one has to be stored, before the packet
/// leaves (a send reservation) or after it was accepted (the receive state).
class ComputerChannel {
  ComputerChannel({
    required this.boxSecret,
    required this.desktopBoxPublic,
    required this.versions,
    required this.recvState,
    required BigInt eventReservation,
    required this.onChange,
    required int nowMs,
  }) : reservation = eventReservation,
       _event = SendCounter.resume(
         storedReservation: eventReservation,
         nowUnixMs: BigInt.from(nowMs),
       );

  /// The `box` secret.
  final Uint8List boxSecret;
  final Uint8List desktopBoxPublic;
  final ProtocolVersions versions;
  String recvState;
  BigInt reservation;
  Future<void> Function() onChange;
  final SendCounter _event;

  /// Takes the next counter and stores its reservation first, when it asks for one.
  Future<BigInt> nextCounter() async {
    final t = _event.take();
    if (t.reserve != null) {
      reservation = t.reserve!;
      await onChange();
    }
    return t.counter;
  }

  Future<Uint8List> seal(Uint8List plaintext) async => sealPacket(
    version: versions.current,
    direction: LinkDirection.toDesktop,
    stream: LinkStream.event,
    counter: await nextCounter(),
    senderBoxSecret: boxSecret,
    recipientBoxPublic: desktopBoxPublic,
    plaintext: plaintext,
  );

  Future<Opened> open(Uint8List packet) async {
    try {
      final h = await peekPacket(
        receiverSends: LinkDirection.toDesktop,
        packet: packet,
      );
      if (!await recvCheck(
        state: recvState,
        stream: h.stream,
        counter: h.counter,
      )) {
        return const OpenedDropped();
      }
      final plain = await openPacket(
        receiverBoxSecret: boxSecret,
        senderBoxPublic: desktopBoxPublic,
        packet: packet,
      );
      // The version is judged only after the packet authenticated: an unauthenticated byte never says who must update.
      await checkVersion(
        min: versions.min,
        max: versions.max,
        version: h.version,
      );
      recvState = await recvCommit(
        state: recvState,
        stream: h.stream,
        counter: h.counter,
      );
      await onChange();
      try {
        return OpenedMessage(await parseDesktopMessage(bytes: plain));
      } on CoreError {
        return const OpenedDropped();
      }
    } on CoreError catch (e) {
      return switch (e) {
        CoreError.peerMustUpdate => const OpenedVersion(phoneMustUpdate: false),
        CoreError.weMustUpdate => const OpenedVersion(phoneMustUpdate: true),
        _ => const OpenedDropped(),
      };
    }
  }
}
