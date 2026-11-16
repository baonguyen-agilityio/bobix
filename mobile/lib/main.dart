import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app.dart';
import 'link/link_providers.dart';
import 'pets/cover_pets.dart';
import 'surfaces/pet_png.dart';

import 'package:path_provider/path_provider.dart';

import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  // The stills the privacy cover shows (a pet at work), made once and kept; not waited for.
  unawaited(
    getApplicationSupportDirectory().then(
      (dir) => writeCoverPets(
        dir,
        (design, color, state) => renderPetPng(design, color, state),
      ),
    ),
  );
  runApp(
    ProviderScope(
      overrides: [linkPhoneData, ...linkActionOverrides],
      child: const BobixApp(),
    ),
  );
}
