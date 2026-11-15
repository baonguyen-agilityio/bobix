// The shared crate's test vectors through the bridge, in the app on the iOS simulator (the Rust library built into it):
//   flutter test integration_test/core_vectors_test.dart -d <simulator>
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:phone/src/rust/frb_generated.dart';

import '../test/support/core_vectors_checks.dart';
import 'support/crate_vectors.g.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'every vector of the shared crate passes through the bridge in the app',
    (tester) async {
      await RustLib.init();
      final v = jsonDecode(crateVectorsJson) as Map<String, dynamic>;
      await checkVectors(v);
    },
  );
}
