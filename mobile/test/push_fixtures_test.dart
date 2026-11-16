// The notices the notification extension opens (push-core/fixtures/push-fixtures.json, made with the keys of the shared
// crate's test vectors) through the bridge the app uses, on this computer: build the library first,
//   cargo build --manifest-path rust/Cargo.toml
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/src/rust/api/core.dart';

import 'support/core_library.dart';

Uint8List _hex(String s) => Uint8List.fromList([
  for (var i = 0; i < s.length; i += 2)
    int.parse(s.substring(i, i + 2), radix: 16),
]);

Uint8List _b64(String s) => base64Url.decode(base64Url.normalize(s));

void main() {
  late Map<String, dynamic> fx;
  late Uint8List phoneSecret, desktopPublic;

  setUpAll(() async {
    await initCoreLibrary();
    fx = jsonDecode(
      File('push-core/fixtures/push-fixtures.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    phoneSecret = _hex(fx['phoneBoxSecretHex'] as String);
    desktopPublic = _b64(
      (fx['computer'] as Map<String, dynamic>)['boxPublicB64'] as String,
    );
  });

  Uint8List packet(String name) =>
      _b64((fx['packets'] as Map<String, dynamic>)[name]['sealed'] as String);
  BigInt counter(String name) => BigInt.from(
    (fx['packets'] as Map<String, dynamic>)[name]['counter'] as int,
  );

  Future<Map<String, dynamic>> open(String name) async => jsonDecode(
    utf8.decode(
      await openPacket(
        receiverBoxSecret: phoneSecret,
        senderBoxPublic: desktopPublic,
        packet: packet(name),
      ),
    ),
  ) as Map<String, dynamic>;

  test('the three kinds of notice open with their fields, and the text is the redacted one', () async {
    final w = await open('waiting');
    expect(
      (w['kind'], w['what'], w['ref'], w['title']),
      ('notice', 'waiting', 'r1', 'shop-app needs you'),
    );
    expect(w['body'], 'Bash wants to run: curl -H [redacted]');
    expect((await open('closed'))['what'], 'closed');
    expect((await open('done'))['what'], 'done');
  });

  test('each notice is on the event stream, travelling to the phone, with the counter the fixture says', () async {
    for (final n in ['waiting', 'closed', 'done', 'waitingOtherRef']) {
      final h = await peekPacket(
        receiverSends: LinkDirection.toDesktop,
        packet: packet(n),
      );
      expect(
        (h.stream, h.direction, h.counter),
        (LinkStream.event, LinkDirection.toPhone, counter(n)),
        reason: n,
      );
    }
  });

  test('a changed packet and a stranger do not open', () async {
    for (final n in ['tampered', 'stranger']) {
      await expectLater(open(n), throwsA(anything), reason: n);
    }
  });

  test('a version outside the range authenticates and is then refused, saying who must update', () async {
    await open('version9');
    await expectLater(
      checkVersion(min: 1, max: 1, version: 9),
      throwsA(anything),
    );
  });

  test('a replayed counter and one older than the window are refused, an unseen one inside it is not', () async {
    var s = await recvStateNew();
    s = await recvCommit(
      state: s,
      stream: LinkStream.event,
      counter: counter('waiting'),
    );
    expect(
      await recvCheck(
        state: s,
        stream: LinkStream.event,
        counter: counter('waiting'),
      ),
      isFalse,
      reason: 'replay',
    );
    s = await recvCommit(
      state: s,
      stream: LinkStream.event,
      counter: counter('high'),
    );
    expect(
      await recvCheck(
        state: s,
        stream: LinkStream.event,
        counter: counter('late'),
      ),
      isFalse,
      reason: 'late',
    );
    expect(
      await recvCheck(
        state: s,
        stream: LinkStream.event,
        counter: counter('inWindow'),
      ),
      isTrue,
    );
  });
}
