import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import '../config/flavor.dart';
import '../config/relay.dart';
import '../data/models.dart';
import '../keys/secure_keys.dart';
import '../push/push_registration.dart';
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
    this.revokedAtMs,
    this.movedHost,
    this.pushRefusedFor,
  });

  final ConnState conn;
  final LinkProblem problem;

  /// The relay said this phone is cut from a computer. It does not say which one; with one paired computer it is that one.
  final bool maybeRevoked;

  /// The computer itself said so (a sealed `revoked`): keys and data were erased.
  final bool revokedByComputer;

  /// When this phone learned of that `revoked` (Unix ms). Only a time: kept in memory for the "disconnected" screen.
  final int? revokedAtMs;

  /// The host of the relay the computer moved to, once the phone follows it (shown so the move is not silent).
  final String? movedHost;

  /// The relay refused this phone's push grant, which named this relay id: notifications will not arrive. Cleared when the
  /// next connection opens (the grant is made again, and refused again if it is still wrong).
  final String? pushRefusedFor;

  LinkStatus copyWith({
    ConnState? conn,
    LinkProblem? problem,
    bool? maybeRevoked,
    bool? revokedByComputer,
    int? revokedAtMs,
    String? movedHost,
    String? pushRefusedFor,
    bool clearPushRefused = false,
  }) => LinkStatus(
    conn: conn ?? this.conn,
    problem: problem ?? this.problem,
    maybeRevoked: maybeRevoked ?? this.maybeRevoked,
    revokedByComputer: revokedByComputer ?? this.revokedByComputer,
    revokedAtMs: revokedAtMs ?? this.revokedAtMs,
    movedHost: movedHost ?? this.movedHost,
    pushRefusedFor: clearPushRefused
        ? null
        : (pushRefusedFor ?? this.pushRefusedFor),
  );
}

/// The last `home` and when this phone got it.
class HomeSnapshot {
  const HomeSnapshot(this.home, this.receivedAtMs);
  final Map<String, dynamic> home;
  final int receivedAtMs;
}

/// Who closed an item, as far as this phone can tell.
enum Closer {
  /// This phone sent the answer.
  me,
  computer,
  otherPhone,
  expired,
  released,

  /// Nothing says (an older computer, and not this phone's own answer): the phone may only guess.
  unknown,
}

/// A `closed` notice: the item, its session, and who closed it (`by` as the computer sent it: `kind` is `computer`, `phone`
/// with a `phone` id, `expired` or `released`; null from an older computer).
/// Whether [id] can be a relay's id: 1 to 253 printable ASCII characters (the crate's `valid_relay_id`).
bool validRelayId(String id) =>
    id.isNotEmpty &&
    id.length <= 253 &&
    id.codeUnits.every((c) => c >= 0x21 && c <= 0x7E);

class ClosedNotice {
  const ClosedNotice(this.ref, this.session, this.by);
  final String ref, session;
  final Map<String, dynamic>? by;
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
    this.push,
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

  /// Registers this phone's push tokens with the relay each time a connection is up. Null in tests that do not need it.
  final PushRegistration? push;
  final DateTime Function() _now;
  final Random _random;
  final Duration Function(int attempt, double random) _delay;
  late final ComputerChannel channel;

  LinkStatus status = const LinkStatus();
  HomeSnapshot? home;

  /// A `home` arrived over the connection that is up. False from the moment a connection opens until its first `home`, and
  /// while the one kept from an earlier run is all there is.
  bool homeLive = false;
  final _statusChanges = StreamController<LinkStatus>.broadcast();
  final _homeChanges = StreamController<HomeSnapshot>.broadcast();
  final _messages = StreamController<DesktopMessage>.broadcast();
  final _relayEvents = StreamController<RelayFrame>.broadcast();
  final _results = StreamController<CommandResultInfo>.broadcast();
  final _closedNotices = StreamController<ClosedNotice>.broadcast();
  final _details = <String, Completer<Map<String, dynamic>>>{};

  Stream<LinkStatus> get statusChanges => _statusChanges.stream;
  Stream<HomeSnapshot> get homeChanges => _homeChanges.stream;

  /// Every message that was accepted (not dropped), in order.
  Stream<DesktopMessage> get messages => _messages.stream;

  /// The relay's own answers to packets this phone sent: `queued`, `delivered`, `dropped`, `withdrawn`, `tooLate`.
  Stream<RelayFrame> get relayEvents => _relayEvents.stream;

  /// Every `closed` notice of the computer.
  Stream<ClosedNotice> get closedNotices => _closedNotices.stream;

  /// Every `result` of the computer, whatever it answers.
  Stream<CommandResultInfo> get results => _results.stream;

  /// When this phone first saw each waiting item (Unix ms), to say how long it has been waiting.
  final Map<String, int> waitingSince = {};

  /// Items this phone sent an answer for (the newest 64), and who the computer said closed an item. Memory only. They
  /// tell this phone's own answer from the computer's, so it never calls its own answer "answered at the computer".
  final Set<String> answeredHere = {};
  final Map<String, Map<String, dynamic>?> _closedBy = {};

  void answeredItem(String id) {
    answeredHere
      ..remove(id)
      ..add(id);
    if (answeredHere.length > 64) answeredHere.remove(answeredHere.first);
  }

  /// Who closed [id]: the computer's notice first, then this phone's own answer, else [Closer.unknown].
  Closer closerOf(String id) {
    final by = _closedBy[id];
    if (by != null) {
      return switch (by['kind']) {
        'computer' => Closer.computer,
        'phone' =>
          by['phone'] == identity.deviceId ? Closer.me : Closer.otherPhone,
        'expired' => Closer.expired,
        'released' => Closer.released,
        _ => Closer.unknown,
      };
    }
    return answeredHere.contains(id) ? Closer.me : Closer.unknown;
  }

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
    await _save();
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

  /// Kept on disk, unless this link has ended: a link left over after its pairing ended never writes its state back.
  Future<void> _save() async {
    if (_gone) return;
    await store.save(computer);
  }

  /// Ends this link for good: it never connects or writes its state again, and what it was writing is on disk when this
  /// returns.
  Future<void> dispose() async {
    _gone = true;
    await pause();
    _pushForget();
    await _statusChanges.close();
    await _homeChanges.close();
    await _messages.close();
    await _relayEvents.close();
    await _results.close();
    await _closedNotices.close();
    await store.flush();
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
        homeLive = false;
        _set(status.copyWith(conn: ConnState.ok, clearPushRefused: true));
        _pushUp(session);
        session.send(await relayPeersFrame(allow: [computer.id]));
        await _sendSealed(
          await syncMessage(
            waiting: computer.pending.toList(),
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
        _pushDown();
        await session.close();
        if (!_gone && _running && status.conn == ConnState.ok) {
          _set(status.copyWith(conn: ConnState.relayDown));
        }
      } on Object catch (e) {
        _session = null;
        _pushDown();
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

  /// The relay id [push] has this link registered under while a connection is up, and the last one it used (to forget).
  String? _pushId, _pushUsed;

  /// A connection is up: the push tokens go to the relay over it, each with a fresh grant. A failure here never ends the
  /// connection: the next one, or the daily renewal, tries again.
  void _pushUp(RelaySession session) {
    // The id the relay announced; an older relay announces none, and its address's host is the best guess. An id that is
    // not one a relay can have (the rule the invite codes use) is not signed over: the grant would name a text the relay
    // never chose, and the address's host stands in.
    final announced = session.relayId;
    final p = push,
        id = announced != null && validRelayId(announced)
            ? announced
            : relayHost(computer.relay);
    if (p == null || id == null) return;
    _pushId = _pushUsed = id;
    unawaited(() async {
      try {
        await p.connected(id, (frame) async => session.send(jsonEncode(frame)));
      } on Object {
        // Push is not available (no platform channel, a closed socket): nothing is lost, it registers again later.
      }
    }());
  }

  /// The connection ended: nothing is sent to the relay until it is up again.
  void _pushDown() {
    final id = _pushId;
    _pushId = null;
    if (id != null) push?.disconnected(id);
  }

  /// This link no longer belongs to the relay (it moved, or the pairing ended): its grants are not renewed.
  void _pushForget() {
    final id = _pushUsed;
    if (id != null) push?.forget(id);
    _pushUsed = null;
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

  /// Seals and sends a message; the pid names it in the relay's answers (`queued`, `delivered`, `dropped`, `withdrawn`).
  Future<String> _sendSealed(Uint8List plaintext) async {
    final session = _session;
    if (session == null) throw const SocketLost();
    final out = await relaySendFrame(
      to: computer.id,
      packet: await channel.seal(plaintext),
      ttlSeconds: 600,
    );
    session.send(out.text);
    return out.pid;
  }

  /// Whether the phone is connected to the relay right now.
  bool get connected => _session != null;

  /// A message that is not a command (a `detail` request, a `sync`, a start preview). Throws [SocketLost] when not connected.
  Future<String> send(Uint8List plaintext) => _sendSealed(plaintext);

  /// The next packet counter, for a command that carries its own inside its signature.
  Future<BigInt> takeCounter() => channel.nextCounter();

  /// Sends a signed command sealed with the counter its signature covers. Its id is remembered (and stored) first, so a
  /// `sync` names it until its result comes, and the command is never sent twice.
  Future<String> sendCommand(
    String cid,
    BigInt counter,
    Uint8List message,
  ) async {
    final session = _session;
    if (session == null) throw const SocketLost();
    computer.pending.add(cid);
    await _save();
    final out = await relaySendFrame(
      to: computer.id,
      packet: channel.sealWith(counter, message),
      ttlSeconds: 600,
    );
    session.send(out.text);
    return out.pid;
  }

  /// Asks the relay to take back a packet it still holds for an offline computer.
  Future<void> withdraw(String pid) async {
    final session = _session;
    if (session == null) throw const SocketLost();
    session.send(await relayWithdrawFrame(pid: pid));
  }

  /// A command that was dealt with (a result came, or the person gave up on it) is no longer waited for.
  Future<void> forget(String cid) async {
    if (computer.pending.remove(cid)) await _save();
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
      case RelayFrame_Queued() ||
          RelayFrame_Delivered() ||
          RelayFrame_Dropped() ||
          RelayFrame_Withdrawn() ||
          RelayFrame_TooLate():
        if (!_relayEvents.isClosed) _relayEvents.add(f);
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
      case RelayError.grant:
        // Only a refusal of this phone's own token reaches a phone. It is said, not guessed at: nothing will be pushed.
        final id = _pushId;
        if (id != null) _set(status.copyWith(pushRefusedFor: id));
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
        homeLive = true;
        // Content of items that no longer wait is not needed again.
        final open = {
          for (final w in (j['waiting'] as List? ?? const []))
            (w as Map<String, dynamic>)['id'],
        };
        detailCache.removeWhere((id, _) => !open.contains(id));
        // A `home` without `rights` (an older computer) keeps the rights the phone has.
        final granted = j['rights'];
        if (granted is Map<String, dynamic>) {
          computer.rights = rightsFromJson(granted);
        }
        computer
          ..home = m.json
          ..homeAt = snap.receivedAtMs;
        await _save();
        final held = {
          for (final w in (j['waiting'] as List).cast<Map<String, dynamic>>())
            w['id'] as String,
        };
        for (final id in held) {
          waitingSince.putIfAbsent(id, () => snap.receivedAtMs);
        }
        waitingSince.removeWhere((id, _) => !held.contains(id));
        if (!_homeChanges.isClosed) _homeChanges.add(snap);
        final state = (j['computer'] as Map<String, dynamic>)['state'];
        if (state == 'asleep' && status.conn == ConnState.ok) {
          _set(status.copyWith(conn: ConnState.asleep));
        } else if (state == 'online' && status.conn == ConnState.asleep) {
          _set(status.copyWith(conn: ConnState.ok));
        }
      case 'notice' when j['what'] == 'closed':
        final by = j['by'];
        final ref = j['ref'] as String;
        if (by is Map<String, dynamic>) {
          _closedBy.remove(ref);
          _closedBy[ref] = by;
          if (_closedBy.length > 64) _closedBy.remove(_closedBy.keys.first);
        }
        if (!_closedNotices.isClosed) {
          _closedNotices.add(
            ClosedNotice(
              ref,
              j['session'] as String? ?? '',
              by is Map<String, dynamic> ? by : null,
            ),
          );
        }
      case 'result':
        final r = await commandResult(messageJson: m.json);
        if (r != null && !_results.isClosed) _results.add(r);
      case 'detail':
        final id = j['id'] as String;
        detailCache[id] = j;
        _details.remove(id)?.complete(j);
      case 'revoked':
        await _eraseEverything();
        _set(
          status.copyWith(
            revokedByComputer: true,
            revokedAtMs: _now().millisecondsSinceEpoch,
          ),
        );
      case 'relayMoved':
        final url = j['url'] as String;
        if (await relayAllowed(url, flavor) && url != computer.relay) {
          _pushForget();
          computer.relay = url;
          await _save();
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
    _pushForget();
    await keys.eraseAll();
    await store.erase();
    await _session?.close();
  }

  /// The whole content of a request, plan or question, from the computer (in memory only). Null when the phone is not
  /// connected or the computer did not answer.
  Future<Map<String, dynamic>?> detail(
    String id, {
    bool fresh = false,
    Duration within = const Duration(seconds: 10),
  }) async {
    final cached = detailCache[id];
    if (cached != null && !fresh) return cached;
    final waiter = _details.putIfAbsent(id, Completer.new);
    try {
      // A screen opened as the app comes back (a tap on a notification or the Live Activity) asks before the link is up
      // again: the question waits for it instead of failing at once.
      if (!connected) {
        await statusChanges.firstWhere((_) => connected).timeout(within);
      }
      await _sendSealed(await detailRequestMessage(id: id));
      return await waiter.future.timeout(within);
    } on Object {
      _details.remove(id);
      return null;
    }
  }

  /// Tells the computer this phone is leaving (a signed goodbye), and waits until the relay says it has it or [within] is over.
  /// Best effort: a phone that is offline just leaves, and the computer can still remove it there.
  Future<void> sayGoodbye({
    Duration within = const Duration(seconds: 4),
  }) async {
    final session = _session;
    if (session == null) return;
    final packet = await channel.bye(
      authSeed: identity.authSeed,
      boxSecret: identity.boxSecret,
      phoneId: identity.deviceId,
      computerId: computer.id,
    );
    final out = await relaySendFrame(
      to: computer.id,
      packet: packet,
      ttlSeconds: 600,
    );
    final done = Completer<void>();
    final sub = relayEvents.listen((f) {
      final ours = switch (f) {
        RelayFrame_Queued(:final pid) ||
        RelayFrame_Delivered(:final pid) ||
        RelayFrame_Dropped(:final pid) => pid == out.pid,
        _ => false,
      };
      if (ours && !done.isCompleted) done.complete();
    });
    session.send(out.text);
    try {
      await done.future.timeout(within);
    } on TimeoutException {
      // The goodbye is on its way; the phone leaves either way.
    } finally {
      await sub.cancel();
    }
  }

  /// The person erases this phone's pairing (Pair again, or a disconnect): keys, state, everything.
  Future<void> erase() => _eraseEverything();
}

class SocketLost implements Exception {
  const SocketLost();
}
