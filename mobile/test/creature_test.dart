import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/pets/creature.dart';

void main() {
  test('every design, colour, state, pose and frame equals the SVG of the desktop pets', () {
    final fixtures = (jsonDecode(
      File('test/fixtures/creature-fixtures.json').readAsStringSync(),
    ) as Map).cast<String, String>();
    expect(fixtures.length, 5 * 6 * 34);
    final bad = <String>[];
    for (final e in fixtures.entries) {
      final [design, color, state, pose, frame] = e.key.split('/');
      final svg = flatSvg(design, int.parse(color), state, (
        pose,
        int.parse(frame),
      )).replaceAll(RegExp(r'bx\d+'), 'bx');
      if (sha1.convert(utf8.encode(svg)).toString() != e.value) bad.add(e.key);
    }
    expect(bad, isEmpty, reason: '${bad.length} differ, first: ${bad.take(5)}');
  });

  test('an unknown design or colour index draws a pet instead of failing', () {
    final fallback = flatSvg('no-such-pet', 99, 'working', poseAt('working'));
    expect(
      fallback.replaceAll(RegExp(r'bx\d+'), 'bx'),
      flatSvg(
        'default',
        99 % petColorCount,
        'working',
        poseAt('working'),
      ).replaceAll(RegExp(r'bx\d+'), 'bx'),
    );
    expect(
      () => flatSvg('default', -1, 'unknown-state', poseAt('unknown-state')),
      returnsNormally,
    );
  });

  test(
    'a project keeps its design: the same hash and design as the desktop',
    () {
      // Values printed by creature.js (hashStr, designOf), including a non-BMP character.
      const expected = {
        'some project': (371516928, 'miso'),
        'shop': (2823282857, 'tty'),
        'blog': (1622977521, 'nota'),
        'Café ☕ 😀': (583235260, 'default'),
        '': (2166136261, 'nota'),
      };
      expected.forEach((id, v) {
        expect(hashStr(id), v.$1, reason: id);
        expect(designOf(id), v.$2, reason: id);
      });
    },
  );
}
