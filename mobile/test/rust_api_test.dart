// The bridge's whole public surface, listed. The command-signing key is made and used in secure hardware and never
// passes through Rust, so no function may take or return it. A new function fails this test until someone has read it,
// confirmed it handles no signing secret, and added it here.
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

const exposed = {
  'generateIdentity',
  'identityFromSecrets',
  'deviceId',
  'signWithAuth',
  'verifyAuthSignature',
  'verifyP256',
  'compressSignPublic',
  'parsePairingCode',
  'normalizeRelayUrl',
  'relayUrlOk',
  'sealPacket',
  'peekPacket',
  'openPacket',
  'checkVersion',
  'recvStateNew',
  'recvCheck',
  'recvCommit',
  'commandBytes',
  'byeBytes',
  'authBytes',
  'newCommandId',
  'shownHash',
};

void main() {
  final source = File('lib/src/rust/api/core.dart').readAsStringSync();

  test('the exported functions are the listed ones', () {
    final found = RegExp(
      r'^Future<[^\n]*?> ([a-z]\w*)\(',
      multiLine: true,
    ).allMatches(source).map((m) => m[1]!).toSet();
    expect(found, exposed);
  });

  test('nothing is called a signing secret, and no type carries one', () {
    final lower = source.toLowerCase();
    for (final word in [
      'signsecret',
      'signing_key',
      'signingkey',
      'sign_secret',
      'signseed',
      'sign_seed',
    ]) {
      expect(lower.contains(word), isFalse, reason: word);
    }
    // The only private values the API takes or returns are the `auth` and `box` keys (parameters and fields).
    final secrets = RegExp(
      r'(?:required \S+|final \S+) (\w*(?:secret|seed)\w*)',
      caseSensitive: false,
    ).allMatches(source).map((m) => m[1]!).toSet();
    expect(secrets, {
      'authSeed',
      'boxSecret',
      'senderBoxSecret',
      'receiverBoxSecret',
    });
  });

  test('the generated bridge lists the Rust functions: nothing was added or removed without regenerating', () {
    final rust = File('rust/src/api/core.rs').readAsStringSync();
    String camel(String snake) => snake.replaceAllMapped(
      RegExp(r'_([a-z0-9])'),
      (m) => m[1]!.toUpperCase(),
    );
    final inRust = RegExp(
      r'^pub fn (\w+)',
      multiLine: true,
    ).allMatches(rust).map((m) => camel(m[1]!)).toSet();
    expect(
      inRust,
      exposed,
      reason: 'run flutter_rust_bridge_codegen generate in mobile/, then update the list above',
    );
  });
}
