import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../data/models.dart';
import '../../link/link_providers.dart';
import 'actions_models.dart';

/// Everything the action screens read and every command they send, behind one interface. The real connection
/// implements this one class; the screens do not change.
abstract class ActionsSource {
  /// The computer the answers go to.
  String get computerName;

  /// Resolves while the id is unknown (the screen shows "loading" and the implementation asks `sync`).
  Future<RequestView> request(String id);
  Future<PlanDoc> plan(String id);
  Future<QuestionDoc> question(String id);
  Future<SessionDetail> session(String id);
  Future<List<StartIssue>> issues();

  /// Asks the computer to build the prompt it would send; nothing starts.
  Future<StartPreview> startPreview(String issueId, StartMode mode);

  /// A fresh id for each command; never reused.
  String newCommandId();

  /// Seals and sends the command (the biometric step already happened) and waits for its result.
  Future<CommandOutcome> send(Command command);

  /// A `sync` for a command whose result did not come: the computer answers with its first result for it, or says
  /// it never got it ([OutcomeKind.notReceived]). The command is never sent again.
  Future<CommandOutcome> checkStatus(String commandId);

  /// Takes back a held command: answers "withdrawn" or "too late".
  Future<CommandOutcome> withdraw(String commandId);
}

/// The system's biometric prompt, which is also the prompt of the signature itself. Cancelling sends nothing.
abstract class BiometricGate {
  /// Asks for the biometric that signs [command]. Only [GateResult.confirmed] lets the command go.
  Future<GateResult> confirm(String reason, Command command);
}

/// How the biometric step ended. Each is its own state: a cancel is the person's choice, a lockout clears with the
/// passcode, a missing or turned-off biometric is a setting, and only [keyLost] means the phone must pair again.
enum GateResult {
  confirmed,
  cancelled,
  lockedOut,
  unavailable,
  keyLost,
  failed,
}

/// The real connection implements [ActionsSource] (see `link_actions_source.dart`); tests override this with made-up data.
final actionsSourceProvider = Provider<ActionsSource>(
  (ref) => throw UnimplementedError('no computer is paired'),
);

/// What the computer let this phone do, as one value that changes when the owner changes it there. Screens that load
/// something only a right allows watch it, so a right raised on the computer shows without leaving the screen.
final grantedRightsProvider = Provider<Object?>((ref) => null);

final biometricGateProvider = Provider<BiometricGate>(
  (ref) => throw UnimplementedError('no computer is paired'),
);

/// The connection state the action screens read: what the link says.
class Conn extends Notifier<ConnState> {
  @override
  ConnState build() =>
      ref.watch(linkStatusProvider).value?.conn ?? ConnState.relayDown;
}

final connProvider = NotifierProvider<Conn, ConnState>(Conn.new);

/// Sending needs network and the relay; nothing is queued on the phone. Reads the connection the Home screen shows.
final canSendProvider = Provider<bool>((ref) {
  final c = ref.watch(connProvider);
  return c != ConnState.noNetwork && c != ConnState.relayDown;
});
