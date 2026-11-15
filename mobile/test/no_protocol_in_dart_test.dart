// Dart moves bytes and text: relay frames and the receive rules come from the shared crate through the bridge. A frame
// field name or a counter check written in the link code would be the protocol written twice.
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'the link code builds no relay frame and runs no receive rule of its own',
    () {
      final files = Directory('lib/link')
          .listSync(recursive: true)
          .whereType<File>()
          .where((f) => f.path.endsWith('.dart'));
      expect(files, isNotEmpty);
      const forbidden = [
        "'type':", // a relay frame written as a map
        "['type']",
        "'pairSend'",
        "'challenge'",
        "'presence'",
        "'relayToken'",
        'peekPacket(',
        'recvCheck(',
        'recvCommit(',
        'openPacket(',
        'checkVersion(',
        'sealPacket(',
        'authBytes(',
        'signWithAuth(',
      ];
      for (final f in files) {
        final text = f.readAsStringSync();
        for (final word in forbidden) {
          expect(text.contains(word), isFalse, reason: '${f.path} has $word');
        }
      }
    },
  );
}
