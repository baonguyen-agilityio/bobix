import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import '../config/flavor.dart';
import '../keys/secure_keys.dart';
import '../src/rust/api/core.dart';
import '../src/rust/api/messages.dart';
import 'computer_channel.dart';
import 'link_state.dart';
import 'relay_session.dart';

/// Why a pairing did not happen. Each has its own message on screen and a way out.
enum PairFailure {
  /// The scanned text is not a Bobix pairing code.
  notBobix,

  /// The relay in the code is not `wss://` (or, in the `dev` flavor, `ws://` to this computer).
  insecureRelay,
  codeExpired,
  codeUsed,
  relayUnreachable,
  computerSaidNo,

  /// Two phones answered the same code: the computer cancelled the pairing.
  twoPhones,
  noStrongBiometrics,
  keyFailed,

  /// A pairing, or what may hold one, is already on this phone: pairing never runs over it.
  alreadyPaired,

  /// One side speaks a version the other cannot read; [PairException.phoneMustUpdate] says which side is to update.
  updateNeeded,
  inviteNeeded,
  cancelled,
}

class PairException implements Exception {
  const PairException(this.failure, {this.phoneMustUpdate = false});
  final PairFailure failure;
  final bool phoneMustUpdate;

  @override
  String toString() => 'PairException($failure)';
}

String _pid(Random r) => List.generate(
  8,
  (_) => r.nextInt(256).toRadixString(16).padLeft(2, '0'),
).join();

/// What the person chose on the "name and rights" screen.
class PairRequest {
  const PairRequest({
    required this.uri,
    required this.phoneName,
    required this.rights,
  });
  final String uri;
  final String phoneName;
  final PhoneRights rights;
}

/// A running pairing: [result] completes with the paired computer, or fails with a [PairException]. [cancel] ends it at
/// any time before the computer's answer, and leaves no key and no state behind.
class PairingAttempt {
  PairingAttempt._(this.result, this._cancel);
  final Future<PairedComputer> result;
  final void Function() _cancel;
  void cancel() => _cancel();
}

/// Pairs this phone with the computer that shows a code. Nothing is stored before the computer's `paired` arrived: a
/// pairing that fails or is cancelled leaves nothing behind.
class PairingService {
  PairingService({
    required this.keys,
    required this.store,
    required this.connect,
    required this.flavor,
    this.waitForComputer = const Duration(minutes: 3),
    DateTime Function()? now,
    Random? random,
  }) : _now = now ?? DateTime.now,
       _random = random ?? Random.secure();

  final SecureKeys keys;
  final LinkStateStore store;
  final RelayConnector connect;
  final Flavor flavor;

  /// How long the phone waits for the person to confirm on the computer (a code lasts two minutes at the relay).
  final Duration waitForComputer;
  final DateTime Function() _now;
  final Random _random;

  /// Checks a scanned text before anything else: a Bobix code, and a relay this build may use.
  Future<PairingInfo> inspect(String uri) async {
    final PairingInfo info;
    try {
      info = await parsePairingCode(uri: uri);
    } on CoreError catch (e) {
      throw PairException(
        e == CoreError.insecureRelay
            ? PairFailure.insecureRelay
            : PairFailure.notBobix,
      );
    }
    if (!await relayAllowed(info.relay, flavor)) {
      throw const PairException(PairFailure.insecureRelay);
    }
    return info;
  }

  PairingAttempt start(PairRequest request) {
    var cancelled = false;
    RelaySession? session;
    final done = Completer<PairedComputer>();
    void cancel() {
      cancelled = true;
      unawaited(session?.close());
    }

    unawaited(() async {
      try {
        done.complete(
          await _pair(request, () => cancelled, (s) => session = s),
        );
      } on Object catch (e) {
        done.completeError(
          cancelled
              ? const PairException(PairFailure.cancelled)
              : e is PairException
              ? e
              : const PairException(PairFailure.relayUnreachable),
        );
      }
    }());
    return PairingAttempt._(done.future, cancel);
  }

  Future<PairedComputer> _pair(
    PairRequest request,
    bool Function() cancelled,
    void Function(RelaySession) onSession,
  ) async {
    final info = await inspect(request.uri);
    if (!await keys.strongBiometrics()) {
      throw const PairException(PairFailure.noStrongBiometrics);
    }
    // Never over a pairing: this one would replace its keys. A state that cannot be read counts as one that may exist.
    try {
      if (await store.exists() || await keys.secret(SecureKeys.auth) != null) {
        throw const PairException(PairFailure.alreadyPaired);
      }
    } on SecureKeysException {
      throw const PairException(PairFailure.keyFailed);
    }
    final identity = await generateIdentity();
    // The signing key is made only when there is none, and removed on a failure only when this attempt made it.
    var madeKey = false;
    final Uint8List signPublic;
    try {
      final existing = await keys.signingPublicKey();
      signPublic = existing ?? await keys.createSigningKey();
      madeKey = existing == null;
    } on SecureKeysException {
      throw const PairException(PairFailure.keyFailed);
    }
    try {
      return await _run(
        request,
        cancelled,
        onSession,
        info,
        identity,
        signPublic,
      );
    } on Object {
      if (madeKey) await keys.deleteSigningKey().catchError((Object _) {});
      rethrow;
    }
  }

  Future<PairedComputer> _run(
    PairRequest request,
    bool Function() cancelled,
    void Function(RelaySession) onSession,
    PairingInfo info,
    DeviceIdentity identity,
    Uint8List signPublic,
  ) async {
    final versions = await protocolVersions();
    final channel = ComputerChannel(
      boxSecret: identity.boxSecret,
      desktopBoxPublic: info.desktopBoxPublic,
      versions: versions,
      recvState: await recvStateNew(),
      eventReservation: BigInt.zero,
      onChange: () async {},
      nowMs: _now().millisecondsSinceEpoch,
    );
    final RelaySession session;
    try {
      session = await RelaySession.open(
        url: info.relay,
        identity: identity,
        versions: versions,
        connect: connect,
      );
    } on Object {
      throw const PairException(PairFailure.relayUnreachable);
    }
    onSession(session);
    final it = StreamIterator(session.frames);
    try {
      final body = await buildHelloBody(
        pairingUri: request.uri,
        authSeed: identity.authSeed,
        boxSecret: identity.boxSecret,
        signPublicKey: signPublic,
        name: request.phoneName,
        rights: request.rights,
        versionMin: versions.min,
        versionMax: versions.max,
        counter: await channel.nextCounter(),
      );
      session
        ..send({
          'type': 'peers',
          'allow': [info.desktopDeviceId],
        })
        ..send({
          'type': 'pairSend',
          'relayToken': b64(info.relayToken),
          'body': b64(body),
        });
      final answer = await _awaitAnswer(it, session, channel, info, cancelled);
      final granted = answer.rights;
      await keys.putSecret(SecureKeys.auth, identity.authSeed);
      await keys.putSecret(SecureKeys.box, identity.boxSecret);
      final computer = PairedComputer(
        id: info.desktopDeviceId,
        name: answer.name,
        relay: info.relay,
        phoneName: request.phoneName,
        desktopBoxPublic: info.desktopBoxPublic,
        rights: granted,
        recvState: channel.recvState,
        eventReservation: channel.reservation,
      );
      await store.save(computer);
      channel.onChange = () async {
        computer
          ..recvState = channel.recvState
          ..eventReservation = channel.reservation;
        await store.save(computer);
      };
      // Pairing ends when the computer has the acknowledgement: wait for the relay to say it was passed on.
      final pid = _pid(_random);
      session.send({
        'type': 'send',
        'pid': pid,
        'to': info.desktopDeviceId,
        'ttl': 600,
        'body': b64(await channel.seal(await pairedAckMessage())),
      });
      await _awaitDelivery(it, pid);
      return computer;
    } finally {
      unawaited(it.cancel());
      await session.close();
    }
  }

  Future<({String name, PhoneRights rights})> _awaitAnswer(
    StreamIterator<Map<String, dynamic>> it,
    RelaySession session,
    ComputerChannel channel,
    PairingInfo info,
    bool Function() cancelled,
  ) async {
    final deadline = _now().add(waitForComputer);
    while (true) {
      final left = deadline.difference(_now());
      if (left.isNegative || cancelled()) {
        throw PairException(
          cancelled() ? PairFailure.cancelled : PairFailure.codeExpired,
        );
      }
      final bool more;
      try {
        more = await it.moveNext().timeout(left);
      } on TimeoutException {
        throw const PairException(PairFailure.codeExpired);
      }
      if (!more) {
        throw PairException(
          cancelled() ? PairFailure.cancelled : PairFailure.relayUnreachable,
        );
      }
      final f = it.current;
      switch (f['type']) {
        case 'error':
          throw _failureOf(f);
        case 'msg':
          session.send({'type': 'ack', 'pid': f['pid']});
          if (f['from'] != info.desktopDeviceId) continue;
          final opened = await channel.open(b64d(f['body'] as String));
          switch (opened) {
            case OpenedVersion(:final phoneMustUpdate):
              throw PairException(
                PairFailure.updateNeeded,
                phoneMustUpdate: phoneMustUpdate,
              );
            case OpenedDropped():
              continue;
            case OpenedMessage(:final message):
              final j = jsonDecode(message.json) as Map<String, dynamic>;
              if (message.kind == 'paired') {
                return (
                  name: j['computer'] as String,
                  rights: rightsFromJson(j['rights'] as Map<String, dynamic>),
                );
              }
              if (message.kind == 'revoked') {
                throw PairException(
                  j['why'] == 'pairing cancelled'
                      ? PairFailure.twoPhones
                      : PairFailure.computerSaidNo,
                );
              }
          }
      }
    }
  }

  Future<void> _awaitDelivery(
    StreamIterator<Map<String, dynamic>> it,
    String pid,
  ) async {
    try {
      await (() async {
        while (await it.moveNext()) {
          final f = it.current;
          if (f['pid'] == pid &&
              (f['type'] == 'delivered' || f['type'] == 'queued')) {
            return;
          }
        }
      })().timeout(const Duration(seconds: 5));
    } on Object {
      // Not confirmed in time: the acknowledgement was sent, and the computer asks for it again by syncing on connect.
    }
  }
}

PairException _failureOf(Map<String, dynamic> f) => switch (f['code']) {
  'pairExpired' => const PairException(PairFailure.codeExpired),
  'noPeer' => const PairException(PairFailure.codeUsed),
  'revoked' => const PairException(PairFailure.computerSaidNo),
  'invite' => const PairException(PairFailure.inviteNeeded),
  'update' => PairException(
    PairFailure.updateNeeded,
    phoneMustUpdate: updateIsOnThePhone(f),
  ),
  _ => const PairException(PairFailure.relayUnreachable),
};

/// An `error { update }` names who is to update in `who`; the relay does not send it yet, so the format is an assumption
/// (see `relay_frames.rs`): `desktop` means the computer, anything else (or nothing) the phone.
bool updateIsOnThePhone(Map<String, dynamic> errorFrame) =>
    errorFrame['who'] != 'desktop';
