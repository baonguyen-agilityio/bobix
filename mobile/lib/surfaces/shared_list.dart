import 'dart:convert';

import '../data/models.dart';
import '../pets/creature.dart';

/// The shared list the widgets, the Live Activity and the controls read (`ios/WidgetsShared/SharedList.swift` decodes
/// exactly this). Names and counts only: never a tool input, a command or a key.
class SharedListJson {
  SharedListJson._();

  /// The longest a request counts as current, in seconds: surfaces fade on age (15 minutes) whatever the computer says.
  static const holdSecs = 900;

  static String build(
    HomeData home, {
    required String computerId,
    required bool canStart,
    required DateTime now,
    DateTime? builtAt,
    int todo = 0,
  }) => jsonEncode(toMap(home, computerId: computerId, canStart: canStart, now: now, builtAt: builtAt, todo: todo));

  static Map<String, Object?> toMap(
    HomeData home, {
    required String computerId,
    required bool canStart,
    required DateTime now,
    DateTime? builtAt,
    int todo = 0,
  }) => {
    'written': now.millisecondsSinceEpoch,
    'computerName': home.computer,
    'computerState': switch (home.conn) {
      ConnState.ok => 'online',
      ConnState.asleep => 'asleep',
      _ => 'offline',
    },
    'todo': todo,
    'canStart': canStart,
    'rows': [
      for (final s in home.sessions.where((s) => s.stage != Stage.todo))
        {
          'id': s.id,
          'name': s.name,
          'project': s.project,
          'state': s.petState,
          'pet': s.petDesign,
          'color': s.petColor % petColorCount,
          // A request answered at the computer or held there is not one to open from a surface.
          'ref': s.needsYou && !s.answerAtComputer ? (s.waitingId ?? '') : '',
          'computer': computerId,
          // What is left of the request's wait when this `home` came; none when the computer waits for ever.
          // A list written again later (the heartbeat) takes the time since [builtAt] off the wait the computer still holds.
          'holdSecs': (s.holdSecs == null ? holdSecs : s.holdSecs! - (builtAt == null ? 0 : now.difference(builtAt).inSeconds)).clamp(1, holdSecs),
          'at': now.millisecondsSinceEpoch,
        },
    ],
  };

  /// What the list says, without the times it was written at: two lists with the same key look the same on every surface.
  static String contentKey(Map<String, Object?> map) => jsonEncode({
    for (final e in map.entries)
      if (e.key != 'written')
        e.key: e.key == 'rows'
            ? [
                for (final r in e.value! as List)
                  {
                    for (final f in (r as Map<String, Object?>).entries)
                      if (f.key != 'at') f.key: f.value,
                  },
              ]
            : e.value,
  });

  /// The sessions that wait for this phone: something is held for it and the person has not taken it to the computer. The
  /// computer counts them the same way when it pushes the Live Activity.
  static int waiting(HomeData h) => h.sessions
      .where((s) => s.needsYou && !s.answerAtComputer && !s.answeredAtComputer && !s.handledAtComputer)
      .length;
}
