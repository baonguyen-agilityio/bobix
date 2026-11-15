import '../data/models.dart';

/// The small list the Android surfaces outside the app read (widgets, the Live Update, the Quick Settings tiles): what
/// waits, by name, and counts. It holds no tool input, command or plan text, and no preview of any. Pure: the caller writes
/// it. The Kotlin side (`SurfaceList.kt`) reads exactly these keys.
Map<String, Object?> surfaceListJson(
  HomeData data, {
  required int nowMs,
  String computerId = '',
}) {
  Map<String, Object?>? pet(Session s) => {
    'design': s.petDesign,
    'color': s.petColor,
    'state': s.petState,
  };
  bool isWaiting(Session s) =>
      s.needsYou && s.waitingId != null && !s.answeredAtComputer;
  final waiting = [
    for (final s in data.sessions)
      if (isWaiting(s))
        {
          'id': s.waitingId,
          'kind': switch (s.waitingKind) {
            WaitingKind.plan => 'plan',
            WaitingKind.question => 'question',
            _ => 'request',
          },
          'name': s.name,
          'waitMinutes': s.waitingMinutes,
          'leftSeconds': s.waitingLeftSeconds,
          'pet': pet(s),
        },
  ];
  return {
    'v': 1,
    'writtenAt': nowMs,
    'computerId': computerId,
    'computer': data.computer,
    'online': data.conn == ConnState.ok,
    'todo': data.sessions.where((s) => s.stage == Stage.todo).length,
    'waiting': waiting,
    'sessions': [
      for (final s in data.sessions.where((s) => !isWaiting(s)).take(6))
        {'name': s.name, 'pet': pet(s)},
    ],
  };
}

/// The image file name the widgets look for: `<design>-<colour>-<state>.png` (the same on the Kotlin side).
String petFileName(String design, int color, String state) =>
    '$design-$color-$state.png';

/// Every pet image the list mentions, as file names with what draws them.
Set<(String, int, String)> petsOf(Map<String, Object?> list) {
  final out = <(String, int, String)>{};
  void add(Object? p) {
    if (p is Map<String, Object?>) {
      out.add((
        p['design']! as String,
        p['color']! as int,
        p['state']! as String,
      ));
    }
  }

  for (final w in (list['waiting']! as List).cast<Map<String, Object?>>()) {
    add(w['pet']);
  }
  for (final s in (list['sessions']! as List).cast<Map<String, Object?>>()) {
    add(s['pet']);
  }
  return out;
}
