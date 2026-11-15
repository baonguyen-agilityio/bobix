// The shared crate's test vectors through the bridge, on this computer: build the library first,
//   cargo build --manifest-path rust/Cargo.toml
// (CI does), or point BOBIX_CORE_LIB at a built library.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import '../integration_test/support/crate_vectors.g.dart';
import 'support/core_library.dart';
import 'support/core_vectors_checks.dart';

void main() {
  const crateVectors = '../crates/bobix-link/tests/vectors.json';

  setUpAll(initCoreLibrary);

  test('every vector of the shared crate passes through the bridge', () async {
    final v = jsonDecode(
      File(crateVectors).readAsStringSync(),
    ) as Map<String, dynamic>;
    await checkVectors(v);
  });

  test("the simulator test's copy of the vectors is the crate's: an updated file with no matching change fails here", () {
    expect(
      crateVectorsJson,
      File(crateVectors).readAsStringSync(),
      reason: 'run: node mobile/tool/crate-vectors.mjs, then update the bridge',
    );
  });
}
