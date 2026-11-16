import 'dart:io';
import 'dart:typed_data';

import 'creature.dart';

/// The folder, inside the app's support folder, that holds the stills of pets at work for the native privacy cover
/// (`ios/Runner/SceneDelegate.swift` reads the same name). Always there once the app has run: it needs no session and no
/// computer, and nothing in it says anything about either.
const coverPetsFolder = 'cover-pets';

/// The pets drawn for the cover: every design in three colours.
List<(String, int)> coverPetChoices() {
  final colors = {for (var i = 0; i < 3; i++) i * petColorCount ~/ 3};
  return [
    for (final d in designs)
      for (final c in colors) (d, c),
  ];
}

/// Writes the cover stills that are not there yet into [support]/[coverPetsFolder]. [render] draws one (the same
/// renderer the widgets use). A failure leaves what is written: the cover falls back to the app tile.
Future<int> writeCoverPets(
  Directory support,
  Future<Uint8List> Function(String design, int color, String state) render,
) async {
  final dir = Directory('${support.path}/$coverPetsFolder');
  var written = 0;
  try {
    await dir.create(recursive: true);
    for (final (design, color) in coverPetChoices()) {
      final file = File('${dir.path}/$design-$color-working.png');
      if (await file.exists()) continue;
      await file.writeAsBytes(
        await render(design, color, 'working'),
        flush: true,
      );
      written++;
    }
  } catch (_) {
    // Not worth stopping the app for: the cover shows the app tile.
  }
  return written;
}
