import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../config/flavor.dart';
import '../config/relay.dart';
import '../keys/platform_secure_keys.dart';
import '../src/rust/api/messages.dart';
import 'link_providers.dart';
import 'link_state.dart';
import 'pairing_service.dart';

enum PairStep { welcome, scan, relay, details, waiting, done, failed }

class PairingState {
  const PairingState({
    this.step = PairStep.welcome,
    this.uri,
    this.relayHost,
    this.failure,
    this.phoneMustUpdate = false,
    this.asked,
    this.computer,
    this.cameraDenied = false,
  });

  final PairStep step;

  /// The scanned code, held until the person has named the phone and chosen the rights.
  final String? uri;

  /// The relay's host, when it is not the one Bobix runs: shown before the person goes on.
  final String? relayHost;
  final PairFailure? failure;
  final bool phoneMustUpdate;
  final PhoneRights? asked;
  final PairedComputer? computer;
  final bool cameraDenied;

  PairingState copyWith({
    PairStep? step,
    String? uri,
    String? relayHost,
    PairFailure? failure,
    bool? phoneMustUpdate,
    PhoneRights? asked,
    PairedComputer? computer,
    bool? cameraDenied,
  }) => PairingState(
    step: step ?? this.step,
    uri: uri ?? this.uri,
    relayHost: relayHost ?? this.relayHost,
    failure: failure ?? this.failure,
    phoneMustUpdate: phoneMustUpdate ?? this.phoneMustUpdate,
    asked: asked ?? this.asked,
    computer: computer ?? this.computer,
    cameraDenied: cameraDenied ?? this.cameraDenied,
  );
}

/// The pairing screens' state: welcome, scan, (the relay, when it is not Bobix's), name and rights, waiting for the
/// computer, done or a failure. Nothing is stored before the computer's answer; leaving at any step leaves nothing behind.
class PairingController extends Notifier<PairingState> {
  PairingAttempt? _attempt;
  bool _busy = false;

  @override
  PairingState build() => const PairingState();

  PairingService _service(Directory dir) => PairingService(
    keys: ref.read(secureKeysProvider),
    store: LinkStateStore(dir),
    connect: ref.read(relayConnectorProvider),
    flavor: ref.read(flavorProvider),
    now: ref.read(clockProvider),
  );

  void toScan() => state = const PairingState(step: PairStep.scan);

  /// The scanner (or the development field) read [text]: a Bobix code with a relay this build may use, or a failure.
  Future<void> scanned(String text) async {
    if (_busy || state.step != PairStep.scan) return;
    _busy = true;
    try {
      final dir = await ref.read(supportDirectoryProvider.future);
      final info = await _service(dir).inspect(text);
      state = PairingState(
        step: isCustomRelay(info.relay) ? PairStep.relay : PairStep.details,
        uri: text,
        relayHost: isCustomRelay(info.relay) ? relayHost(info.relay) : null,
      );
    } on PairException catch (e) {
      state = PairingState(step: PairStep.failed, failure: e.failure);
    } finally {
      _busy = false;
    }
  }

  void relayAccepted() => state = state.copyWith(step: PairStep.details);

  void cameraDenied() =>
      state = const PairingState(step: PairStep.scan, cameraDenied: true);

  /// The person named the phone and chose what to ask for: ask the computer.
  Future<void> submit(String name, PhoneRights rights) async {
    final uri = state.uri;
    if (uri == null || state.step != PairStep.details) return;
    state = state.copyWith(step: PairStep.waiting, asked: rights);
    final dir = await ref.read(supportDirectoryProvider.future);
    final attempt = _attempt = _service(dir)
        .start(PairRequest(uri: uri, phoneName: name.trim(), rights: rights));
    try {
      final computer = await attempt.result;
      state = state.copyWith(step: PairStep.done, computer: computer);
    } on PairException catch (e) {
      // A cancelled pairing already went back to the welcome screen.
      if (e.failure != PairFailure.cancelled) {
        state = PairingState(
          step: PairStep.failed,
          failure: e.failure,
          phoneMustUpdate: e.phoneMustUpdate,
        );
      }
    } finally {
      if (identical(_attempt, attempt)) _attempt = null;
    }
  }

  /// Leaving while the computer has not answered: the attempt ends and nothing is kept.
  void cancel() {
    _attempt?.cancel();
    state = const PairingState();
  }

  void restart() => state = const PairingState(step: PairStep.scan);

  /// "Open Bobix" on the done screen: the link to the new computer starts.
  Future<void> finish() async {
    final c = state.computer;
    if (c == null) return;
    await ref.read(linkControllerProvider.notifier).paired(c);
    state = const PairingState();
  }
}

final pairingControllerProvider =
    NotifierProvider<PairingController, PairingState>(PairingController.new);
