// Draws every pet the widgets can show to a PNG in a running app (the simulator), and checks each one has pixels.
// With --dart-define=BOBIX_PET_DIR=<folder> it also writes them as `<design>-<colour>-<state>.png`, the names the widget
// extension reads, so the widgets can be drawn with real pets.
//   flutter test integration_test/pet_png_test.dart -d <simulator> --dart-define=BOBIX_PET_DIR=/some/folder
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:phone/pets/creature.dart';
import 'package:phone/surfaces/pet_png.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const dir = String.fromEnvironment('BOBIX_PET_DIR');

  testWidgets('every design in every state draws, in a few colours', (tester) async {
    if (dir.isNotEmpty) Directory(dir).createSync(recursive: true);
    for (final design in designs) {
      for (final state in petStates) {
        for (final color in [0, 1, 2, 3]) {
          final png = await renderPetPng(design, color, state);
          final codec = await ui.instantiateImageCodec(png);
          final image = (await codec.getNextFrame()).image;
          final raw = (await image.toByteData())!;
          var alpha = 0;
          for (var i = 3; i < raw.lengthInBytes; i += 4) {
            alpha += raw.getUint8(i);
          }
          expect(alpha, greaterThan(0), reason: '$design $color $state');
          if (dir.isNotEmpty) File('$dir/$design-$color-$state.png').writeAsBytesSync(png);
        }
      }
    }
  });
}
