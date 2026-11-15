import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/src/rust/frb_generated.dart';

/// Where `cargo build --manifest-path rust/Cargo.toml` puts the library, or BOBIX_CORE_LIB.
String coreLibraryPath() {
  final given = Platform.environment['BOBIX_CORE_LIB'];
  if (given != null && given.isNotEmpty) return given;
  final name = Platform.isMacOS
      ? 'libbobix_phone_core.dylib'
      : Platform.isWindows
      ? 'bobix_phone_core.dll'
      : 'libbobix_phone_core.so';
  return 'rust/target/debug/$name';
}

/// Loads the Rust core for a test on this computer (once per test file).
Future<void> initCoreLibrary() async {
  final path = coreLibraryPath();
  if (!File(path).existsSync()) {
    fail(
      'The Rust library is not built ($path). Run: cargo build --manifest-path rust/Cargo.toml',
    );
  }
  await RustLib.init(externalLibrary: ExternalLibrary.open(path));
}
