import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/pets/cover_pets.dart';
import 'package:phone/pets/creature.dart';

void main() {
  late Directory support;
  setUp(() => support = Directory.systemTemp.createTempSync('bobix-cover-'));
  tearDown(() => support.deleteSync(recursive: true));

  final png = Uint8List.fromList([0x89, 0x50, 0x4e, 0x47]);

  test('the cover stills are every design at work, written once', () async {
    final states = <String>{};
    Future<Uint8List> render(String d, int c, String s) async {
      states.add(s);
      return png;
    }

    final n = await writeCoverPets(support, render);
    final files = Directory('${support.path}/$coverPetsFolder')
        .listSync()
        .map((f) => f.uri.pathSegments.last)
        .toList();
    expect(n, designs.length * 3);
    expect(coverPetChoices().toSet().length, n, reason: 'no two the same');
    expect(files.length, n);
    expect(
      files.every((f) => f.endsWith('-working.png')),
      isTrue,
      reason: 'only pets at work: the native cover reads that suffix',
    );
    for (final d in designs) {
      expect(files.any((f) => f.startsWith('$d-')), isTrue, reason: d);
    }
    expect(states, {'working'});
    expect(
      await writeCoverPets(support, render),
      0,
      reason: 'nothing is drawn twice',
    );
  });

  test('a renderer that fails does not stop the app', () async {
    final n = await writeCoverPets(
      support,
      (d, c, s) async => throw StateError('no'),
    );
    expect(n, 0);
  });

  test(
    'a write cut short leaves no broken still, and it is drawn again',
    () async {
      var fail = true;
      Future<Uint8List> render(String d, int c, String s) async {
        if (fail) throw StateError('cut short');
        return png;
      }

      expect(await writeCoverPets(support, render), 0);
      final dir = Directory('${support.path}/$coverPetsFolder');
      expect(
        dir.listSync().where((f) => f.path.endsWith('-working.png')),
        isEmpty,
      );
      fail = false;
      expect(await writeCoverPets(support, render), designs.length * 3);
    },
  );
}
