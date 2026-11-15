import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a closed request is remembered in memory only', () {
    final src = File('lib/link/link_actions_source.dart').readAsStringSync();
    expect(src, contains('_lastOpen'));
    // No file, preference or log write exists in the source that holds the remembered request.
    for (final banned in [
      'dart:io',
      'File(',
      'writeAs',
      'SharedPreferences',
      'print(',
      'debugPrint',
    ]) {
      expect(src, isNot(contains(banned)), reason: banned);
    }
  });
}
