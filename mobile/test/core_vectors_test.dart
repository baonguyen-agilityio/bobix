// The shared crate's test vectors through the bridge, on this computer: build the library first,
//   cargo build --manifest-path rust/Cargo.toml
// (CI does), or point BOBIX_CORE_LIB at a built library.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/src/rust/frb_generated.dart';

import 'support/core_vectors_checks.dart';

String libraryPath() {
  final given = Platform.environment['BOBIX_CORE_LIB'];
  if (given != null && given.isNotEmpty) return given;
  final name = Platform.isMacOS
      ? 'libbobix_phone_core.dylib'
      : Platform.isWindows
      ? 'bobix_phone_core.dll'
      : 'libbobix_phone_core.so';
  return 'rust/target/debug/$name'; // where `cargo build` puts it
}

void main() {
  const crateVectors = '../crates/bobix-link/tests/vectors.json';

  setUpAll(() async {
    final path = libraryPath();
    if (!File(path).existsSync()) {
      fail(
        'The Rust library is not built ($path). Run: cargo build --manifest-path rust/Cargo.toml',
      );
    }
    await RustLib.init(externalLibrary: ExternalLibrary.open(path));
  });

  test('every vector of the shared crate passes through the bridge', () async {
    final v = jsonDecode(
      File(crateVectors).readAsStringSync(),
    ) as Map<String, dynamic>;
    await checkVectors(v);
  });

  test("the app's copy of vectors.json is the crate's: an updated file with no matching change fails here", () {
    expect(
      File('assets/vectors.json').readAsStringSync(),
      File(crateVectors).readAsStringSync(),
      reason: 'copy crates/bobix-link/tests/vectors.json to mobile/assets/vectors.json, then update the bridge',
    );
  });
}
