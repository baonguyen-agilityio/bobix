import 'dart:async';
import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/phone_data.dart';
import '../link/link_providers.dart';
import 'pet_png.dart';
import 'surface_list.dart';
import 'surface_sink.dart';

final surfaceSinkProvider = Provider<SurfaceSink>(
  (ref) => platformSurfaceSink(),
);

/// Draws a pet to a PNG; tests give a cheap one.
final petPngProvider =
    Provider<Future<Uint8List> Function(String, int, String)>(
      (ref) =>
          (design, color, state) => renderPetPng(design, color, state),
    );

/// Keeps the surfaces outside the app (Android widgets, Live Update, tiles) in step with what the computer last said: every
/// `home` writes the small list, with the pet images it mentions first; when nothing is paired any more it clears them.
/// Watched once at the top of the app.
final surfaceSyncProvider = Provider<void>((ref) {
  final sink = ref.watch(surfaceSinkProvider);
  final now = ref.watch(clockProvider);
  final draw = ref.watch(petPngProvider);
  final written = <String>{};
  var queue = Future<void>.value();

  Future<void> write(Map<String, Object?> list) async {
    for (final (design, color, state) in petsOf(list)) {
      final name = petFileName(design, color, state);
      if (!written.add(name)) continue;
      try {
        await sink.writePet(name, await draw(design, color, state));
      } on Object {
        written.remove(
          name,
        ); // the next list tries again; the widget shows no picture meanwhile
      }
    }
    await sink.writeList(jsonEncode(list));
  }

  ref.listen(homeProvider, (_, next) {
    final data = next.value;
    if (data == null) return;
    final list = surfaceListJson(data, nowMs: now().millisecondsSinceEpoch);
    queue = queue.then((_) => write(list)).catchError((Object _) {});
  });
  ref.listen(linkControllerProvider, (_, next) {
    final phase = next.value;
    if (phase is Unpaired || phase is PairAgain) {
      queue = queue.then((_) => sink.clear()).catchError((Object _) {});
    }
  });
});

/// A tap on a surface outside the app: the screen it asks for. `request` carries the id of what waits.
@immutable
class SurfaceOpen {
  const SurfaceOpen(this.target, {this.id = '', this.kind = ''});

  /// `home`, `start` or `request`.
  final String target;
  final String id, kind;

  /// A tap is input from outside: only these targets, and a request needs an id.
  static SurfaceOpen? tryParse(Object? raw) {
    if (raw is! Map) return null;
    final t = raw['target'], id = raw['id'], k = raw['kind'];
    if (t is! String || id is! String || k is! String) return null;
    if (t != 'home' && t != 'start' && t != 'request') return null;
    if (t == 'request' && (id.isEmpty || id.length > 256)) return null;
    return SurfaceOpen(t, id: id, kind: k);
  }
}

class SurfaceOpenNotifier extends Notifier<SurfaceOpen?> {
  @override
  SurfaceOpen? build() => null;

  void open(SurfaceOpen o) => state = o;
  void clear() => state = null;
}

final surfaceOpenProvider = NotifierProvider<SurfaceOpenNotifier, SurfaceOpen?>(
  SurfaceOpenNotifier.new,
);

/// Receives the taps of the Android surfaces (`open` events, and the tap that started the app) and hands them to the app
/// shell through [surfaceOpenProvider]. Android only.
final surfaceOpenListenerProvider = Provider<void>((ref) {
  if (defaultTargetPlatform != TargetPlatform.android) return;
  const channel = MethodChannel('dev.bobix.phone/surfaces');
  channel.setMethodCallHandler((call) async {
    if (call.method == 'open') {
      final o = SurfaceOpen.tryParse(call.arguments);
      if (o != null) ref.read(surfaceOpenProvider.notifier).open(o);
    }
    return null;
  });
  // The first `initialOpen` is also native's "Dart listens" signal; a tap that started the app comes back here.
  unawaited(
    channel
        .invokeMethod<Object?>('initialOpen')
        .then((raw) {
          final o = SurfaceOpen.tryParse(raw);
          if (o != null) ref.read(surfaceOpenProvider.notifier).open(o);
        })
        .catchError((Object _) {}),
  );
  ref.onDispose(() => channel.setMethodCallHandler(null));
});
