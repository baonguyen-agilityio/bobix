import 'dart:convert';

import '../data/models.dart';
import '../pets/creature.dart';

/// The shared list the widgets, the Live Activity and the controls read (`ios/WidgetsShared/SharedList.swift` decodes
/// exactly this). Names and counts only: never a tool input, a command or a key.
class SharedListJson {
  SharedListJson._();

  /// How long the computer holds a request for phones, in seconds, when the computer does not say.
  static const holdSecs = 600;

  static String build(
    HomeData home, {
    required String computerId,
    required bool canStart,
    required DateTime now,
    int todo = 0,
  }) => jsonEncode(toMap(home, computerId: computerId, canStart: canStart, now: now, todo: todo));

  static Map<String, Object?> toMap(
    HomeData home, {
    required String computerId,
    required bool canStart,
    required DateTime now,
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
          'holdSecs': holdSecs,
        },
    ],
  };

  /// The sessions that wait for this phone.
  static int waiting(HomeData h) =>
      h.sessions.where((s) => s.needsYou && !s.answerAtComputer && !s.answeredAtComputer).length;
}
