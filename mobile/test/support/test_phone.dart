import 'dart:async';
import 'dart:io';

import 'package:phone/config/flavor.dart';
import 'package:phone/keys/secure_keys.dart';
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/link/pairing_service.dart';
import 'package:phone/src/rust/api/core.dart';
import 'package:phone/src/rust/api/messages.dart';

import 'link_harness.dart';
import 'memory_secure_keys.dart';

const all = PhoneRights(
  answer: true,
  plan: true,
  question: true,
  start: true,
  stop: true,
  activity: true,
);

/// One phone: its keys, its private folder and its (cuttable) network.
class TestPhone {
  TestPhone(this.dir) : keys = MemorySecureKeys(), store = LinkStateStore(dir);

  final Directory dir;
  final MemorySecureKeys keys;
  final LinkStateStore store;
  final connector = FaultyConnector();

  PairingService service({
    Flavor flavor = Flavor.dev,
    Duration wait = const Duration(seconds: 20),
  }) => PairingService(
    keys: keys,
    store: store,
    connect: connector.call,
    flavor: flavor,
    waitForComputer: wait,
  );

  Future<PairedComputer> pair(
    String qr, {
    PhoneRights rights = all,
    String name = 'Test phone',
  }) => service()
      .start(PairRequest(uri: qr, phoneName: name, rights: rights))
      .result;

  Future<ComputerLink> link(
    PairedComputer c, {
    Duration Function(int, double)? delay,
  }) async => ComputerLink(
    computer: c,
    keys: keys,
    store: store,
    connect: connector.call,
    flavor: Flavor.dev,
    identity: await identityFromSecrets(
      authSeed: (await keys.secret(SecureKeys.auth))!,
      boxSecret: (await keys.secret(SecureKeys.box))!,
    ),
    versions: await protocolVersions(),
    delay: delay,
  );
}

Future<T> until<T>(
  T? Function() check, {
  Duration within = const Duration(seconds: 15),
  String what = 'the condition',
}) async {
  final end = DateTime.now().add(within);
  while (DateTime.now().isBefore(end)) {
    final v = check();
    if (v != null) return v;
    await Future<void>.delayed(const Duration(milliseconds: 40));
  }
  throw TimeoutException('timed out waiting for $what');
}
