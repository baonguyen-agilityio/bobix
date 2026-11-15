// Every check of the shared crate's test vectors, run through the bridge. The host test (`test/core_vectors_test.dart`)
// and the simulator test (`integration_test/core_vectors_test.dart`) call the same function.
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phone/src/rust/api/core.dart';
import 'package:phone/src/rust/api/messages.dart';

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
  await checkMessages(v);
}

PhoneRights rightsOf(Map<String, dynamic> r) => PhoneRights(
  answer: r['answer'],
  plan: r['plan'],
  question: r['question'],
  start: r['start'],
  stop: r['stop'],
  activity: r['activity'],
);

/// The hello of pairing, commands in two steps, the messages the phone sends and parses, and the hashes.
Future<void> checkMessages(Map<String, dynamic> v) async {
  final desktop = v['desktop'] as Map<String, dynamic>,
      phone = v['phone'] as Map<String, dynamic>;
  final protocol = v['protocol'] as Map<String, dynamic>;
  final desktopBoxSecret = unhex(desktop['boxSecret']),
      phoneBoxPublic = unhex(phone['boxPublic']);
  final signPublic = unhex(phone['signPublicSec1']);
  final hello = protocol['hello'] as Map<String, dynamic>;
  final rights = rightsOf(hello['rights']);

  // The MAC input and the whole hello are the committed ones (the MAC is deterministic; only the sealing nonce is random).
  expect(
    hex(
      await helloMacInput(
        pairingUri: protocol['pairingUri'],
        phoneAuthPublic: unhex(phone['authPublic']),
        phoneBoxPublic: phoneBoxPublic,
        signPublicKey: signPublic,
        name: hello['name'],
        rights: rights,
      ),
    ),
    protocol['helloMacInput'],
  );
  final body = await buildHelloBody(
    pairingUri: protocol['pairingUri'],
    authSeed: unhex(phone['authSeed']),
    boxSecret: unhex(phone['boxSecret']),
    signPublicKey: signPublic,
    name: hello['name'],
    rights: rights,
    versionMin: 1,
    versionMax: 1,
    counter: BigInt.from(11),
  );
  expect(
    hex(body.sublist(0, 32)),
    phone['boxPublic'],
    reason: 'the body starts with the phone box key',
  );
  final opened = jsonDecode(
    utf8.decode(
      await openPacket(
        receiverBoxSecret: desktopBoxSecret,
        senderBoxPublic: phoneBoxPublic,
        packet: Uint8List.fromList(body.sublist(32)),
      ),
    ),
  );
  expect(opened['kind'], 'hello');
  expect(Map<String, dynamic>.of(opened)..remove('kind'), hello);
  final h = await peekPacket(
    receiverSends: LinkDirection.toPhone,
    packet: Uint8List.fromList(body.sublist(32)),
  );
  expect(
    (h.direction, h.stream, h.counter),
    (LinkDirection.toDesktop, LinkStream.event, BigInt.from(11)),
  );
  // The committed hello body opens to the same hello.
  final committed = unhex((protocol['helloBody'] as Map)['body']);
  final fromVector = jsonDecode(
    utf8.decode(
      await openPacket(
        receiverBoxSecret: desktopBoxSecret,
        senderBoxPublic: phoneBoxPublic,
        packet: Uint8List.fromList(committed.sublist(32)),
      ),
    ),
  );
  expect(Map<String, dynamic>.of(fromVector)..remove('kind'), hello);
  expect(rights, rightsOf(hello['rights']));
  // A name that is not plain text, an empty one, a bad code or a bad key are refused.
  Future<Object?> hello2(String name, {String? uri, List<int>? seed}) =>
      buildHelloBody(
        pairingUri: uri ?? protocol['pairingUri'],
        authSeed: seed ?? unhex(phone['authSeed']),
        boxSecret: unhex(phone['boxSecret']),
        signPublicKey: signPublic,
        name: name,
        rights: rights,
        versionMin: 1,
        versionMax: 1,
        counter: BigInt.one,
      );
  await expectCoreError(() => hello2(''), CoreError.hello);
  await expectCoreError(
    () => hello2('a\u202eb'),
    CoreError.hello,
    reason: 'a bidirectional control',
  );
  await expectCoreError(
    () => hello2('x' * 65),
    CoreError.hello,
    reason: 'longer than 64 bytes',
  );
  await expectCoreError(() => hello2('ok', uri: 'nope'), CoreError.malformed);
  await expectCoreError(
    () => hello2('ok', seed: Uint8List(3)),
    CoreError.invalidKey,
  );

  // Text and hashes.
  expect(await isPlainText(text: 'Sam\'s phone', max: BigInt.from(64)), isTrue);
  expect(
    await isPlainText(text: 'two\nlines', max: BigInt.from(64)),
    isTrue,
    reason: 'a newline is allowed here (the hello refuses it itself)',
  );
  expect(
    await isPlainText(text: 'a\u200bb', max: BigInt.from(64)),
    isFalse,
    reason: 'a zero-width character',
  );
  expect(await isPlainText(text: 'abc', max: BigInt.from(2)), isFalse);
  final ph = protocol['promptHash'] as Map<String, dynamic>;
  expect(await promptHash(prompt: ph['prompt']), ph['hash']);
  expect(await promptHash(prompt: '${ph['prompt']}!'), isNot(ph['hash']));

  // The auth key signs what the relay asks (Ed25519: the same bytes every time).
  expect(
    hex(
      await signWithAuth(
        authSeed: unhex(desktop['authSeed']),
        boxSecret: desktopBoxSecret,
        message: unhex(protocol['authBytes']),
      ),
    ),
    protocol['authSignature'],
  );
  final authVec = v['authSignature'] as Map<String, dynamic>;
  expect(
    hex(
      await signWithAuth(
        authSeed: unhex(desktop['authSeed']),
        boxSecret: desktopBoxSecret,
        message: unhex(authVec['message']),
      ),
    ),
    authVec['signature'],
  );
  // ... and the phone's own signature verifies under its own public key, and swapping the two secrets does not.
  final mine = await signWithAuth(
    authSeed: unhex(phone['authSeed']),
    boxSecret: phoneBoxPublic.isEmpty
        ? Uint8List(32)
        : unhex(phone['boxSecret']),
    message: unhex('00'),
  );
  expect(
    await verifyAuthSignature(
      authPublic: unhex(phone['authPublic']),
      message: unhex('00'),
      signature: mine,
    ),
    isTrue,
  );
  final swapped = await signWithAuth(
    authSeed: unhex(phone['boxSecret']),
    boxSecret: unhex(phone['authSeed']),
    message: unhex('00'),
  );
  expect(
    await verifyAuthSignature(
      authPublic: unhex(phone['authPublic']),
      message: unhex('00'),
      signature: swapped,
    ),
    isFalse,
  );

  // A command in two steps: the bytes to sign, then the message with the hardware's signature.
  final command = protocol['command'] as Map<String, dynamic>;
  final bodyJson = jsonDecode(protocol['commandBody']) as Map<String, dynamic>;
  final input = CommandInput.answer(
    id: bodyJson['id'],
    allow: bodyJson['choice'] == 'allow',
    shown: bodyJson['shown'],
  );
  final prepared = await prepareCommand(
    input: input,
    phoneId: phone['deviceId'],
    computerId: desktop['deviceId'],
    counter: BigInt.from(protocol['commandCounter']),
    issuedAt: BigInt.from(protocol['issuedAt']),
  );
  expect((prepared.kind, prepared.target), ('answer', 'req1'));
  expect(prepared.cid.length, 32);
  expect(
    hex(prepared.bytesToSign),
    hex(
      await commandBytes(
        version: 1,
        phoneId: phone['deviceId'],
        computerId: desktop['deviceId'],
        kind: 'answer',
        target: 'req1',
        cid: prepared.cid,
        counter: BigInt.from(protocol['commandCounter']),
        issuedAt: BigInt.from(protocol['issuedAt']),
        body: protocol['commandBody'],
      ),
    ),
  );
  expect(
    (await prepareCommand(
      input: input,
      phoneId: phone['deviceId'],
      computerId: desktop['deviceId'],
      counter: BigInt.one,
      issuedAt: BigInt.one,
    )).cid,
    isNot(prepared.cid),
    reason: 'a fresh id each time',
  );
  final message = await finishCommand(
    input: input,
    cid: protocol['cid'],
    phoneId: phone['deviceId'],
    computerId: desktop['deviceId'],
    counter: BigInt.from(protocol['commandCounter']),
    issuedAt: BigInt.from(protocol['issuedAt']),
    signatureDer: b64(command['sig']),
  );
  expect(jsonDecode(utf8.decode(message)), <String, dynamic>{
    'kind': 'cmd',
    ...command,
  });
  // Every kind of command builds, and the rights decide which are allowed (Stop needs the Answer right too).
  final inputs = <CommandInput>[
    input,
    const CommandInput.plan(
      id: 'p1',
      approve: false,
      note: 'smaller',
      shown: 'h',
    ),
    const CommandInput.question(id: 'q1', answers: ['SQLite'], shown: 'h'),
    CommandInput.start(
      project: 'shop',
      iid: '41',
      mode: StartMode.quick,
      promptHash: ph['hash'],
    ),
    const CommandInput.stop(session: 's1', request: 'r1'),
  ];
  for (final c in inputs) {
    final p = await prepareCommand(
      input: c,
      phoneId: phone['deviceId'],
      computerId: desktop['deviceId'],
      counter: BigInt.two,
      issuedAt: BigInt.from(5),
    );
    expect(p.bytesToSign, isNotEmpty);
    expect(
      jsonDecode(
        utf8.decode(
          await finishCommand(
            input: c,
            cid: p.cid,
            phoneId: phone['deviceId'],
            computerId: desktop['deviceId'],
            counter: BigInt.two,
            issuedAt: BigInt.from(5),
            signatureDer: Uint8List.fromList([1, 2, 3]),
          ),
        ),
      )['kind'],
      'cmd',
    );
  }
  const none = PhoneRights(
    answer: false,
    plan: false,
    question: false,
    start: false,
    stop: false,
    activity: false,
  );
  const stopOnly = PhoneRights(
    answer: false,
    plan: false,
    question: false,
    start: false,
    stop: true,
    activity: false,
  );
  const stopAndAnswer = PhoneRights(
    answer: true,
    plan: false,
    question: false,
    start: false,
    stop: true,
    activity: false,
  );
  expect([
    for (final c in inputs) await rightsAllow(rights: none, input: c),
  ], everyElement(isFalse));
  expect(
    await rightsAllow(rights: stopOnly, input: inputs.last),
    isFalse,
    reason: 'stop denies a held request, so it needs answer too',
  );
  expect(await rightsAllow(rights: stopAndAnswer, input: inputs.last), isTrue);
  expect(await rightsAllow(rights: rights, input: inputs[1]), isTrue);
  expect(
    await rightsAllow(rights: rights, input: inputs[3]),
    isFalse,
    reason: 'the vector phone was not given the start right',
  );
  await expectCoreError(
    () => prepareCommand(
      input: CommandInput.plan(
        id: 'p1',
        approve: false,
        note: 'n' * 1001,
        shown: 'h',
      ),
      phoneId: phone['deviceId'],
      computerId: desktop['deviceId'],
      counter: BigInt.one,
      issuedAt: BigInt.one,
    ),
    CoreError.limit,
    reason: 'a note over 1000 bytes',
  );

  // The other messages the phone sends.
  expect(utf8.decode(await pairedAckMessage()), '{"kind":"pairedAck"}');
  expect(
    jsonDecode(
      utf8.decode(
        await syncMessage(waiting: ['a', 'b'], versionMin: 1, versionMax: 1),
      ),
    ),
    {
      'kind': 'sync',
      'waiting': ['a', 'b'],
      'versions': {'min': 1, 'max': 1},
    },
  );
  expect(jsonDecode(utf8.decode(await detailRequestMessage(id: 'r1'))), {
    'kind': 'detail',
    'id': 'r1',
  });
  expect(
    jsonDecode(
      utf8.decode(
        await startPreviewMessage(
          project: 'shop',
          iid: '41',
          mode: StartMode.full,
        ),
      ),
    ),
    {'kind': 'startPreview', 'project': 'shop', 'iid': '41', 'mode': 'full'},
  );
  expect(
    jsonDecode(
      utf8.decode(
        await byeMessage(signature: Uint8List.fromList([251, 255, 254])),
      ),
    ),
    {'kind': 'bye', 'sig': '-__-'},
  );
  await expectCoreError(
    () => syncMessage(
      waiting: List.generate(1001, (i) => '$i'),
      versionMin: 1,
      versionMax: 1,
    ),
    CoreError.limit,
  );

  // What the computer sends is parsed and checked by the crate; the stream follows the kind.
  final revoked = await parseDesktopMessage(
    bytes: utf8.encode('{"kind":"revoked","why":"removed"}'),
  );
  expect((revoked.kind, revoked.stream), ('revoked', LinkStream.event));
  expect(jsonDecode(revoked.json), {'kind': 'revoked', 'why': 'removed'});
  expect(
    (await parseDesktopMessage(
      bytes: utf8.encode('{"kind":"relayMoved","url":"wss://r.example"}'),
    )).kind,
    'relayMoved',
  );
  expect(
    (await parseDesktopMessage(
      bytes: utf8.encode('{"kind":"error","code":"stale"}'),
    )).kind,
    'error',
  );
  await expectCoreError(
    () => parseDesktopMessage(bytes: utf8.encode('{"kind":"nonsense"}')),
    CoreError.malformed,
  );
  await expectCoreError(
    () => parseDesktopMessage(bytes: utf8.encode('{"kind":"home"}')),
    CoreError.malformed,
    reason: 'a home without its fields',
  );
  await expectCoreError(
    () => parseDesktopMessage(bytes: Uint8List.fromList([0xff, 0xfe])),
    CoreError.malformed,
  );
}
