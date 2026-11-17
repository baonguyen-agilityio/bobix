import 'dart:async';

import '../data/models.dart';
import '../data/phone_data.dart';
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
  final _answeredAtComputer = <String, String>{};

  /// What kind each item in [_answeredAtComputer] was, by session (`request`, `plan` or `question`).
  final _answeredKind = <String, String>{};
  Map<String, String> _previousWaiting = {};
  Map<String, String> _previousKind = {};
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
      // Where the computer's notice (which may come after the `home`) says it was not the computer, it is not listed.
      answeredAtComputer: {
        for (final e in _answeredAtComputer.entries)
          if (link.closerOf(e.value) case Closer.computer || Closer.unknown)
            e.key: e.value,
      },
      answeredKind: _answeredKind,
      nowMs: _nowMs,
    );
  }

  void _track(HomeSnapshot snap) {
    final waiting = <String, String>{};
    final kinds = <String, String>{};
    final expiry = <String, int?>{};
    for (final w
        in (snap.home['waiting'] as List).cast<Map<String, dynamic>>()) {
      final id = w['id'] as String;
      waiting[id] = w['session'] as String;
      kinds[id] = w['what'] as String? ?? 'request';
      final left = w['secondsLeft'] as int?;
      expiry[id] = left == null ? null : snap.receivedAtMs + left * 1000;
      _firstSeen.putIfAbsent(id, () => snap.receivedAtMs);
      _answeredAtComputer.remove(w['session']);
      _answeredKind.remove(w['session']);
    }
    final sessions = {
      for (final x
          in (snap.home['sessions'] as List).cast<Map<String, dynamic>>())
        x['id'] as String,
    };
    for (final e in _previousWaiting.entries) {
      if (waiting.containsKey(e.key) ||
          waiting.containsValue(e.value) ||
          !sessions.contains(e.value)) {
        continue;
      }
      if (_closedAtComputer(e.key, snap.receivedAtMs)) {
        _answeredAtComputer[e.value] = e.key;
        _answeredKind[e.value] = _previousKind[e.key] ?? 'request';
      }
    }
    _firstSeen.removeWhere((id, _) => !waiting.containsKey(id));
    _previousWaiting = waiting;
    _previousKind = kinds;
    _previousExpiry = expiry;
  }

  /// Whether an item that left `home` was closed at the computer: as the computer said, or this phone's own answer (never
  /// the computer's). Only with nothing to go by (an older computer) it is guessed, as before: its time had not run out.
  bool _closedAtComputer(String id, int receivedAtMs) =>
      switch (link.closerOf(id)) {
        Closer.computer => true,
        Closer.unknown =>
          _previousExpiry[id] == null || _previousExpiry[id]! > receivedAtMs,
        _ => false,
      };

  @override
  Stream<HomeData> watchHome() {
    late final StreamController<HomeData> out;
    StreamSubscription<Object>? a, b, c;
    void emit() {
      final d = _build();
      if (d != null && !out.isClosed) out.add(d);
    }

    out = StreamController<HomeData>(
      onListen: () {
        emit();
        a = link.homeChanges.listen((_) => emit());
        b = link.statusChanges.listen((_) => emit());
        // A notice after the `home` that dropped an item says who closed it: what it shows is made again.
        c = link.closedNotices.listen((_) => emit());
      },
      onCancel: () async {
        await a?.cancel();
        await b?.cancel();
        await c?.cancel();
      },
    );
    return out.stream;
  }
}

int? _secondsLeft(Map<String, dynamic>? w, int receivedAtMs, int nowMs) {
  final left = w?['secondsLeft'] as int?;
  if (left == null) return null;
  final remaining = left - (nowMs - receivedAtMs) ~/ 1000;
  return remaining < 0 ? 0 : remaining;
}

/// Builds the screens' data from a `home` (its decoded JSON). Pure: the phone's own knowledge comes in as arguments.
HomeData homeData(
  HomeSnapshot snap, {
  required ConnState conn,
  required String computerName,
  required Map<String, int> firstSeen,
  required Map<String, String> answeredAtComputer,
  Map<String, String> answeredKind = const {},
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
    final atComputer = x['state'] == 'atComputer';
    final needsYou = w != null || atComputer || listedAsNeedsYou.contains(id);
    // Needing you with nothing held for this phone: it can only be answered at the computer.
    final atComputerOnly = needsYou && w == null;
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
        atComputerWhy: atComputerOnly ? x['atComputerWhy'] as String? : null,
        answeredAtComputer: !needsYou && answeredAtComputer.containsKey(id),
        waitingId:
            w?['id'] as String? ?? (needsYou ? null : answeredAtComputer[id]),
        waitingKind: w != null
            ? _kindOf(w['what'] as String)
            : (answeredAtComputer.containsKey(id)
                  ? _kindOf(answeredKind[id] ?? 'request')
                  : null),
        handledAtComputer: w?['atComputer'] == true,
        waitingMinutes: since == null ? 0 : (nowMs - since) ~/ 60000,
        holdSecs: _secondsLeft(w, snap.receivedAtMs, nowMs),
        petDesign: pet['design'] as String,
        petColor: pet['color'] as int,
        activityLines: (x['lines'] as num?)?.toInt() ?? 0,
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

WaitingKind _kindOf(String what) => switch (what) {
  'plan' => WaitingKind.plan,
  'question' => WaitingKind.question,
  _ => WaitingKind.request,
};
