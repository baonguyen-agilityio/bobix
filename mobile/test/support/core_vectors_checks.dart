// Every check of the shared crate's test vectors, run through the bridge. The host test (`test/core_vectors_test.dart`)
// and the simulator test (`integration_test/core_vectors_test.dart`) call the same function.
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/src/rust/api/core.dart';

Uint8List unhex(String s) => Uint8List.fromList([
  for (var i = 0; i < s.length; i += 2)
    int.parse(s.substring(i, i + 2), radix: 16),
]);

String hex(List<int> b) =>
    b.map((x) => x.toRadixString(16).padLeft(2, '0')).join();

/// URL-safe base64 without padding, as the protocol writes binary values in JSON and URIs.
Uint8List b64(String s) => base64Url.decode(base64Url.normalize(s));

Future<void> expectCoreError(
  Future<Object?> Function() call,
  CoreError error, {
  String? reason,
}) async {
  try {
    await call();
  } catch (e) {
    expect(e, error, reason: reason);
    return;
  }
  fail(
    'expected $error${reason == null ? '' : ' ($reason)'}, but the call succeeded',
  );
}

LinkStream streamOf(String s) =>
    LinkStream.values.firstWhere((v) => v.name == s.toLowerCase());

LinkDirection directionOf(String s) =>
    s == 'ToPhone' ? LinkDirection.toPhone : LinkDirection.toDesktop;

Future<void> checkVectors(Map<String, dynamic> v) async {
  final desktop = v['desktop'] as Map<String, dynamic>,
      phone = v['phone'] as Map<String, dynamic>;
  final protocol = v['protocol'] as Map<String, dynamic>,
      negative = v['negative'] as Map<String, dynamic>;
  final phoneBoxSecret = unhex(phone['boxSecret']),
      desktopBoxSecret = unhex(desktop['boxSecret']);
  final phoneBoxPublic = unhex(phone['boxPublic']),
      desktopBoxPublic = unhex(desktop['boxPublic']);

  // The stored secrets give back the public keys and the id.
  final id = await identityFromSecrets(
    authSeed: unhex(phone['authSeed']),
    boxSecret: phoneBoxSecret,
  );
  expect(hex(id.authPublic), phone['authPublic']);
  expect(hex(id.boxPublic), phone['boxPublic']);
  expect(id.deviceId, phone['deviceId']);
  expect(
    await deviceId(authPublic: unhex(desktop['authPublic'])),
    desktop['deviceId'],
  );

  // Every envelope opens to its plaintext, from the phone's side and the desktop's, and its header reads back.
  for (final e in (v['envelopes'] as List).cast<Map<String, dynamic>>()) {
    final dir = directionOf(e['dir']), packet = unhex(e['packet']);
    final toPhone = dir == LinkDirection.toPhone;
    final plain = await openPacket(
      receiverBoxSecret: toPhone ? phoneBoxSecret : desktopBoxSecret,
      senderBoxPublic: toPhone ? desktopBoxPublic : phoneBoxPublic,
      packet: packet,
    );
    expect(
      utf8.decode(plain),
      e['plaintext'],
      reason: '${e['dir']} ${e['stream']} ${e['counter']}',
    );
    final h = await peekPacket(
      receiverSends: toPhone ? LinkDirection.toDesktop : LinkDirection.toPhone,
      packet: packet,
    );
    expect(
      (h.version, h.direction, h.stream, h.counter),
      (e['version'], dir, streamOf(e['stream']), BigInt.from(e['counter'])),
    );
  }

  // A packet this phone seals opens on the other side, and the lane and counter are in the clear.
  final sealed = await sealPacket(
    version: 1,
    direction: LinkDirection.toDesktop,
    stream: LinkStream.event,
    counter: BigInt.from(77),
    senderBoxSecret: phoneBoxSecret,
    recipientBoxPublic: desktopBoxPublic,
    plaintext: utf8.encode('{"kind":"ping"}'),
  );
  expect(
    utf8.decode(
      await openPacket(
        receiverBoxSecret: desktopBoxSecret,
        senderBoxPublic: phoneBoxPublic,
        packet: sealed,
      ),
    ),
    '{"kind":"ping"}',
  );
  final header = await peekPacket(
    receiverSends: LinkDirection.toPhone,
    packet: sealed,
  );
  expect(
    (header.direction, header.stream, header.counter),
    (LinkDirection.toDesktop, LinkStream.event, BigInt.from(77)),
  );

  // The negative vectors are refused for the stated reason.
  final tampered = negative['tamperedPacket'] as Map<String, dynamic>;
  await expectCoreError(
    () => openPacket(
      receiverBoxSecret: phoneBoxSecret,
      senderBoxPublic: desktopBoxPublic,
      packet: unhex(tampered['packet']),
    ),
    CoreError.decrypt,
    reason: 'tampered packet',
  );
  final reflected = negative['reflectedPacket'] as Map<String, dynamic>;
  await expectCoreError(
    () => peekPacket(
      receiverSends: LinkDirection.toPhone,
      packet: unhex(reflected['packet']),
    ), // the desktop sends toPhone
    CoreError.reflected,
    reason: 'reflected packet',
  );
  await expectCoreError(
    () =>
        peekPacket(receiverSends: LinkDirection.toPhone, packet: Uint8List(5)),
    CoreError.short,
  );
  for (final k in (negative['lowOrderBoxKeys'] as Map)['keys'] as List) {
    await expectCoreError(
      () => sealPacket(
        version: 1,
        direction: LinkDirection.toDesktop,
        stream: LinkStream.event,
        counter: BigInt.from(1),
        senderBoxSecret: phoneBoxSecret,
        recipientBoxPublic: unhex(k),
        plaintext: const [],
      ),
      CoreError.invalidKey,
      reason: 'low-order box key $k',
    );
  }
  await expectCoreError(
    () => sealPacket(
      version: 1,
      direction: LinkDirection.toDesktop,
      stream: LinkStream.event,
      counter: BigInt.from(1),
      senderBoxSecret: Uint8List(31),
      recipientBoxPublic: desktopBoxPublic,
      plaintext: const [],
    ),
    CoreError.invalidKey,
    reason: 'a key of the wrong length',
  );

  // Signatures are checked by verifying them (a hardware signature is different every time).
  final signPublic = unhex(phone['signPublicSec1']);
  final p256 = v['p256'] as Map<String, dynamic>;
  expect(
    await verifyP256(
      publicKey: signPublic,
      message: unhex(p256['message']),
      signatureDer: unhex(p256['signatureDer']),
    ),
    isTrue,
  );
  final high = negative['highSSignature'] as Map<String, dynamic>;
  expect(
    await verifyP256(
      publicKey: signPublic,
      message: unhex(high['message']),
      signatureDer: unhex(high['signatureDer']),
    ),
    isTrue,
    reason: 'the hardware does not normalise S',
  );
  expect(
    await verifyP256(
      publicKey: signPublic,
      message: unhex('00'),
      signatureDer: unhex(p256['signatureDer']),
    ),
    isFalse,
  );
  final auth = v['authSignature'] as Map<String, dynamic>;
  expect(
    await verifyAuthSignature(
      authPublic: unhex(desktop['authPublic']),
      message: unhex(auth['message']),
      signature: unhex(auth['signature']),
    ),
    isTrue,
  );
  expect(
    await verifyAuthSignature(
      authPublic: unhex(desktop['authPublic']),
      message: unhex('00'),
      signature: unhex(auth['signature']),
    ),
    isFalse,
  );

  // The command: the bytes to sign are reproduced, and the committed signature verifies against them.
  final command = protocol['command'] as Map<String, dynamic>;
  final body = jsonDecode(protocol['commandBody']) as Map<String, dynamic>;
  final bytes = await commandBytes(
    version: 1,
    phoneId: phone['deviceId'],
    computerId: desktop['deviceId'],
    kind: body['kind'],
    target: body['id'],
    cid: protocol['cid'],
    counter: BigInt.from(protocol['commandCounter']),
    issuedAt: BigInt.from(protocol['issuedAt']),
    body: protocol['commandBody'],
  );
  expect(hex(bytes), protocol['commandSignedBytes']);
  expect(
    await verifyP256(
      publicKey: signPublic,
      message: bytes,
      signatureDer: b64(command['sig']),
    ),
    isTrue,
  );
  expect(
    hex(
      await byeBytes(
        version: 1,
        phoneId: phone['deviceId'],
        computerId: desktop['deviceId'],
        counter: BigInt.from(9),
      ),
    ),
    protocol['byeBytes'],
  );
  expect(
    hex(
      (await authBytes(
        relayUrl: 'wss://Relay.Example:443/',
        challenge: protocol['authChallenge'],
      ))!,
    ),
    protocol['authBytes'],
  );
  expect(
    await authBytes(relayUrl: 'ws://relay.example', challenge: 'x'),
    isNull,
  );
  expect((await compressSignPublic(publicKey: signPublic)).length, 33);

  // The pairing code: what a scan gives back, and the addresses that are refused.
  final info = await parsePairingCode(uri: protocol['pairingUri']);
  expect(info.relay, 'wss://relay.example/ws');
  expect(hex(info.desktopAuthPublic), desktop['authPublic']);
  expect(hex(info.desktopBoxPublic), desktop['boxPublic']);
  expect(info.desktopDeviceId, desktop['deviceId']);
  expect(hex(info.relayToken), protocol['relayToken']);
  expect(hex(info.macKey), protocol['macKey']);
  for (final pair in (protocol['relayUrlNormalisation'] as List)) {
    expect(
      await normalizeRelayUrl(url: pair[0]),
      pair[1],
      reason: '${pair[0]}',
    );
  }
  for (final bad in (protocol['relayUrlRefused'] as List).cast<String>()) {
    expect(await relayUrlOk(url: bad), isFalse, reason: bad);
  }
  await expectCoreError(
    () => parsePairingCode(uri: 'not a code'),
    CoreError.malformed,
    reason: 'not a Bobix code',
  );
  await expectCoreError(
    () => parsePairingCode(uri: '${protocol['pairingUri']}&x=1'),
    CoreError.malformed,
    reason: 'an extra field',
  );
  final plain = (protocol['pairingUri'] as String).replaceFirst(
    'wss%3A%2F%2F',
    'ws%3A%2F%2F',
  );
  await expectCoreError(
    () => parsePairingCode(uri: plain),
    CoreError.insecureRelay,
    reason: 'ws:// to a relay that is not on localhost',
  );

  // What the person was shown hashes to the committed value, and any change to it changes the hash.
  for (final c in (protocol['shown'] as List).cast<Map<String, dynamic>>()) {
    final questions = [
      for (final q in (c['questions'] as List).cast<Map<String, dynamic>>())
        ShownQuestion(
          text: q['text'],
          options: (q['options'] as List).cast<String>(),
          multiSelect: q['multiSelect'],
        ),
    ];
    expect(
      await shownHash(text: c['text'], cut: c['cut'], questions: questions),
      c['shown'],
      reason: '${c['what']} ${c['text']}',
    );
    expect(
      await shownHash(
        text: '${c['text']} ',
        cut: c['cut'],
        questions: questions,
      ),
      isNot(c['shown']),
    );
  }

  // New keys: 32 bytes each, never the same twice, and the id follows the public key.
  final a = await generateIdentity(), b = await generateIdentity();
  expect(
    [a.authSeed, a.boxSecret, a.authPublic, a.boxPublic].map((k) => k.length),
    everyElement(32),
  );
  expect(hex(a.authSeed), isNot(hex(b.authSeed)));
  expect(a.deviceId, await deviceId(authPublic: a.authPublic));
  expect(
    (await identityFromSecrets(
      authSeed: a.authSeed,
      boxSecret: a.boxSecret,
    )).deviceId,
    a.deviceId,
  );

  // Counters: the receive window refuses a counter it has seen; the send counter never goes backwards.
  var state = await recvStateNew();
  expect(
    await recvCheck(
      state: state,
      stream: LinkStream.event,
      counter: BigInt.from(5),
    ),
    isTrue,
  );
  state = await recvCommit(
    state: state,
    stream: LinkStream.event,
    counter: BigInt.from(5),
  );
  expect(
    await recvCheck(
      state: state,
      stream: LinkStream.event,
      counter: BigInt.from(5),
    ),
    isFalse,
    reason: 'replayed',
  );
  expect(
    await recvCheck(
      state: state,
      stream: LinkStream.event,
      counter: BigInt.from(6),
    ),
    isTrue,
  );
  expect(
    await recvCheck(
      state: state,
      stream: LinkStream.state,
      counter: BigInt.from(5),
    ),
    isTrue,
    reason: 'streams are independent',
  );
  await expectCoreError(
    () => recvCheck(
      state: 'nonsense',
      stream: LinkStream.event,
      counter: BigInt.from(1),
    ),
    CoreError.malformed,
  );
  final send = SendCounter.resume(
    storedReservation: BigInt.zero,
    nowUnixMs: BigInt.from(1000),
  );
  final first = send.take(), second = send.take();
  expect(first.counter, BigInt.from(1000));
  expect(
    first.reserve,
    BigInt.from(2000),
    reason: 'the first counter asks to store a reservation',
  );
  expect(second.counter, BigInt.from(1001));
  expect(second.reserve, isNull);

  // The protocol version of an authenticated packet.
  await checkVersion(min: 1, max: 1, version: 1);
  await expectCoreError(
    () => checkVersion(min: 1, max: 1, version: 2),
    CoreError.weMustUpdate, // a newer packet: this app is the old one
  );
  await expectCoreError(
    () => checkVersion(min: 2, max: 3, version: 1),
    CoreError.peerMustUpdate, // an older packet: the peer is the old one
  );
}
