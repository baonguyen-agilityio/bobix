import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/src/rust/api/messages.dart';

PairedComputer computer({String? home, int? homeAt}) => PairedComputer(
  id: 'dev1',
  name: 'Studio Mac',
  relay: 'wss://relay.example/ws',
  phoneName: 'Phone',
  desktopBoxPublic: Uint8List.fromList(List.filled(32, 7)),
  rights: const PhoneRights(answer: true, plan: false, question: false, start: false, stop: false, activity: false),
  recvState: '{"n":1}',
  eventReservation: BigInt.from(5),
  home: home,
  homeAt: homeAt,
);

void main() {
  late Directory tmp;
  setUp(() => tmp = Directory.systemTemp.createTempSync('bobix-link-state-'));
  tearDown(() => tmp.deleteSync(recursive: true));
  File f(String n) => File('${tmp.path}/$n');

  test('the home is kept in its own file, not in the state file', () async {
    final store = LinkStateStore(tmp, homeDelay: const Duration(milliseconds: 20));
    final c = computer(home: '{"kind":"home"}', homeAt: 42);
    await store.save(c);
    expect(f('link-state.json').readAsStringSync(), isNot(contains('"home"')));
    store.saveHome(c);
    await Future<void>.delayed(const Duration(milliseconds: 80));
    await store.flush();
    final loaded = (await LinkStateStore(tmp).load())!;
    expect((loaded.home, loaded.homeAt), ('{"kind":"home"}', 42));
    expect(loaded.eventReservation, BigInt.from(5));
  });

  test('changes within the delay are one write, with the latest home', () async {
    final store = LinkStateStore(tmp, homeDelay: const Duration(minutes: 5));
    final c = computer(home: 'a', homeAt: 1);
    store.saveHome(c);
    c
      ..home = 'b'
      ..homeAt = 2;
    store.saveHome(c);
    expect(f('home-cache.json').existsSync(), isFalse);
    await store.flush();
    expect(jsonDecode(f('home-cache.json').readAsStringSync()), {'id': 'dev1', 'home': 'b', 'homeAt': 2});
  });

  test('a state file from before the split is migrated once: the home moves out of it', () async {
    final old = {'computer': {...computer().toJson(), 'home': '{"kind":"home","x":1}', 'homeAt': 7}};
    f('link-state.json').writeAsStringSync(jsonEncode(old));
    final loaded = (await LinkStateStore(tmp).load())!;
    expect((loaded.home, loaded.homeAt), ('{"kind":"home","x":1}', 7), reason: 'the person keeps the home they had');
    expect(f('link-state.json').readAsStringSync(), isNot(contains('"home"')));
    expect(jsonDecode(f('home-cache.json').readAsStringSync()), {'id': 'dev1', 'home': '{"kind":"home","x":1}', 'homeAt': 7});
    final again = (await LinkStateStore(tmp).load())!;
    expect((again.home, again.homeAt), ('{"kind":"home","x":1}', 7));
  });

  test('a cache that cannot be read is left out; the pairing still loads', () async {
    await LinkStateStore(tmp).save(computer());
    f('home-cache.json').writeAsStringSync('{ not json');
    final loaded = (await LinkStateStore(tmp).load())!;
    expect(loaded.home, isNull);
    expect(loaded.id, 'dev1');
  });

  test('a cache that belongs to another computer is ignored', () async {
    final store = LinkStateStore(tmp, homeDelay: const Duration(minutes: 5));
    final old = computer(home: 'old', homeAt: 1);
    store.saveHome(old);
    await store.flush();
    expect(jsonDecode(f('home-cache.json').readAsStringSync())['id'], 'dev1');
    final other = PairedComputer(
      id: 'dev2',
      name: 'Other',
      relay: 'wss://relay.example/ws',
      phoneName: 'Phone',
      desktopBoxPublic: Uint8List.fromList(List.filled(32, 7)),
      rights: old.rights,
      recvState: '{}',
      eventReservation: BigInt.zero,
    );
    await LinkStateStore(tmp).save(other);
    expect((await LinkStateStore(tmp).load())!.home, isNull, reason: 'the old computer\'s sessions never show');
  });

  test('erase removes both files and their temporary files, and nothing is written back', () async {
    final store = LinkStateStore(tmp, homeDelay: const Duration(minutes: 5));
    final c = computer(home: 'h', homeAt: 1);
    await store.save(c);
    store.saveHome(c);
    await store.flush();
    f('home-cache.json.tmp').writeAsStringSync('x');
    store.saveHome(c);
    await store.erase();
    store.saveHome(c);
    await store.flush();
    await store.save(c);
    expect(tmp.listSync(), isEmpty);
  });
}
