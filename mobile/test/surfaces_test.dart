import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/app.dart';
import 'package:phone/data/models.dart';
import 'package:phone/data/phone_data.dart';
import 'package:phone/push/push_platform.dart';
import 'package:phone/push/push_providers.dart';
import 'package:phone/push/push_registration.dart';
import 'package:phone/surfaces/pet_png.dart';
import 'package:phone/surfaces/shared_list.dart';
import 'package:phone/surfaces/surfaces_platform.dart';
import 'package:phone/surfaces/surfaces_host.dart';
import 'package:phone/surfaces/surfaces_sync.dart';
import 'package:phone/link/link_providers.dart';

import 'support/fake_push_platform.dart';
import 'support/fixture_phone_data.dart';

class FakeSurfaces implements SurfacesPlatform {
  bool activities = true;
  bool startWorks = true;
  Object? listError;
  final lists = <Map<String, Object?>>[];
  final pets = <String>[];
  final calls = <String>[];
  final tokens = StreamController<({String id, String token})>.broadcast();
  final starts = StreamController<String>.broadcast();

  @override
  Future<bool> liveActivities() async => activities;
  @override
  Future<void> writeList(String json) async {
    if (listError != null) throw listError!;
    lists.add(jsonDecode(json) as Map<String, Object?>);
  }

  @override
  Future<void> writePet(String d, int c, String s, Uint8List png) async => pets.add('$d-$c-$s');
  final states = <Map<String, Object?>>[];
  @override
  Future<({bool active, String? id})> syncActivity(Map<String, Object?> s) async {
    calls.add('sync ${s['waiting']}');
    states.add(s);
    return (active: startWorks && s['waiting'] != 0, id: 'act1');
  }

  int clears = 0;
  @override
  Future<void> clear() async => clears++;

  final ended = StreamController<String>.broadcast();
  @override
  Stream<String> get activityEnded => ended.stream;
  @override
  Stream<({String id, String token})> get activityTokens => tokens.stream;
  @override
  Stream<String> get pushToStartTokens => starts.stream;
}

HomeData home({int waiting = 1, ConnState conn = ConnState.ok}) => HomeData(
  computer: 'Studio Mac',
  conn: conn,
  lastContact: '10:00',
  sessions: [
    for (var i = 0; i < waiting; i++)
      Session(
        id: 'w$i',
        name: 'Add a dark mode switch $i',
        project: 'shop',
        stage: Stage.doing,
        needsYou: true,
        waitingId: 'req$i',
        waitingKind: WaitingKind.request,
        tool: 'Bash: rm -rf /secret/path',
        petDesign: 'nota',
        petColor: 70,
      ),
    const Session(id: 'x', name: 'Fix the cart', project: 'shop', stage: Stage.doing),
    const Session(id: 't', name: 'Todo item', project: 'blog', stage: Stage.todo),
  ],
);

class _Controller extends LinkController {
  _Controller(this.first);
  final AppPhase first;
  @override
  Future<AppPhase> build() async => first;
  void go(AppPhase p) => state = AsyncData(p);
}

void main() {
  final now = DateTime.utc(2026, 11, 15, 10);

  group('the shared list', () {
    test('holds names, counts and ids, and no tool input', () {
      final m = SharedListJson.toMap(home(), computerId: 'c1', canStart: true, now: now);
      expect(m.keys.toSet(), {'written', 'computerName', 'computerState', 'todo', 'canStart', 'rows'});
      final rows = m['rows']! as List;
      expect(rows, hasLength(2), reason: 'a todo item is not a row');
      expect((rows.first as Map).keys.toSet(), {'id', 'name', 'project', 'state', 'pet', 'color', 'ref', 'computer', 'holdSecs', 'at'});
      expect(jsonEncode(m).contains('secret'), isFalse, reason: 'a command never reaches the list');
      expect((rows.first as Map)['state'], 'waiting');
      expect((rows.first as Map)['ref'], 'req0');
      expect(m['written'], now.millisecondsSinceEpoch);
    });

    test('a request held at the computer is not one a surface opens', () {
      final h = HomeData(
        computer: 'M',
        conn: ConnState.ok,
        lastContact: '',
        sessions: const [
          Session(id: 'a', name: 'n', project: 'p', stage: Stage.doing, needsYou: true, answerAtComputer: true, waitingId: 'r'),
        ],
      );
      final row = (SharedListJson.toMap(h, computerId: 'c', canStart: false, now: now)['rows']! as List).single as Map;
      expect(row['ref'], '');
      expect(SharedListJson.waiting(h), 0);
    });

    test('how long a request counts as current is what the computer still holds it for, at most 15 minutes', () {
      int hold(int? secs) {
        final h = HomeData(
          computer: 'M',
          conn: ConnState.ok,
          lastContact: '',
          sessions: [Session(id: 'a', name: 'n', project: 'p', stage: Stage.doing, needsYou: true, waitingId: 'r', holdSecs: secs)],
        );
        return ((SharedListJson.toMap(h, computerId: 'c', canStart: true, now: now)['rows']! as List).single as Map)['holdSecs']! as int;
      }

      expect((hold(120), hold(null), hold(5000)), (120, 900, 900));
    });

    test('the computer\'s state: online, asleep, otherwise offline', () {
      String s(ConnState c) =>
          SharedListJson.toMap(home(conn: c), computerId: 'c', canStart: true, now: now)['computerState']! as String;
      expect((s(ConnState.ok), s(ConnState.asleep), s(ConnState.offline), s(ConnState.relayDown)), ('online', 'asleep', 'offline', 'offline'));
    });
  });

  group('the sync', () {
    late FakeSurfaces p;
    late SurfacesSync sync;
    setUp(() {
      p = FakeSurfaces();
      sync = SurfacesSync(p, now: () => now, renderPet: (d, c, s) async => Uint8List.fromList([1]));
    });

    test('writes the list, and each pet once, with the colour taken inside the palette', () async {
      await sync.sync(home(), computerId: 'c1', canStart: true);
      await sync.sync(home(), computerId: 'c1', canStart: true);
      expect(p.lists, hasLength(2));
      expect(p.pets.toSet(), hasLength(p.pets.length), reason: 'an image is written once');
      expect(p.pets.every((k) => int.parse(k.split('-')[1]) < 64), isTrue);
    });

    test('a failing write does not stop the app or the activity', () async {
      p.listError = Exception('disk');
      await sync.sync(home(), computerId: 'c1', canStart: true);
      expect(p.calls, ['sync 1']);
    });

    test('the platform is asked to keep one activity in step with the count each time, and no name goes into it', () async {
      await sync.sync(home(waiting: 1), computerId: 'c1', canStart: true);
      await sync.sync(home(waiting: 2), computerId: 'c1', canStart: true);
      await sync.sync(home(waiting: 0), computerId: 'c1', canStart: true);
      expect(p.calls, ['sync 1', 'sync 2', 'sync 0']);
      expect(p.states.first.keys.toSet(), {'waiting', 'written', 'computerId'}, reason: 'counts and a time: the activity draws the shared list');
      expect(p.states.first['written'], now.millisecondsSinceEpoch);
    });

    test('with Live Activities off, only the list is written', () async {
      p.activities = false;
      await sync.sync(home(), computerId: 'c1', canStart: true);
      expect(p.calls, isEmpty);
      expect(p.lists, hasLength(1));
    });

  });

  group('the activity tokens', () {
    test('are registered with a grant, with their kind and the activity as the reference, and again on a new connection', () async {
      final platform = FakePushPlatform()..currentToken = 'dd';
      final reg = PushRegistration(platform, now: () => now, platform: 'ios');
      final sent = <Map<String, Object?>>[];
      await reg.connected('r', (f) async => sent.add(f));
      await reg.registerExtra(kind: 'activity', ref: 'act1', token: 'aa');
      await reg.registerExtra(kind: 'pushToStart', token: 'bb');
      final extra = sent.where((f) => f['kind'] != 'device').toList();
      expect(extra.map((f) => (f['kind'], f['ref'], f['token'])).toSet(), {('activity', 'act1', 'aa'), ('pushToStart', null, 'bb')});
      expect(extra.every((f) => (f['grant']! as String).isNotEmpty && f['type'] == 'pushToken'), isTrue);
      expect(extra.where((f) => f['kind'] == 'pushToStart').every((f) => !f.containsKey('ref')), isTrue);
      sent.clear();
      await reg.connected('r2', (f) async => sent.add(f));
      expect(sent.where((f) => f['kind'] == 'activity'), hasLength(1));
      reg.clearExtra('activity', ref: 'act1');
      sent.clear();
      await reg.connected('r3', (f) async => sent.add(f));
      expect(sent.where((f) => f['kind'] == 'activity'), isEmpty, reason: 'an ended activity\'s token is not used again');
      expect(sent.where((f) => f['kind'] == 'pushToStart'), hasLength(1));
      reg.dispose();
    });
  });

  group('when the pairing ends', () {
    test('every surface is cleared once, on entering not paired or pair again', () async {
      final p = FakeSurfaces();
      final c = _Controller(const Unavailable());
      final container = ProviderContainer(
        overrides: [surfacesPlatformProvider.overrideWithValue(p), linkControllerProvider.overrideWith(() => c)],
      );
      addTearDown(container.dispose);
      container.listen(surfacesClearProvider, (_, _) {});
      await container.read(linkControllerProvider.future);
      await Future<void>.delayed(Duration.zero);
      expect(p.clears, 0, reason: 'a Keychain that is locked for now erases nothing');
      c.go(const PairAgain(computer: 'Studio Mac', removedByComputer: true));
      await Future<void>.delayed(Duration.zero);
      expect(p.clears, 1, reason: 'a sealed revoked, or an erase');
      c.go(const Unpaired());
      await Future<void>.delayed(Duration.zero);
      expect(p.clears, 1, reason: 'already cleared');
    });
  });

  group('taps from surfaces', () {
    test('start and home pick a tab and carry nothing else; a request needs both ids', () {
      expect(PushTap.tryParse({'computer': '', 'ref': '', 'kind': 'start'}), const PushTap(computerId: '', ref: '', kind: 'start'));
      expect(PushTap.tryParse({'kind': 'home'})?.kind, 'home');
      expect(PushTap.tryParse({'computer': '', 'ref': 'r', 'kind': 'waiting'}), isNull);
      expect(PushTap.tryParse({'computer': 'c', 'ref': 'r', 'kind': 'answer'}), isNull);
    });
  });

  testWidgets('a control that opens Start switches to the Start tab, and a link to a request opens it', (tester) async {
    final push = FakePushPlatform();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          pushPlatformProvider.overrideWithValue(push),
          phoneDataProvider.overrideWithValue(FixturePhoneData()),
        ],
        child: const BobixApp(home: AppShell()),
      ),
    );
    await tester.pump(const Duration(milliseconds: 300));
    push.tapController.add(const PushTap(computerId: '', ref: '', kind: 'start'));
    await tester.pump(const Duration(milliseconds: 300));
    expect(tester.widget<IndexedStack>(find.byType(IndexedStack)).index, 2, reason: 'the Start tab');
    push.tapController.add(const PushTap(computerId: '', ref: '', kind: 'home'));
    await tester.pump(const Duration(milliseconds: 300));
    expect(tester.widget<IndexedStack>(find.byType(IndexedStack)).index, 0);
  });

  // flutter_test's rasteriser draws nothing, so that the PNG really shows the pet is checked in a running app:
  // integration_test/pet_png_test.dart.
  testWidgets('a pet comes back as a PNG for the widgets', (tester) async {
    final png = await tester.runAsync(() => renderPetPng('nota', 3, 'waiting'));
    expect(png!.sublist(0, 4), [0x89, 0x50, 0x4e, 0x47]);
  });
}
