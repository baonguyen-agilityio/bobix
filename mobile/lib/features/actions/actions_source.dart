import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../fixtures/fixtures.dart';
import '../../screens/home_screen.dart';
import 'actions_models.dart';
import 'fixture_actions_source.dart';

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
  Future<bool> confirm(String reason);
}

/// The result the fixture gives every command (null: applied). Only the fixture gallery sets it.
class FixtureOutcome extends Notifier<CommandOutcome?> {
  @override
  CommandOutcome? build() => null;
  void set(CommandOutcome? o) => state = o;
}

final fixtureOutcomeProvider =
    NotifierProvider<FixtureOutcome, CommandOutcome?>(FixtureOutcome.new);

/// The fixture today; wiring is one class: return the real `ActionsSource` here.
final actionsSourceProvider = Provider<ActionsSource>(
  (ref) => FixtureActionsSource(forced: ref.watch(fixtureOutcomeProvider)),
);

final biometricGateProvider = Provider<BiometricGate>(
  (ref) => const FixtureBiometricGate(),
);

/// Sending needs network and the relay; nothing is queued on the phone. Reads the connection the Home screen shows.
final canSendProvider = Provider<bool>((ref) {
  final c = ref.watch(connProvider);
  return c != ConnState.noNetwork && c != ConnState.relayDown;
});
