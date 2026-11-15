import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/data/models.dart';
import 'package:phone/data/phone_data.dart';
import 'package:phone/link/link_providers.dart';
import 'package:phone/surfaces/pet_png.dart';
import 'package:phone/surfaces/surface_list.dart';
import 'package:phone/surfaces/surface_sink.dart';
import 'package:phone/surfaces/surface_sync.dart';

/// Everything here is made up.
HomeData home({ConnState conn = ConnState.ok}) => HomeData(
  computer: 'Studio Mac (demo)',
  conn: conn,
  lastContact: '21:02',
  sessions: const [
    Session(
      id: 's1',
      name: 'Fix the login retry',
      project: 'demo-app',
      stage: Stage.doing,
      needsYou: true,
      waitingId: 'r1',
      waitingKind: WaitingKind.request,
      waitingMinutes: 3,
      waitingLeftSeconds: 240,
      tool: 'Bash: rm -rf secret-folder',
      petColor: 3,
    ),
    Session(
      id: 's2',
      name: 'Add the export',
      project: 'api',
      stage: Stage.doing,
      needsYou: true,
      waitingId: 'p1',
      waitingKind: WaitingKind.plan,
    ),
    Session(
      id: 's3',
      name: 'Answered at the computer',
      project: 'web',
      stage: Stage.verify,
      needsYou: true,
      waitingId: 'r9',
      answeredAtComputer: true,
    ),
    Session(
      id: 's4',
      name: 'Tidy the docs',
      project: 'docs',
      stage: Stage.todo,
    ),
  ],
);

class FakeSink implements SurfaceSink {
  final calls = <String>[];
  String? lastList;

  @override
  Future<void> clear() async => calls.add('clear');

  @override
  Future<void> writeList(String json) async {
    calls.add('list');
    lastList = json;
  }

  @override
  Future<void> writePet(String name, Uint8List png) async =>
      calls.add('pet:$name');
}

void main() {
  test(
    'the list has what waits by name and counts, and no tool or command text',
    () {
      final list = surfaceListJson(home(), nowMs: 1800000000000);
      final text = jsonEncode(list);
      expect(text, isNot(contains('rm -rf')));
      expect(text, isNot(contains('secret-folder')));
      expect(list['v'], 1);
      expect(list['online'], true);
      expect(list['todo'], 1);
      final waiting = (list['waiting']! as List).cast<Map<String, Object?>>();
      expect(waiting.map((w) => (w['id'], w['kind'])), [
        ('r1', 'request'),
        ('p1', 'plan'),
      ], reason: 'a request answered at the computer is not waiting here');
      expect(waiting.first['leftSeconds'], 240);
      expect(waiting.first['waitMinutes'], 3);
      expect(waiting.last['leftSeconds'], isNull);
      expect(
        (list['sessions']! as List).length,
        2,
        reason: 'the others, not the waiting ones',
      );
      expect(
        surfaceListJson(home(conn: ConnState.offline), nowMs: 1)['online'],
        false,
      );
    },
  );

  test('every pet the list mentions has a file name the widgets look for', () {
    final list = surfaceListJson(home(), nowMs: 1);
    final pets = petsOf(list);
    expect(pets, contains(('default', 3, 'waiting')));
    expect(petFileName('default', 3, 'waiting'), 'default-3-waiting.png');
    expect(
      pets.every(
        (p) =>
            RegExp(r'^[a-z0-9-]+-\d{1,3}-[a-z]+$')
                .hasMatch(petFileName(p.$1, p.$2, p.$3).replaceAll('.png', '')),
      ),
      isTrue,
    );
  });

  test('a pet is drawn to a PNG', () async {
    TestWidgetsFlutterBinding.ensureInitialized();
    final png = await renderPetPng('default', 3, 'waiting', px: 64);
    expect(png.sublist(0, 4), [0x89, 0x50, 0x4e, 0x47]);
    expect(png.length, greaterThan(200));
    expect(png.length, lessThan(200 * 1024));
  });

  test(
    'each home writes the pet images first and then the list; unpairing clears',
    () async {
      final sink = FakeSink();
      final controller = StreamController<HomeData>();
      final container = ProviderContainer(
        overrides: [
          surfaceSinkProvider.overrideWithValue(sink),
          petPngProvider.overrideWithValue((d, c, s) async => Uint8List(4)),
          phoneDataProvider.overrideWithValue(_Data(controller.stream)),
          clockProvider.overrideWithValue(() => DateTime.utc(2027, 1, 15, 8)),
        ],
      );
      addTearDown(() {
        container.dispose();
        controller.close();
      });
      container.listen(surfaceSyncProvider, (_, _) {});
      container.listen(homeProvider, (_, _) {});
      controller.add(home());
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(sink.calls.last, 'list');
      expect(
        sink.calls.indexWhere((c) => c.startsWith('pet:')),
        lessThan(sink.calls.indexOf('list')),
      );
      final written = jsonDecode(sink.lastList!) as Map<String, dynamic>;
      expect(
        written['writtenAt'],
        DateTime.utc(2027, 1, 15, 8).millisecondsSinceEpoch,
      );
      final pets = sink.calls.where((c) => c.startsWith('pet:')).length;
      controller.add(home());
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(
        sink.calls.where((c) => c.startsWith('pet:')).length,
        pets,
        reason: 'a pet image is written once',
      );
      expect(sink.calls.where((c) => c == 'list').length, 2);
    },
  );

  test('a reachable computer is written again every few minutes; an unreachable one is not', () async {
    for (final (conn, rewritten) in [
      (ConnState.ok, true),
      (ConnState.offline, false),
    ]) {
      final sink = FakeSink();
      final controller = StreamController<HomeData>();
      var clock = DateTime.utc(2027, 1, 15, 8);
      final container = ProviderContainer(
        overrides: [
          surfaceSinkProvider.overrideWithValue(sink),
          surfaceHeartbeatProvider.overrideWithValue(
            const Duration(milliseconds: 40),
          ),
          petPngProvider.overrideWithValue((d, c, s) async => Uint8List(4)),
          phoneDataProvider.overrideWithValue(_Data(controller.stream)),
          clockProvider.overrideWithValue(
            () => clock = clock.add(const Duration(minutes: 5)),
          ),
        ],
      );
      container.listen(surfaceSyncProvider, (_, _) {});
      container.listen(homeProvider, (_, _) {});
      controller.add(home(conn: conn));
      await Future<void>.delayed(const Duration(milliseconds: 150));
      final lists = sink.calls.where((c) => c == 'list').length;
      expect(lists > 1, rewritten, reason: '$conn wrote $lists lists');
      container.dispose();
      await controller.close();
    }
  });

  test('a tap from a surface is handled only inside the app shell, behind the gate', () {
    // The pairing, the "pair again" screen and (when built) the app lock all come before `AppShell`: a surface can only ask,
    // and the shell is what opens a screen, and nothing answers from there.
    final users =
        Directory('lib')
            .listSync(recursive: true)
            .whereType<File>()
            .where(
              (f) => f.path.endsWith('.dart') && !f.path.contains('lib/src/'),
            )
            .where((f) => f.readAsStringSync().contains('surfaceOpenProvider'))
            .map((f) => f.path)
            .toList()
          ..sort();
    expect(users, ['lib/app.dart', 'lib/surfaces/surface_sync.dart']);
    final shell = File('lib/app.dart').readAsStringSync();
    expect(
      shell.indexOf('class AppShell'),
      lessThan(shell.indexOf('surfaceOpenProvider, (_, o)')),
    );
  });

  test(
    'a tap from outside is only a known screen, and a request needs its id',
    () {
      expect(
        SurfaceOpen.tryParse({'target': 'home', 'id': '', 'kind': ''})?.target,
        'home',
      );
      expect(
        SurfaceOpen.tryParse({'target': 'start', 'id': '', 'kind': ''})?.target,
        'start',
      );
      final r = SurfaceOpen.tryParse({
        'target': 'request',
        'id': 'r1',
        'kind': 'request',
      });
      expect((r?.target, r?.id), ('request', 'r1'));
      for (final bad in <Object?>[
        null,
        'home',
        {'target': 'request', 'id': '', 'kind': ''},
        {'target': 'request', 'id': 'x' * 300, 'kind': ''},
        {'target': 'settings', 'id': '', 'kind': ''},
        {'target': 5, 'id': '', 'kind': ''},
        {'target': 'home', 'id': 1, 'kind': ''},
      ]) {
        expect(SurfaceOpen.tryParse(bad), isNull, reason: '$bad');
      }
    },
  );
}

class _Data implements PhoneData {
  const _Data(this.stream);
  final Stream<HomeData> stream;

  @override
  Stream<HomeData> watchHome() => stream;
}
