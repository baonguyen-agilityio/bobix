import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import '../config/flavor.dart';
import '../config/relay.dart';
import '../data/models.dart';
import '../keys/secure_keys.dart';
import '../src/rust/api/core.dart';
import '../src/rust/api/messages.dart';
import '../src/rust/api/relay.dart';
import 'computer_channel.dart';
import 'link_state.dart';
import 'pairing_service.dart' show updateIsOnThePhone;
import 'relay_session.dart';

/// A problem the person has to act on, other than the connection being down.
enum LinkProblem { none, updatePhone, updateComputer, invite }

class LinkStatus {
  const LinkStatus({
    this.conn = ConnState.relayDown,
    this.problem = LinkProblem.none,
    this.maybeRevoked = false,
    this.revokedByComputer = false,
    this.movedHost,
  });

  final ConnState conn;
  final LinkProblem problem;

  /// The relay said this phone is cut from a computer. It does not say which one; with one paired computer it is that one.
  final bool maybeRevoked;

  /// The computer itself said so (a sealed `revoked`): keys and data were erased.
  final bool revokedByComputer;

  /// The host of the relay the computer moved to, once the phone follows it (shown so the move is not silent).
  final String? movedHost;

  LinkStatus copyWith({
    ConnState? conn,
    LinkProblem? problem,
    bool? maybeRevoked,
    bool? revokedByComputer,
    String? movedHost,
  }) => LinkStatus(
    conn: conn ?? this.conn,
    problem: problem ?? this.problem,
    maybeRevoked: maybeRevoked ?? this.maybeRevoked,
    revokedByComputer: revokedByComputer ?? this.revokedByComputer,
    movedHost: movedHost ?? this.movedHost,
  );
}

/// The last `home` and when this phone got it.
class HomeSnapshot {
  const HomeSnapshot(this.home, this.receivedAtMs);
  final Map<String, dynamic> home;
  final int receivedAtMs;
}

/// How long to wait before the next try: 1 s, 2 s, 4 s ... up to 60 s, each with up to a quarter more at random so phones
/// do not all come back together.
Duration reconnectDelay(int attempt, double random) {
  final base = min(60.0, pow(2, max(0, attempt - 1)).toDouble());
  return Duration(
    milliseconds: (min(60.0, base * (1 + 0.25 * random)) * 1000).round(),
  );
}

/// The connection of this phone to its one computer, through the relay: logs in with the `auth` key, keeps the counters,
/// acknowledges every packet, syncs on connect and reconnects with backoff. It runs only while the app is in the foreground
/// ([resume] and [pause]) and never asks for biometrics.
class ComputerLink {
  ComputerLink({
    required this.computer,
    required this.keys,
    required this.store,
    required this.connect,
    required this.flavor,
    required this.identity,
    required this.versions,
    DateTime Function()? now,
    Random? random,
    Duration Function(int attempt, double random)? delay,
  }) : _now = now ?? DateTime.now,
       _random = random ?? Random.secure(),
       _delay = delay ?? reconnectDelay {
    channel = ComputerChannel(
      boxSecret: identity.boxSecret,
      desktopBoxPublic: computer.desktopBoxPublic,
      recvState: computer.recvState,
      eventReservation: computer.eventReservation,
      nowMs: _now().millisecondsSinceEpoch,
      onChange: _persistChannel,
    );
    final h = computer.home;
    if (h != null && computer.homeAt != null) {
      home = HomeSnapshot(
        jsonDecode(h) as Map<String, dynamic>,
        computer.homeAt!,
      );
    }
  }

  final PairedComputer computer;
  final SecureKeys keys;
  final LinkStateStore store;
  final RelayConnector connect;
  final Flavor flavor;
  final DeviceIdentity identity;
  final ProtocolVersions versions;
  final DateTime Function() _now;
  final Random _random;
  final Duration Function(int attempt, double random) _delay;
  late final ComputerChannel channel;

  LinkStatus status = const LinkStatus();
  HomeSnapshot? home;
  final _statusChanges = StreamController<LinkStatus>.broadcast();
  final _homeChanges = StreamController<HomeSnapshot>.broadcast();
  final _messages = StreamController<DesktopMessage>.broadcast();
  final _details = <String, Completer<Map<String, dynamic>>>{};

  Stream<LinkStatus> get statusChanges => _statusChanges.stream;
  Stream<HomeSnapshot> get homeChanges => _homeChanges.stream;

  /// Every message that was accepted (not dropped), in order.
  Stream<DesktopMessage> get messages => _messages.stream;

  /// Whole inputs, plans and questions: in memory only, never written to the stored state.
  final Map<String, Map<String, dynamic>> detailCache = {};

  bool _running = false;
  bool _gone = false;
  bool _moved = false;
  RelaySession? _session;
  Completer<void>? _wake;

  Future<void> _persistChannel() async {
    computer
      ..recvState = channel.recvState
      ..eventReservation = channel.reservation;
    await store.save(computer);
  }

  void _set(LinkStatus s) {
    status = s;
    if (!_statusChanges.isClosed) _statusChanges.add(s);
  }

  /// The app came to the foreground: connect, and keep connected until [pause].
  void resume() {
    if (_running || _gone) return;
    _running = true;
    unawaited(_loop());
  }

  /// The app left the foreground: close the connection and stop trying.
  Future<void> pause() async {
    _running = false;
    _wake?.complete();
    await _session?.close();
  }

  Future<void> dispose() async {
    await pause();
    await _statusChanges.close();
    await _homeChanges.close();
    await _messages.close();
  }

  Future<void> _loop() async {
    var attempt = 0;
    while (_running && !_gone) {
      var reachedRelay = false;
      try {
        final session = await RelaySession.open(
          url: computer.relay,
          identity: identity,
          versions: versions,
          connect: connect,
        );
        _session = session;
        if (!_running) {
          await session.close();
          break;
        }
        _set(status.copyWith(conn: ConnState.ok));
        session.send(await relayPeersFrame(allow: [computer.id]));
        await _sendSealed(
          await syncMessage(
            waiting: const [],
            versionMin: versions.min,
            versionMax: versions.max,
          ),
        );
        await for (final f in session.frames) {
          if (f is! RelayFrame_Error) reachedRelay = true;
          if (reachedRelay) attempt = 0;
          await _onFrame(session, f);
          if (_gone || !_running) break;
        }
        _session = null;
        await session.close();
        if (!_gone && _running && status.conn == ConnState.ok) {
          _set(status.copyWith(conn: ConnState.relayDown));
        }
      } on Object catch (e) {
        _session = null;
        _set(status.copyWith(conn: _unreachable(e)));
      }
      if (!_running || _gone || status.maybeRevoked) break;
      if (_moved) {
        _moved = false;
        attempt = 0;
        continue;
      }
      attempt++;
      await _sleep(_delay(attempt, _random.nextDouble()));
    }
    _running = false;
  }

  ConnState _unreachable(Object e) => whyUnreachable(e) == Unreachable.noNetwork
      ? ConnState.noNetwork
      : ConnState.relayDown;

  /// Waits, or less when [pause] or a moved relay asks to be tried now.
  Future<void> _sleep(Duration d) async {
    final wake = _wake = Completer<void>();
    await Future.any([Future<void>.delayed(d), wake.future]);
    _wake = null;
  }

  Future<void> _sendSealed(Uint8List plaintext) async {
    final session = _session;
    if (session == null) throw const SocketLost();
    final out = await relaySendFrame(
      to: computer.id,
      packet: await channel.seal(plaintext),
      ttlSeconds: 600,
    );
    session.send(out.text);
  }

  Future<void> _onFrame(RelaySession session, RelayFrame f) async {
    switch (f) {
      case RelayFrame_Presence(:final peer, :final state)
          when peer == computer.id:
        _set(
          status.copyWith(
            conn: switch (state) {
              PeerState.asleep => ConnState.asleep,
              PeerState.offline => ConnState.offline,
              PeerState.online => ConnState.ok,
            },
          ),
        );
      case RelayFrame_Error(:final code, :final who):
        _onError(code, who);
      case RelayFrame_Msg(:final pid, :final from, :final packet):
        // Acknowledged on arrival, before it is looked at: the relay stops holding it.
        session.send(await relayAckFrame(pid: pid));
        if (from != computer.id) return;
        await _onPacket(packet);
      default:
        break;
    }
  }

  void _onError(RelayError code, RelaySide? who) {
    switch (code) {
      case RelayError.revoked:
        _set(status.copyWith(maybeRevoked: true));
      case RelayError.update:
        _set(
          status.copyWith(
            problem: updateIsOnThePhone(who)
                ? LinkProblem.updatePhone
                : LinkProblem.updateComputer,
          ),
        );
      case RelayError.invite:
        _set(status.copyWith(problem: LinkProblem.invite));
      default:
        break;
    }
  }

  Future<void> _onPacket(Uint8List packet) async {
    switch (await channel.open(packet)) {
      case OpenedDropped():
        return;
      case OpenedVersion(:final phoneMustUpdate):
        _set(
          status.copyWith(
            problem: phoneMustUpdate
                ? LinkProblem.updatePhone
                : LinkProblem.updateComputer,
          ),
        );
      case OpenedMessage(:final message):
        await _onMessage(message);
    }
  }

  Future<void> _onMessage(DesktopMessage m) async {
    if (!_messages.isClosed) _messages.add(m);
    final j = jsonDecode(m.json) as Map<String, dynamic>;
    switch (m.kind) {
      case 'home':
        final snap = HomeSnapshot(j, _now().millisecondsSinceEpoch);
        home = snap;
        computer
          ..home = m.json
          ..homeAt = snap.receivedAtMs;
        await store.save(computer);
        if (!_homeChanges.isClosed) _homeChanges.add(snap);
        final state = (j['computer'] as Map<String, dynamic>)['state'];
        if (state == 'asleep' && status.conn == ConnState.ok) {
          _set(status.copyWith(conn: ConnState.asleep));
        } else if (state == 'online' && status.conn == ConnState.asleep) {
          _set(status.copyWith(conn: ConnState.ok));
        }
      case 'detail':
        final id = j['id'] as String;
        detailCache[id] = j;
        _details.remove(id)?.complete(j);
      case 'revoked':
        await _eraseEverything();
        _set(status.copyWith(revokedByComputer: true));
      case 'relayMoved':
        final url = j['url'] as String;
        if (await relayAllowed(url, flavor) && url != computer.relay) {
          computer.relay = url;
          await store.save(computer);
          // The old connection ends; the loop opens the new relay at once.
          _moved = true;
          _set(status.copyWith(movedHost: relayHost(url)));
          await _session?.close();
        }
    }
  }

  Future<void> _eraseEverything() async {
    _gone = true;
    _running = false;
    await keys.eraseAll();
    await store.erase();
    await _session?.close();
  }

  /// The whole content of a request, plan or question, from the computer (in memory only). Null when the phone is not
  /// connected or the computer did not answer.
  Future<Map<String, dynamic>?> detail(String id) async {
    final cached = detailCache[id];
    if (cached != null) return cached;
    final waiter = _details.putIfAbsent(id, Completer.new);
    try {
      await _sendSealed(await detailRequestMessage(id: id));
      return await waiter.future.timeout(const Duration(seconds: 10));
    } on Object {
      _details.remove(id);
      return null;
    }
  }

  /// The person erases this phone's pairing (Pair again, or a disconnect): keys, state, everything.
  Future<void> erase() => _eraseEverything();
}

class SocketLost implements Exception {
  const SocketLost();
}
