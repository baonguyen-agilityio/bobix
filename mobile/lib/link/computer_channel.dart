import 'dart:typed_data';

import '../src/rust/api/messages.dart';
import '../src/rust/api/relay.dart';

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

/// The two directions between this phone and one computer. The counters, the receive rules and the version check are the
/// crate's `Link` (through [PhoneLink]); this only stores what it says to store: a send reservation before the packet
/// leaves, and the receive state after a packet was accepted. [onChange] is awaited for each.
class ComputerChannel {
  ComputerChannel({
    required Uint8List boxSecret,
    required Uint8List desktopBoxPublic,
    required this.recvState,
    required BigInt eventReservation,
    required this.onChange,
    required int nowMs,
  }) : reservation = eventReservation,
       _link = PhoneLink.create(
         boxSecret: boxSecret,
         desktopBoxPublic: desktopBoxPublic,
         eventReservation: eventReservation,
         nowUnixMs: BigInt.from(nowMs),
         recvState: recvState,
       );

  String recvState;
  BigInt reservation;
  Future<void> Function() onChange;
  final PhoneLink _link;

  Future<void> _store(BigInt? reserve) async {
    if (reserve == null) return;
    reservation = reserve;
    await onChange();
  }

  /// The next counter, for a message that carries its own (the pairing `hello`); its reservation is stored first.
  Future<BigInt> nextCounter() async {
    final t = _link.takeCounter();
    await _store(t.reserve);
    return t.counter;
  }

  Future<Uint8List> seal(Uint8List plaintext) async {
    final sealed = _link.seal(plaintext: plaintext);
    await _store(sealed.reserve);
    return sealed.packet;
  }

  /// Seals with a counter taken by [nextCounter]: a signed command has its packet counter inside the signature.
  Uint8List sealWith(BigInt counter, Uint8List plaintext) =>
      _link.sealWith(counter: counter, plaintext: plaintext);

  Future<Opened> open(Uint8List packet) async {
    switch (_link.open(packet: packet)) {
      case OpenedPacket_Message(:final field0):
        recvState = _link.recvState();
        await onChange();
        return OpenedMessage(field0);
      case OpenedPacket_PeerMustUpdate():
        return const OpenedVersion(phoneMustUpdate: false);
      case OpenedPacket_WeMustUpdate():
        return const OpenedVersion(phoneMustUpdate: true);
      case OpenedPacket_Dropped():
        return const OpenedDropped();
    }
  }
}
