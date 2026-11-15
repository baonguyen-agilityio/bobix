// The bridge's whole public surface, listed. The command-signing key is made and used in secure hardware and never
// passes through Rust, so no function may take or return it. A new function fails this test until someone has read it,
// confirmed it handles no signing secret, and added it here. Rust functions are counted in every file under
// `rust/src/api`, methods included, and Dart ones in every generated api file, synchronous or not.
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

const exposed = {
  // core.rs
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
  'resume', // SendCounter
  'take', // SendCounter
  // messages.rs
  'promptHash',
  'isPlainText',
  'helloMacInput',
  'buildHelloBody',
  'parseDesktopMessage',
  'pairedAckMessage',
  'syncMessage',
  'detailRequestMessage',
  'startPreviewMessage',
  'byeMessage',
  'prepareCommand',
  'finishCommand',
  'rightsAllow',
};

List<File> filesIn(String dir, String ext, {bool Function(String)? skip}) =>
    Directory(dir)
        .listSync()
        .whereType<File>()
        .where((f) => f.path.endsWith(ext) && !(skip?.call(f.path) ?? false))
        .toList();

String camel(String snake) =>
    snake.replaceAllMapped(RegExp(r'_([a-z0-9])'), (m) => m[1]!.toUpperCase());

void main() {
  final rustFiles = filesIn('rust/src/api', '.rs');
  final rust = rustFiles.map((f) => f.readAsStringSync()).join('\n');
  final dartFiles = filesIn(
    'lib/src/rust/api',
    '.dart',
    skip: (p) => p.endsWith('.freezed.dart'),
  );
  final dart = dartFiles.map((f) => f.readAsStringSync()).join('\n');

  test('the Rust functions are the listed ones: every file, every pub fn, methods too', () {
    expect(rustFiles, isNotEmpty);
    final inRust = RegExp(r'\bpub fn (\w+)')
        .allMatches(rust)
        .map((m) => camel(m[1]!))
        .toSet();
    expect(
      inRust,
      exposed,
      reason: 'a new pub fn: read it, then add it to `exposed` and run flutter_rust_bridge_codegen generate',
    );
  });

  test('the generated Dart has exactly those functions: nothing added or removed without regenerating', () {
    final names = <String>{
      // top level: `Future<T> name(` or a synchronous `T name(`
      ...RegExp(
        r'^(?:Future<[^\n]*?>|[A-Z][\w<>?]*) ([a-z]\w*)\(',
        multiLine: true,
      ).allMatches(dart).map((m) => m[1]!),
      // methods of an opaque class, static or not
      ...RegExp(
        r'^  (?:static )?[A-Z][\w<>?]* ([a-z]\w*)\(',
        multiLine: true,
      ).allMatches(dart).map((m) => m[1]!),
    };
    expect(names, exposed);
  });

  test('no Rust function can reach the signing key: no P-256 secret type, no signing-key name', () {
    for (final word in [
      'SigningKey',
      'SoftSigner',
      'sign_secret',
      'signing_key',
      'sign_seed',
      'p256::ecdsa::Signing',
      'SecretKey::from(sign',
    ]) {
      expect(rust.contains(word), isFalse, reason: word);
    }
    final lower = dart.toLowerCase();
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
    ).allMatches(dart).map((m) => m[1]!).toSet();
    expect(secrets, {
      'authSeed',
      'boxSecret',
      'senderBoxSecret',
      'receiverBoxSecret',
    });
  });
}
