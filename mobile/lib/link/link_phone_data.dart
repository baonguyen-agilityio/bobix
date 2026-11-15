import 'dart:async';

import '../data/models.dart';
import '../data/phone_data.dart';
import '../src/rust/api/messages.dart';
import 'computer_link.dart';

String _clock(int ms) {
  final t = DateTime.fromMillisecondsSinceEpoch(ms);
  return '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}';
}

Stage _stageOf(String group) => switch (group) {
  'waitingForReview' => Stage.verify,
  'stuck' => Stage.stuck,
  'toDo' => Stage.todo,
  'done' => Stage.done,
  _ => Stage.doing,
};

/// What the screens show, made from what the computer sent: the real data behind [PhoneData]. The computer's `home` says
/// which sessions exist, what waits and in which group; this adds what only the phone knows (how long a request has been
/// waiting here, which one the computer answered first).
class LinkPhoneData implements PhoneData {
  LinkPhoneData(this.link, {DateTime Function()? now})
    : _now = now ?? DateTime.now;

  final ComputerLink link;
  final DateTime Function() _now;

  /// When this phone first saw a waiting item, to say how long it has been waiting.
  final _firstSeen = <String, int>{};

  /// A request that was in the previous `home` and is not in this one while time was left: it was answered at the
  /// computer (first answer wins), so the session shows it. Cleared when the session waits again.
  final _answeredAtComputer = <String>{};
  Map<String, String> _previousWaiting = {};
  Map<String, int?> _previousExpiry = {};
  HomeSnapshot? _seen;

  int get _nowMs => _now().millisecondsSinceEpoch;

  HomeData? _build() {
    final snap = link.home;
    final conn = link.status.conn;
    if (snap == null) {
      return conn == ConnState.ok
          ? null
          : HomeData(
              computer: link.computer.name,
              conn: conn,
              lastContact: '',
              sessions: const [],
            );
    }
    if (!identical(snap, _seen)) {
      _track(snap);
      _seen = snap;
    }
    return homeData(
      snap,
      conn: conn,
      computerName: link.computer.name,
      firstSeen: _firstSeen,
      answeredAtComputer: _answeredAtComputer,
      nowMs: _nowMs,
    );
  }

  void _track(HomeSnapshot snap) {
    final waiting = <String, String>{};
    final expiry = <String, int?>{};
    for (final w
        in (snap.home['waiting'] as List).cast<Map<String, dynamic>>()) {
      final id = w['id'] as String;
      waiting[id] = w['session'] as String;
      final left = w['secondsLeft'] as int?;
      expiry[id] = left == null ? null : snap.receivedAtMs + left * 1000;
      _firstSeen.putIfAbsent(id, () => snap.receivedAtMs);
      _answeredAtComputer.remove(w['session']);
    }
    final sessions = {
      for (final x
          in (snap.home['sessions'] as List).cast<Map<String, dynamic>>())
        x['id'] as String,
    };
    for (final e in _previousWaiting.entries) {
      final timeLeft =
          _previousExpiry[e.key] == null ||
          _previousExpiry[e.key]! > snap.receivedAtMs;
      if (!waiting.containsKey(e.key) &&
          !waiting.containsValue(e.value) &&
          sessions.contains(e.value) &&
          timeLeft) {
        _answeredAtComputer.add(e.value);
      }
    }
    _firstSeen.removeWhere((id, _) => !waiting.containsKey(id));
    _previousWaiting = waiting;
    _previousExpiry = expiry;
  }

  @override
  Stream<HomeData> watchHome() {
    late final StreamController<HomeData> out;
    StreamSubscription<Object>? a, b;
    void emit() {
      final d = _build();
      if (d != null && !out.isClosed) out.add(d);
    }

    out = StreamController<HomeData>(
      onListen: () {
        emit();
        a = link.homeChanges.listen((_) => emit());
        b = link.statusChanges.listen((_) => emit());
      },
      onCancel: () async {
        await a?.cancel();
        await b?.cancel();
      },
    );
    return out.stream;
  }

  Map<String, dynamic>? _waitingOf(String sessionId, {String? what}) {
    final snap = link.home;
    if (snap == null) return null;
    for (final w
        in (snap.home['waiting'] as List).cast<Map<String, dynamic>>()) {
      if (w['session'] == sessionId && (what == null || w['what'] == what)) {
        return w;
      }
    }
    return null;
  }

  Session? _sessionOf(String id) {
    final d = _build();
    if (d == null) return null;
    for (final x in d.sessions) {
      if (x.id == id) return x;
    }
    return null;
  }

  @override
  Future<RequestDetail?> request(String sessionId) async {
    final w = _waitingOf(sessionId, what: 'request');
    final snap = link.home;
    if (w == null || snap == null) return null;
    final d = await link.detail(w['id'] as String);
    if (d == null) return null;
    final project = [
      for (final x
          in (snap.home['sessions'] as List).cast<Map<String, dynamic>>())
        if (x['id'] == sessionId) x['project'] as String,
    ].firstOrNull;
    final left = w['secondsLeft'] as int?;
    final remaining = left == null
        ? null
        : left - (_nowMs - snap.receivedAtMs) ~/ 1000;
    return RequestDetail(
      sessionId: sessionId,
      command: d['text'] as String,
      folder: project ?? '',
      expiresMinutes: remaining == null
          ? null
          : (remaining / 60).ceil().clamp(0, 1 << 30),
      destructive: w['risky'] == true,
      hiddenCharacters: d['odd'] == true,
    );
  }

  @override
  Future<SessionDetail?> session(String sessionId) async {
    final s = _sessionOf(sessionId);
    final snap = link.home;
    if (s == null || snap == null) return null;
    final held = _waitingOf(sessionId, what: 'request');
    final canStop =
        held != null &&
        await rightsAllow(
          rights: link.computer.rights,
          input: CommandInput.stop(
            session: sessionId,
            request: held['id'] as String,
          ),
        );
    return SessionDetail(
      session: s,
      // A session's last lines travel in their own `detail`; the protocol does not name them in `home` yet.
      activity: const [],
      lastContact: _clock(snap.receivedAtMs),
      canStop: canStop,
    );
  }
}

/// Builds the screens' data from a `home` (its decoded JSON). Pure: the phone's own knowledge comes in as arguments.
HomeData homeData(
  HomeSnapshot snap, {
  required ConnState conn,
  required String computerName,
  required Map<String, int> firstSeen,
  required Set<String> answeredAtComputer,
  required int nowMs,
}) {
  final h = snap.home;
  final waiting = <String, Map<String, dynamic>>{
    for (final w in (h['waiting'] as List).cast<Map<String, dynamic>>())
      w['id'] as String: w,
  };
  final heldBySession = <String, Map<String, dynamic>>{};
  // A session may wait on several things; a request (which can be answered here) comes first.
  for (final w in waiting.values) {
    final held = heldBySession[w['session']];
    if (held == null || (held['what'] != 'request' && w['what'] == 'request')) {
      heldBySession[w['session'] as String] = w;
    }
  }
  final groupOf = <String, String>{};
  final listedAsNeedsYou = <String>{};
  for (final g in (h['groups'] as List).cast<Map<String, dynamic>>()) {
    for (final id in (g['ids'] as List).cast<String>()) {
      // An id names a waiting item or a session.
      final session = waiting[id]?['session'] as String? ?? id;
      groupOf.putIfAbsent(session, () => g['group'] as String);
      if (g['group'] == 'needsYou') listedAsNeedsYou.add(session);
    }
  }
  final sessions = <Session>[];
  for (final x in (h['sessions'] as List).cast<Map<String, dynamic>>()) {
    final id = x['id'] as String;
    final w = heldBySession[id];
    final pet = x['pet'] as Map<String, dynamic>;
    final needsYou = w != null || listedAsNeedsYou.contains(id);
    // Only a request can be answered here for now: a plan or a question is answered at the computer.
    final atComputerOnly = needsYou && (w == null || w['what'] != 'request');
    final since = w == null ? null : firstSeen[w['id']];
    sessions.add(
      Session(
        id: id,
        name: x['title'] as String,
        project: x['project'] as String,
        stage: needsYou
            ? Stage.doing
            : _stageOf(
                groupOf[id] ?? (x['state'] == 'closed' ? 'done' : 'inProgress'),
              ),
        needsYou: needsYou,
        answerAtComputer: atComputerOnly,
        answeredAtComputer: !needsYou && answeredAtComputer.contains(id),
        handledAtComputer: w?['atComputer'] == true,
        waitingMinutes: since == null ? 0 : (nowMs - since) ~/ 60000,
        petDesign: pet['design'] as String,
        petColor: pet['color'] as int,
      ),
    );
  }
  final computer = h['computer'] as Map<String, dynamic>;
  return HomeData(
    computer: computer['name'] as String? ?? computerName,
    conn: conn,
    lastContact: _clock(snap.receivedAtMs),
    sessions: sessions,
  );
}
