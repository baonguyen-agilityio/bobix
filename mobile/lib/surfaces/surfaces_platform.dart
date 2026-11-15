import 'dart:async';

import 'package:flutter/services.dart';

/// The widgets, the Live Activity and the controls, seen from Dart: the app writes what they show and runs the activity.
/// Implemented over the channel `dev.bobix.phone/surfaces` (Swift on iOS); tests use a fake. Nothing here answers a
/// request: every surface opens the app.
abstract interface class SurfacesPlatform {
  /// Whether Live Activities can run (the person may have turned them off).
  Future<bool> liveActivities();

  /// Writes the shared list (JSON of [SharedListJson]) and asks the widgets and controls to reload.
  Future<void> writeList(String json);

  /// Writes the image of one pet: a design, a colour index and a state.
  Future<void> writePet(String design, int color, String state, Uint8List png);

  Future<({bool started, String? id})> startActivity(Map<String, Object?> state);
  Future<void> updateActivity(Map<String, Object?> state);
  Future<void> endActivity();

  /// A token of the running activity (its own push token), and the token that lets a push start one.
  Stream<({String id, String token})> get activityTokens;
  Stream<String> get pushToStartTokens;
}

class ChannelSurfacesPlatform implements SurfacesPlatform {
  ChannelSurfacesPlatform({MethodChannel? channel})
    : _channel = channel ?? const MethodChannel('dev.bobix.phone/surfaces') {
    _channel.setMethodCallHandler((call) async {
      switch (call.method) {
        case 'activityToken':
          final m = call.arguments as Map<Object?, Object?>;
          _activity.add((id: '${m['id']}', token: '${m['token']}'));
        case 'pushToStartToken':
          _start.add('${call.arguments}');
      }
      return null;
    });
  }

  final MethodChannel _channel;
  final _activity = StreamController<({String id, String token})>.broadcast();
  final _start = StreamController<String>.broadcast();

  @override
  Stream<({String id, String token})> get activityTokens => _activity.stream;

  @override
  Stream<String> get pushToStartTokens => _start.stream;

  @override
  Future<bool> liveActivities() async {
    final m = await _channel.invokeMethod<Map<Object?, Object?>>('capabilities');
    return m?['liveActivities'] == true;
  }

  @override
  Future<void> writeList(String json) =>
      _channel.invokeMethod<void>('writeList', {'json': json});

  @override
  Future<void> writePet(String design, int color, String state, Uint8List png) =>
      _channel.invokeMethod<void>('writePet', {
        'design': design,
        'color': color,
        'state': state,
        'png': png,
      });

  @override
  Future<({bool started, String? id})> startActivity(
    Map<String, Object?> state,
  ) async {
    final m = await _channel.invokeMethod<Map<Object?, Object?>>(
      'startActivity',
      state,
    );
    return (started: m?['started'] == true, id: m?['id'] as String?);
  }

  @override
  Future<void> updateActivity(Map<String, Object?> state) =>
      _channel.invokeMethod<void>('updateActivity', state);

  @override
  Future<void> endActivity() => _channel.invokeMethod<void>('endActivity');
}
