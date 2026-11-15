import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:phone/link/relay_session.dart';

/// The relay (`relay --dev`) and the shared crate's `fake-desktop`, started as the real programs. Build them first:
///   cargo build --manifest-path relay/Cargo.toml
///   cargo build --manifest-path crates/bobix-link/Cargo.toml --example fake-desktop
class Harness {
  Harness._(this.url, this._relay);

  final String url;
  final Process _relay;

  static const _relayBinary = '../relay/target/debug/bobix-relay';
  static const _desktopBinary =
      '../crates/bobix-link/target/debug/examples/fake-desktop';

  static void requireBuilt() {
    for (final p in [_relayBinary, _desktopBinary]) {
      if (!File(p).existsSync()) {
        throw StateError(
          '$p is not built. Run the cargo build commands at the top of test/support/link_harness.dart.',
        );
      }
    }
  }

  static Future<Harness> start() async {
    requireBuilt();
    final probe = await ServerSocket.bind('127.0.0.1', 0);
    final port = probe.port;
    await probe.close();
    final relay = await Process.start(
      _relayBinary,
      ['--dev'],
      environment: {'RELAY_ADDR': '127.0.0.1:$port'},
    );
    final ready = Completer<void>();
    relay.stdout.transform(utf8.decoder).listen((t) {
      if (t.contains('listening') && !ready.isCompleted) ready.complete();
    });
    relay.stderr.drain<void>();
    await ready.future.timeout(const Duration(seconds: 10));
    return Harness._('ws://127.0.0.1:$port', relay);
  }

  Future<FakeDesktop> desktop(String scenario) async {
    final p = await Process.start(_desktopBinary, [
      '--relay',
      url,
      '--scenario',
      scenario,
    ]);
    final qr = Completer<String>();
    final lines = <String>[];
    var next = false;
    p.stdout.transform(utf8.decoder).transform(const LineSplitter()).listen((
      l,
    ) {
      lines.add(l);
      if (next && l.startsWith('bobix://') && !qr.isCompleted) qr.complete(l);
      next = l.startsWith('QR string');
    });
    p.stderr.drain<void>();
    return FakeDesktop._(
      p,
      await qr.future.timeout(const Duration(seconds: 10)),
      lines,
    );
  }

  Future<void> stop() async {
    _relay.kill();
    await _relay.exitCode;
  }
}

class FakeDesktop {
  FakeDesktop._(this._process, this.qr, this.lines);
  final Process _process;

  /// The pairing code it shows.
  final String qr;

  /// What it printed.
  final List<String> lines;

  Future<void> stop() async {
    _process.kill();
    await _process.exitCode;
  }
}

/// Opens the real WebSocket, and lets a test cut the connection or refuse new ones, as a lost network would.
class FaultyConnector {
  bool blocked = false;
  int opened = 0;
  final _open = <_Cuttable>[];

  Future<RelaySocket> call(String url) async {
    if (blocked) throw const SocketException('Failed host lookup: test');
    final s = _Cuttable(await WebSocketRelaySocket.connect(url));
    opened++;
    _open.add(s);
    return s;
  }

  /// Closes every open connection from this side.
  Future<void> cutAll() async {
    for (final s in List.of(_open)) {
      await s.close();
    }
  }

  int get stillOpen => _open.where((s) => !s.closed).length;
}

class _Cuttable implements RelaySocket {
  _Cuttable(this._inner);
  final RelaySocket _inner;
  bool closed = false;

  @override
  Stream<String> get frames => _inner.frames;

  @override
  void send(String text) => _inner.send(text);

  @override
  Future<void> close() async {
    closed = true;
    await _inner.close();
  }
}
