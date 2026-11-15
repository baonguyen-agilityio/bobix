import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../data/models.dart';
import '../../data/phone_data.dart';
import '../../l10n/l10n.dart';
import '../../pets/creature.dart' show petColorCount;
import '../../pets/pet_view.dart';
import '../home_screen.dart' show openSession, waitingText;

/// What the previews draw: what the app has, as it is, with nothing (no request waits) or with the computer offline.
enum PreviewState { normal, empty, offline }

class PreviewStateNotifier extends Notifier<PreviewState> {
  @override
  PreviewState build() => PreviewState.normal;

  void set(PreviewState v) => state = v;
}

final previewStateProvider =
    NotifierProvider<PreviewStateNotifier, PreviewState>(
      PreviewStateNotifier.new,
    );

/// The facts a widget, a Live Activity or a control shows, from the last `home`.
class WidgetData {
  const WidgetData({
    required this.need,
    required this.pets,
    required this.todo,
    required this.off,
    required this.computer,
    required this.updated,
  });

  /// Sessions that wait for the person (none while the preview shows "Nothing").
  final List<Session> need;

  /// Every session that has a pet.
  final List<Session> pets;

  /// Tickets that wait to be started.
  final List<Session> todo;
  final bool off;
  final String computer, updated;

  /// The session a Live Activity or the island follows: the first that waits, else one that works, else any.
  Session? get live {
    if (need.isNotEmpty) return need.first;
    for (final p in pets) {
      if (p.stage == Stage.doing) return p;
    }
    return pets.isEmpty ? null : pets.first;
  }
}

final widgetDataProvider = Provider<WidgetData>((ref) {
  final home = ref.watch(homeProvider).value;
  final state = ref.watch(previewStateProvider);
  final all = home?.sessions ?? const <Session>[];
  return WidgetData(
    need: state == PreviewState.empty
        ? const []
        : all.where((x) => x.needsYou).toList(),
    pets: all.where((x) => x.stage != Stage.todo).toList(),
    todo: all.where((x) => x.stage == Stage.todo).toList(),
    off: state == PreviewState.offline,
    computer: home?.computer ?? '',
    updated: home?.lastContact ?? '',
  );
});

/// What a tap on a widget opens: the session that waits, or the app itself.
void openFromWidget(BuildContext context, Session? x) {
  if (x == null) {
    Navigator.of(context).popUntil((r) => r.isFirst);
  } else {
    openSession(context, x);
  }
}

/// One line about what [x] asks for or does: the request, or what the pet is working on.
String sessionLine(Strings s, Session x) => x.needsYou
    ? waitingText(s, x)
    : (x.tool == null
          ? s('phone.st.working0')
          : s('phone.st.working', {'tool': x.tool!}));

Widget previewPet(Session x, double size) => PetView(
  design: x.petDesign,
  color: x.petColor % petColorCount,
  state: x.petState,
  size: size,
);

/// The colour of the dot under a pet in the widgets: what its state is.
Color petDotColor(Session x) => switch (x.petState) {
  'waiting' => const Color(0xFFFF7A4D),
  'working' => const Color(0xFF5B8DEF),
  'failed' => const Color(0xFFE5484D),
  _ => const Color(0xFF34C38F),
};
