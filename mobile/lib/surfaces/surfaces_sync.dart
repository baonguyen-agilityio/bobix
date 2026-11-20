import 'dart:async';
import 'dart:convert';
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
/// The Live Activity has two writers: this app (when it has data, counts only) and the computer, which pushes a sealed count
/// through the relay (to start one with the app closed, to update it and to end it). Both draw the same count. This one
/// never ends the activity on a zero it cannot trust: only a `home` that came over the connection that is up, from a
/// computer that is reachable, says nothing waits.
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

  /// The longest the list and the activity go without a write when nothing changed: well under the 15 minutes after which
  /// every surface draws them stale.
  static const refreshAfter = Duration(minutes: 5);

  // What was last written (the list's content without its times, and when) and last sent to the activity.
  String? _listKey;
  DateTime? _listAt;
  ({int waiting, String computerId, bool trusted})? _activityKey;
  DateTime? _activityAt;

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
    final map = SharedListJson.toMap(
      home,
      computerId: computerId,
      canStart: canStart,
      now: now,
      builtAt: _builtAt,
    );
    final key = SharedListJson.contentKey(map);
    // Unchanged content is not written again (each write reloads every widget), except as the heartbeat or once it is old.
    final fresh = _listAt != null && now.difference(_listAt!) < refreshAfter;
    if (again || key != _listKey || !fresh) {
      try {
        await _platform.writeList(jsonEncode(map));
        _listKey = key;
        _listAt = now;
      } catch (_) {
        // A surface that is not refreshed goes stale on its own; the app goes on.
        _listKey = null;
      }
    }
    await _activity(home, now, computerId, force: again);
  }

  /// Forgets what was written, so the next `home` writes everything (the surfaces were cleared).
  void reset() {
    _listKey = null;
    _listAt = null;
    _activityKey = null;
    _activityAt = null;
    _last = null;
    _petsWritten.clear(); // the images were deleted with the rest
  }

  /// Writes the last `home` again, as the heartbeat: only while the computer was reachable then (once the app is paused or the
  /// computer is gone, the data is left to age into stale). The seconds a request still has count down from when it was built.
  Future<void> again({required String computerId, required bool canStart}) async {
    final h = _last;
    if (!visible || h == null || h.conn != ConnState.ok) return;
    await sync(h, computerId: computerId, canStart: canStart, again: true);
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

  Future<void> _activity(
    HomeData home,
    DateTime now,
    String computerId, {
    required bool force,
  }) async {
    try {
      if (!await _platform.liveActivities()) return;
      final waiting = SharedListJson.waiting(home);
      // The `home` kept from an earlier run, or the last one before the computer went away, has no say in ending the
      // activity: the computer's own push does that, and the app's next live `home` too.
      if (waiting == 0 && !(home.live && home.conn == ConnState.ok)) return;
      // The platform keeps one activity: it ends it when nothing waits, updates the one that exists (also one from an
      // earlier run of the app) and starts one otherwise. No name goes into it: it draws the shared list.
      // The activity draws the count; its stale date is the last update plus 15 minutes. Without a change it is updated
      // only to move that date.
      final key = (
        waiting: waiting,
        computerId: computerId,
        trusted: home.live && home.conn == ConnState.ok,
      );
      final fresh =
          _activityAt != null && now.difference(_activityAt!) < refreshAfter;
      if (!force && fresh && key == _activityKey) return;
      await _platform.syncActivity({
        'waiting': waiting,
        'written': now.millisecondsSinceEpoch,
        'computerId': computerId,
      });
      _activityKey = key;
      _activityAt = now;
    } catch (_) {
      // No Live Activity is not a reason to stop the app.
    }
  }
}
