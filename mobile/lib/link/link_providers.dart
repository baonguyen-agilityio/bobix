import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import '../config/flavor.dart';
import '../data/phone_data.dart';
import '../keys/platform_secure_keys.dart';
import '../keys/secure_keys.dart';
import '../src/rust/api/core.dart';
import '../features/actions/actions_source.dart';
import 'computer_link.dart';
import 'link_actions_source.dart';
import 'link_phone_data.dart';
import 'link_state.dart';
import 'relay_session.dart';

/// Where the app keeps its private state. App Support is not backed up on iOS (the keys' marker file lives there too).
final supportDirectoryProvider = FutureProvider<Directory>(
  (ref) => getApplicationSupportDirectory(),
);

final relayConnectorProvider = Provider<RelayConnector>(
  (ref) => WebSocketRelaySocket.connect,
);

final clockProvider = Provider<DateTime Function()>((ref) => DateTime.now);

sealed class AppPhase {
  const AppPhase();
}

class Unpaired extends AppPhase {
  const Unpaired();
}

class Paired extends AppPhase {
  const Paired(this.link);
  final ComputerLink link;
}

/// The pairing cannot be opened right now (the Keychain is locked, a read failed): nothing was erased and nothing is paired
/// over it; the person tries again.
class Unavailable extends AppPhase {
  const Unavailable();
}

/// The phone has a pairing it cannot use any more: [removedByComputer] when the computer itself said so (keys and data are
/// already erased), otherwise the relay says it may have been disconnected (or the keys are gone).
class PairAgain extends AppPhase {
  const PairAgain({required this.computer, required this.removedByComputer});
  final String computer;
  final bool removedByComputer;
}

/// Which screen the app is on: not paired, paired (the link runs while the app is in front), or needing a new pairing.
class LinkController extends AsyncNotifier<AppPhase> {
  AppLifecycleListener? _lifecycle;
  StreamSubscription<LinkStatus>? _status;

  @override
  Future<AppPhase> build() async {
    ref.onDispose(() {
      _lifecycle?.dispose();
      _status?.cancel();
    });
    final store = await _store();
    final PairedComputer? computer;
    try {
      computer = await store.load();
    } on LinkStateUnreadable catch (e) {
      // A state that cannot be read is never "no pairing". One that never will be readable is unusable: pair again.
      if (!e.corrupt) return const Unavailable();
      await _forget(store, ref.read(secureKeysProvider));
      return const PairAgain(computer: '', removedByComputer: false);
    }
    if (computer == null) return const Unpaired();
    final keys = ref.read(secureKeysProvider);
    final Uint8List? auth, box;
    try {
      auth = await keys.secret(SecureKeys.auth);
      box = await keys.secret(SecureKeys.box);
    } on SecureKeysException {
      // "Could not read" (a locked device, the app started before the first unlock) is not "gone": keep everything.
      return const Unavailable();
    }
    if (auth == null || box == null) {
      // The platform said the keys are definitely not there (a restore, a wiped keychain): pair again.
      await _forget(store, keys);
      return PairAgain(computer: computer.name, removedByComputer: false);
    }
    return _attach(
      computer,
      await identityFromSecrets(authSeed: auth, boxSecret: box),
    );
  }

  Future<LinkStateStore> _store() async =>
      LinkStateStore(await ref.read(supportDirectoryProvider.future));

  Future<void> _forget(LinkStateStore store, SecureKeys keys) async {
    await keys.eraseAll();
    await store.erase();
  }

  Future<AppPhase> _attach(
    PairedComputer computer,
    DeviceIdentity identity,
  ) async {
    final link = ComputerLink(
      computer: computer,
      keys: ref.read(secureKeysProvider),
      store: await _store(),
      connect: ref.read(relayConnectorProvider),
      flavor: ref.read(flavorProvider),
      identity: identity,
      versions: await protocolVersions(),
      now: ref.read(clockProvider),
    );
    _status = link.statusChanges.listen((s) {
      if (s.maybeRevoked || s.revokedByComputer) {
        state = AsyncData(
          PairAgain(
            computer: computer.name,
            removedByComputer: s.revokedByComputer,
          ),
        );
      }
    });
    // Connected only while the app is in front.
    _lifecycle = AppLifecycleListener(
      onResume: link.resume,
      onHide: () => unawaited(link.pause()),
    );
    final now = WidgetsBinding.instance.lifecycleState;
    if (now == null || now == AppLifecycleState.resumed) link.resume();
    return Paired(link);
  }

  /// Tries to open the pairing again after [Unavailable].
  void retry() => ref.invalidateSelf();

  /// The signing key is gone (the enrolled biometrics changed): the phone must pair again.
  void keyLost() {
    final current = state.value;
    if (current is! Paired) return;
    state = AsyncData(
      PairAgain(computer: current.link.computer.name, removedByComputer: false),
    );
  }

  /// A pairing just ended well: start the link to the new computer.
  Future<void> paired(PairedComputer computer) async {
    final keys = ref.read(secureKeysProvider);
    final auth = await keys.secret(SecureKeys.auth);
    final box = await keys.secret(SecureKeys.box);
    state = AsyncData(
      await _attach(
        computer,
        await identityFromSecrets(authSeed: auth!, boxSecret: box!),
      ),
    );
  }

  /// "Erase and pair again": every key and everything kept about the computer goes.
  Future<void> eraseAndPairAgain() async {
    final current = state.value;
    if (current is Paired) await current.link.dispose();
    await _status?.cancel();
    await _forget(await _store(), ref.read(secureKeysProvider));
    state = const AsyncData(Unpaired());
  }
}

final linkControllerProvider = AsyncNotifierProvider<LinkController, AppPhase>(
  LinkController.new,
);

/// Gives the screens the real data once paired. Used by `main`; tests give [phoneDataProvider] made-up data instead.
final linkPhoneData = phoneDataProvider.overrideWith((ref) {
  final phase = ref.watch(linkControllerProvider).value;
  return phase is Paired ? LinkPhoneData(phase.link) : const NoPhoneData();
});

/// The status of the link (connection state, a problem to act on), for the screens that show a banner.
final linkStatusProvider = StreamProvider<LinkStatus>((ref) async* {
  final phase = ref.watch(linkControllerProvider).value;
  if (phase is! Paired) return;
  yield phase.link.status;
  yield* phase.link.statusChanges;
});

/// The action screens' source and biometric step, over the link: one object, since the signature made at the biometric step
/// is what the send step sends.
final linkActionsProvider = Provider<LinkActionsSource>((ref) {
  final phase = ref.watch(linkControllerProvider).value;
  if (phase is! Paired) throw StateError('no computer is paired');
  final source = LinkActionsSource(
    link: phase.link,
    keys: ref.read(secureKeysProvider),
    onKeyLost: ref.read(linkControllerProvider.notifier).keyLost,
    now: ref.read(clockProvider),
  );
  ref.onDispose(source.dispose);
  return source;
});

/// What `main` overrides: the screens read the link.
final linkActionOverrides = [
  actionsSourceProvider.overrideWith((ref) => ref.watch(linkActionsProvider)),
  biometricGateProvider.overrideWith((ref) => ref.watch(linkActionsProvider)),
];
