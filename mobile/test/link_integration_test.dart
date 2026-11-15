// The phone's link code against the real `relay --dev` and the shared crate's `fake-desktop --scenario`, with the real Rust
// core, on this computer. Build the two programs first (see test/support/link_harness.dart) and the Rust library:
//   cargo build --manifest-path rust/Cargo.toml
// Everything here is made up: the desktop and its sessions are the crate's fakes.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/config/flavor.dart';
import 'package:phone/data/models.dart';
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/link_phone_data.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/link/pairing_service.dart';
import 'package:phone/src/rust/api/messages.dart';

import 'support/core_library.dart';
import 'support/link_harness.dart';
import 'support/test_phone.dart';

void main() {
  late Harness relay;
  late Directory tmp;
  final desktops = <FakeDesktop>[];
  final links = <ComputerLink>[];

  setUpAll(() async {
    HttpOverrides.global = null; // flutter_test blocks real sockets otherwise
    await initCoreLibrary();
    relay = await Harness.start();
  });
  tearDownAll(() async => relay.stop());

  setUp(() => tmp = Directory.systemTemp.createTempSync('bobix-link-'));
  tearDown(() async {
    for (final l in links) {
      await l.dispose();
    }
    links.clear();
    for (final d in desktops) {
      await d.stop();
    }
    desktops.clear();
    tmp.deleteSync(recursive: true);
  });

  Future<FakeDesktop> desktop(String scenario) async {
    final d = await relay.desktop(scenario);
    desktops.add(d);
    return d;
  }

  Future<ComputerLink> started(TestPhone p, PairedComputer c) async {
    final l = await p.link(
      c,
      delay: (_, _) => const Duration(milliseconds: 80),
    );
    links.add(l);
    l.resume();
    return l;
  }

  group('pairing', () {
    test('pairs with fewer rights, keeps only the keys and state, and shows the home', () async {
      final d = await desktop('normal');
      final phone = TestPhone(tmp);
      const asked = PhoneRights(
        answer: true,
        plan: true,
        question: false,
        start: false,
        stop: false,
        activity: false,
      );
      final c = await phone.pair(d.qr, rights: asked, name: 'Test phone');
      expect(c.name, 'Studio Mac (demo)');
      expect(c.rights, asked);
      expect(phone.keys.secrets.keys, containsAll(['auth', 'box']));
      expect(phone.keys.signCalls, 0, reason: 'pairing never signs');
      expect(await phone.store.load(), isNotNull);

      final link = await started(phone, c);
      await until(() => link.home, what: 'the first home');
      expect(link.status.conn, ConnState.ok);
      final data = LinkPhoneData(link);
      final home = await data.watchHome().first;
      expect(home.computer, 'Studio Mac (demo)');
      expect(home.sessions.single.name, 'Fix the login retry');
      expect(home.sessions.single.needsYou, isTrue);
      expect(
        home.sessions.single.answerAtComputer,
        isFalse,
        reason: 'it holds a request, which a phone can answer',
      );

      final req = await link.detail('req1');
      expect(req!['text'], 'npm test -- --watch=false');
      expect(
        phone.keys.signCalls,
        0,
        reason: 'reading and reconnecting never ask',
      );
    });

    test('the stored file keeps the last home, never a request', () async {
      final d = await desktop('normal');
      final phone = TestPhone(tmp);
      final c = await phone.pair(d.qr);
      final link = await started(phone, c);
      await until(() => link.home, what: 'home');
      await link.detail('req1');
      final shown = link.detailCache['req1']!['shown'] as String;
      final text = File('${tmp.path}/link-state.json').readAsStringSync();
      expect(
        text,
        contains('Fix the login retry'),
        reason: 'the last home is kept',
      );
      expect(text, isNot(contains(shown)), reason: 'a detail stays in memory');
      expect(text, isNot(contains('toolInput')), reason: 'same');
      expect(text, isNot(contains('"auth"')), reason: 'no key in the file');
    });

    test(
      'two phones answering one code: both are refused and nothing is stored',
      () async {
        final d = await desktop('two-phones');
        final a = TestPhone(Directory('${tmp.path}/a')..createSync());
        final b = TestPhone(Directory('${tmp.path}/b')..createSync());
        // Each result is turned into its error at once: one may fail while the test is still waiting for the other.
        Future<Object?> failure(Future<Object?> f) =>
            f.then<Object?>((_) => null, onError: (Object e) => e);
        final first = failure(a.pair(d.qr));
        await Future<void>.delayed(const Duration(milliseconds: 150));
        final second = failure(b.pair(d.qr));
        for (final f in [first, second]) {
          expect(
            await f,
            isA<PairException>().having(
              (e) => e.failure,
              'failure',
              PairFailure.twoPhones,
            ),
          );
        }
        for (final p in [a, b]) {
          expect(await p.store.load(), isNull);
          expect(p.keys.secrets, isEmpty);
          expect(
            p.keys.hasSigningKey,
            isFalse,
            reason: 'the signing key made for it is deleted',
          );
        }
      },
    );

    test(
      'cancelling while the computer has not answered leaves nothing behind',
      () async {
        final d = await desktop(
          'two-phones',
        ); // answers only after 700 ms: time to cancel
        final phone = TestPhone(tmp);
        final attempt = phone.service().start(
          PairRequest(uri: d.qr, phoneName: 'x', rights: all),
        );
        await Future<void>.delayed(const Duration(milliseconds: 200));
        attempt.cancel();
        await expectLater(
          attempt.result,
          throwsA(
            isA<PairException>().having(
              (e) => e.failure,
              'failure',
              PairFailure.cancelled,
            ),
          ),
        );
        expect(await phone.store.load(), isNull);
        expect(phone.keys.secrets, isEmpty);
        expect(phone.keys.hasSigningKey, isFalse);
      },
    );

    test('a computer that is gone: the code is no longer good', () async {
      final d = await desktop('normal');
      final qr = d.qr;
      await d.stop();
      await Future<void>.delayed(const Duration(milliseconds: 300));
      await expectLater(
        TestPhone(tmp).pair(qr),
        throwsA(
          isA<PairException>().having(
            (e) => e.failure,
            'failure',
            PairFailure.codeUsed,
          ),
        ),
      );
    });

    test('a relay that does not answer', () async {
      final d = await desktop('normal');
      final probe = await ServerSocket.bind('127.0.0.1', 0);
      final closedPort = probe.port;
      await probe.close();
      final moved = d.qr.replaceFirst(
        Uri.encodeComponent(relay.url),
        Uri.encodeComponent('ws://127.0.0.1:$closedPort'),
      );
      expect(moved, isNot(d.qr));
      await expectLater(
        TestPhone(tmp).pair(moved),
        throwsA(
          isA<PairException>().having(
            (e) => e.failure,
            'failure',
            PairFailure.relayUnreachable,
          ),
        ),
      );
    });

    test(
      'what is not a Bobix code, a plain relay in prod, no biometrics',
      () async {
        final d = await desktop('normal');
        final phone = TestPhone(tmp);
        expect(
          () => phone.service().inspect('https://example.com/pair'),
          throwsA(
            isA<PairException>().having(
              (e) => e.failure,
              'failure',
              PairFailure.notBobix,
            ),
          ),
        );
        expect(
          () => phone.service().inspect(''),
          throwsA(
            isA<PairException>().having(
              (e) => e.failure,
              'failure',
              PairFailure.notBobix,
            ),
          ),
        );
        expect(
          () => phone.service(flavor: Flavor.prod).inspect(d.qr),
          throwsA(
            isA<PairException>().having(
              (e) => e.failure,
              'failure',
              PairFailure.insecureRelay,
            ),
          ),
          reason:
              'the fake desktop speaks ws:// to localhost, which prod refuses',
        );
        expect((await phone.service().inspect(d.qr)).relay, relay.url);
        phone.keys.biometrics = false;
        await expectLater(
          phone.pair(d.qr),
          throwsA(
            isA<PairException>().having(
              (e) => e.failure,
              'failure',
              PairFailure.noStrongBiometrics,
            ),
          ),
        );
        expect(phone.keys.secrets, isEmpty);
        expect(await phone.store.load(), isNull);
      },
    );
  });

  group('connection', () {
    test('asleep and offline computers are named', () async {
      final d1 = await desktop('asleep');
      final p1 = TestPhone(Directory('${tmp.path}/1')..createSync());
      final l1 = await started(p1, await p1.pair(d1.qr));
      await until(
        () => l1.status.conn == ConnState.asleep ? true : null,
        what: 'asleep',
      );

      final d2 = await desktop('offline');
      final p2 = TestPhone(Directory('${tmp.path}/2')..createSync());
      final l2 = await started(p2, await p2.pair(d2.qr));
      await until(
        () => l2.status.conn == ConnState.offline ? true : null,
        what: 'offline',
      );
    });

    test('losing the relay: down, then back with the data kept; background closes the socket', () async {
      final d = await desktop('normal');
      final phone = TestPhone(tmp);
      final link = await started(phone, await phone.pair(d.qr));
      await until(() => link.home, what: 'home');

      phone.connector.blocked = true;
      await phone.connector.cutAll();
      await until(
        () =>
            link.status.conn == ConnState.noNetwork ||
                link.status.conn == ConnState.relayDown
            ? true
            : null,
        what: 'the relay lost',
      );
      expect(link.home, isNotNull, reason: 'the last data stays on screen');
      final opened = phone.connector.opened;
      phone.connector.blocked = false;
      await until(
        () => link.status.conn == ConnState.ok ? true : null,
        what: 'back',
      );
      expect(phone.connector.opened, greaterThan(opened));

      await link.pause();
      await until(
        () => phone.connector.stillOpen == 0 ? true : null,
        what: 'the socket closed in the background',
      );
      final before = phone.connector.opened;
      link.resume();
      await until(
        () => phone.connector.opened > before ? true : null,
        what: 'reconnected on return',
      );
      expect(
        phone.keys.signCalls,
        0,
        reason: 'reconnecting never asks for biometrics',
      );
    });

    test('revoked: the phone is told and learns it must pair again', () async {
      final d = await desktop('revoked');
      final phone = TestPhone(tmp);
      final c = await phone.pair(d.qr);
      final link = await started(phone, c);
      await until(
        () => link.status.maybeRevoked ? true : null,
        what: 'revoked',
      );
      // The relay drained the sealed `revoked` when the computer cut the path, so the phone learns it from the relay:
      // "this computer may have disconnected you". (The sealed one is in computer_link_test.dart.)
      expect(link.status.maybeRevoked, isTrue);
      expect(link.status.revokedByComputer, isFalse);
      expect(
        await phone.store.load(),
        isNotNull,
        reason: 'erased only when the person says so',
      );
    });

    test(
      'a late, replayed or far too old push is dropped and the pairing stays',
      () async {
        final d = await desktop('late-push');
        final phone = TestPhone(tmp);
        final c = await phone.pair(d.qr);
        final link = await started(phone, c);
        final seen = <String>[];
        link.messages.listen((m) {
          final j = jsonDecode(m.json) as Map<String, dynamic>;
          if (m.kind == 'notice') seen.add(j['ref'] as String);
        });
        await until(
          () => seen.contains('on-time') ? true : null,
          what: 'the on-time notice',
        );
        await Future<void>.delayed(const Duration(seconds: 1));

        expect(seen, [
          'on-time',
          'jumped',
          'late-inside-window',
          'final',
        ], reason: 'the replay and the far too old packet are dropped');
        expect(link.status.maybeRevoked, isFalse);
        expect(link.status.revokedByComputer, isFalse);
        expect(
          await phone.store.load(),
          isNotNull,
          reason: 'the pairing stays',
        );
      },
    );

    test(
      'relayMoved: the new relay is used after the old one acknowledged',
      () async {
        final d = await desktop('relay-moved');
        final phone = TestPhone(tmp);
        final c = await phone.pair(d.qr);
        final link = await started(phone, c);
        // movedHost is set after the new relay is saved; c.relay changes one await earlier, before the store has it.
        await until(
          () => link.status.movedHost == 'new-relay.example' ? true : null,
          what: 'the move',
        );
        expect(c.relay, 'wss://new-relay.example/ws');
        expect((await phone.store.load())!.relay, 'wss://new-relay.example/ws');
        expect(link.status.revokedByComputer, isFalse);
      },
    );

    test(
      'a request answered at the computer first, and one being handled there',
      () async {
        final d = await desktop('released');
        final phone = TestPhone(tmp);
        final link = await started(phone, await phone.pair(d.qr));
        await until(() => link.home, what: 'home');
        final home = await LinkPhoneData(link).watchHome().first;
        final s = home.sessions.single;
        expect(
          s.handledAtComputer,
          isTrue,
          reason: 'released: someone is at the computer',
        );
        expect(s.needsYou, isTrue, reason: 'still answerable');
      },
    );
  });

  group('backoff', () {
    test('1 s up to 60 s, with jitter', () {
      expect(reconnectDelay(1, 0), const Duration(seconds: 1));
      expect(reconnectDelay(1, 1), const Duration(milliseconds: 1250));
      expect(reconnectDelay(3, 0), const Duration(seconds: 4));
      expect(reconnectDelay(7, 0), const Duration(seconds: 60));
      for (var a = 1; a < 40; a++) {
        for (final r in [0.0, .5, 1.0]) {
          final d = reconnectDelay(a, r);
          expect(d, greaterThanOrEqualTo(const Duration(seconds: 1)));
          expect(d, lessThanOrEqualTo(const Duration(seconds: 60)));
        }
      }
    });
  });
}
