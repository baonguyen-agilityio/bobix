import 'dart:async';
import 'dart:typed_data';

import '../data/models.dart';
import '../pets/creature.dart';
import 'pet_png.dart';
import 'shared_list.dart';
import 'surfaces_platform.dart';

typedef PetRenderer = Future<Uint8List> Function(
  String design,
  int color,
  String state,
);

/// Keeps the widgets, the controls and the Live Activity in step with the last `home`: writes the shared list and the
/// pets' images, and starts, updates or ends the activity.
///
/// Which Live Activity path runs: the activity is started HERE, when the app has data (the plan's fallback: counts and
/// states only), and may be updated by push through its own token. Starting one by push needs a relay payload with
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
  HomeData? _last;

  /// The app is on the screen. While it is not (paused or hidden) the heartbeat writes nothing, so the last list ages into
  /// stale on its own, and still says what the computer last said (`online`): leaving the app is not the computer going away.
  bool visible = true;
  DateTime? _builtAt;

  Future<void> sync(
    HomeData home, {
    required String computerId,
    required bool canStart,
    bool again = false,
  }) async {
    final now = _now();
    if (!again) {
      _last = home;
      _builtAt = now;
    }
    for (final s in home.sessions.where((s) => s.stage != Stage.todo)) {
      await _pet(s.petDesign, s.petColor % petColorCount, s.petState);
    }
    try {
      await _platform.writeList(
        SharedListJson.build(
          home,
          computerId: computerId,
          canStart: canStart,
          now: now,
          builtAt: _builtAt,
        ),
      );
    } catch (_) {
      // A surface that is not refreshed goes stale on its own; the app goes on.
    }
    await _activity(home, now, computerId);
  }

  /// Writes the last `home` again, as the heartbeat: only while the computer was reachable then (once the app is paused or the
  /// computer is gone, the data is left to age into stale). The seconds a request still has count down from when it was built.
  Future<void> again({
    required String computerId,
    required bool canStart,
  }) async {
    final h = _last;
    if (!visible || h == null || h.conn != ConnState.ok) return;
    await sync(h, computerId: computerId, canStart: canStart, again: true);
  }

  Future<void> _pet(String design, int color, String state) async {
    final key = '$design-$color-$state';
    if (_petsWritten.contains(key)) return;
    try {
      await _platform.writePet(
        design,
        color,
        state,
        await _renderPet(design, color, state),
      );
      _petsWritten.add(key);
    } catch (_) {
      // The surface draws a paw instead; the next sync tries again.
    }
  }

  Future<void> _activity(HomeData home, DateTime now, String computerId) async {
    try {
      if (!await _platform.liveActivities()) return;
      final waiting = SharedListJson.waiting(home);
      // The platform keeps one activity: it ends it when nothing waits, updates the one that exists (also one from an
      // earlier run of the app) and starts one otherwise. No name goes into it: it draws the shared list.
      await _platform.syncActivity({
        'waiting': waiting,
        'written': now.millisecondsSinceEpoch,
        'computerId': computerId,
      });
    } catch (_) {
      // No Live Activity is not a reason to stop the app.
    }
  }
}
