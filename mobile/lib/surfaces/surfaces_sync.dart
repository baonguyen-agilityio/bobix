import 'dart:async';
import 'dart:typed_data';

import '../data/models.dart';
import '../pets/creature.dart';
import 'pet_png.dart';
import 'shared_list.dart';
import 'surfaces_platform.dart';

typedef PetRenderer = Future<Uint8List> Function(String design, int color, String state);

/// Keeps the widgets, the controls and the Live Activity in step with the last `home`: writes the shared list and the
/// pets' images, and starts, updates or ends the activity.
///
/// Which Live Activity path runs: the activity is started HERE, when the app has data (the plan's fallback: counts and
/// states only), and updated by push afterwards through its own token. Starting one by push needs a relay payload with
/// `event: start` and the attributes type, which the relay does not send yet, so the push-to-start token is registered
/// but nothing can use it until it does. Nothing was shown on a real iPhone.
class SurfacesSync {
  SurfacesSync(
    this._platform, {
    DateTime Function()? now,
    PetRenderer? renderPet,
  }) : _now = now ?? DateTime.now,
       _renderPet = renderPet ?? ((d, c, s) => renderPetPng(d, c, s));

  final SurfacesPlatform _platform;
  final DateTime Function() _now;
  final PetRenderer _renderPet;
  final _petsWritten = <String>{};
  bool _active = false;

  Future<void> sync(
    HomeData home, {
    required String computerId,
    required bool canStart,
  }) async {
    final now = _now();
    for (final s in home.sessions.where((s) => s.stage != Stage.todo)) {
      await _pet(s.petDesign, s.petColor % petColorCount, s.petState);
    }
    try {
      await _platform.writeList(
        SharedListJson.build(home, computerId: computerId, canStart: canStart, now: now),
      );
    } catch (_) {
      // A surface that is not refreshed goes stale on its own; the app goes on.
    }
    await _activity(home, now);
  }

  Future<void> _pet(String design, int color, String state) async {
    final key = '$design-$color-$state';
    if (_petsWritten.contains(key)) return;
    try {
      await _platform.writePet(design, color, state, await _renderPet(design, color, state));
      _petsWritten.add(key);
    } catch (_) {
      // The surface draws a paw instead; the next sync tries again.
    }
  }

  Future<void> _activity(HomeData home, DateTime now) async {
    try {
      if (!await _platform.liveActivities()) return;
      final waiting = home.sessions
          .where((s) => s.needsYou && !s.answerAtComputer && !s.answeredAtComputer)
          .toList();
      final state = <String, Object?>{
        'waiting': waiting.length,
        'names': [for (final s in waiting.take(3)) s.name],
        'written': now.millisecondsSinceEpoch,
        'computerName': home.computer,
        'computerState': home.conn == ConnState.ok ? 'online' : 'offline',
      };
      if (waiting.isNotEmpty && !_active) {
        _active = (await _platform.startActivity(state)).started;
      } else if (waiting.isNotEmpty) {
        await _platform.updateActivity(state);
      } else if (_active) {
        await _platform.endActivity();
        _active = false;
      }
    } catch (_) {
      // No Live Activity is not a reason to stop the app.
    }
  }
}
