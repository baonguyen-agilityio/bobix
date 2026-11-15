// Shows one phone screen on made-up data, for setting it beside the design prototype on an iOS simulator. Not part of the
// app. Build it for the simulator with
//   flutter build ios --simulator --debug -t test/support/design_shots_main.dart
// and launch it with `SIMCTL_CHILD_BOBIX_SHOT=<id> SIMCTL_CHILD_BOBIX_THEME=light|dark SIMCTL_CHILD_BOBIX_LANG=en|vi
// xcrun simctl launch <device> <bundle id>` (or write `<id> <theme> <lang>` to `bobix-shot` in the app's tmp folder); then `xcrun simctl io <device> screenshot out.png`. Everything shown is made up.
import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:phone/app.dart';
import 'package:phone/data/models.dart';
import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/command_flow.dart';
import 'package:phone/features/actions/plan_screen.dart';
import 'package:phone/features/actions/result_screen.dart';
import 'package:phone/features/actions/request_screen.dart';
import 'package:phone/data/phone_data.dart';
import 'package:phone/features/actions/actions_source.dart';
import 'package:phone/keys/platform_secure_keys.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/link/link_providers.dart';
import 'package:phone/link/link_state.dart';
import 'package:phone/link/computer_link.dart';
import 'package:phone/link/pairing_controller.dart';
import 'package:phone/lock/app_lock.dart';
import 'package:phone/push/push_providers.dart';
import 'package:phone/screens/computers_sheet.dart';
import 'package:phone/screens/pair_again_screen.dart';
import 'package:phone/screens/pairing_screens.dart';
import 'package:phone/screens/app_log_screen.dart';
import 'package:phone/screens/widgets/control_center.dart';
import 'package:phone/screens/widgets/home_screen_widgets.dart';
import 'package:phone/screens/widgets/lock_widgets.dart';
import 'package:phone/screens/settings_screen.dart';
import 'package:phone/screens/task_log_screen.dart';
import 'package:phone/screens/unavailable_screen.dart';
import 'package:phone/settings/app_settings.dart';
import 'package:phone/src/rust/api/messages.dart';
import 'package:phone/widgets/glass_tab_bar.dart';

import 'actions/actions_gallery.dart';
import 'actions/fixture_actions_source.dart';
import 'fake_push_platform.dart';
import 'memory_secure_keys.dart';

/// The settings come from the environment or, on a simulator that does not pass one on, from the first line of
/// `bobix-shot` in the app's temporary folder: `<id> <light|dark> <en|vi>`.
String _env(String k, String d) {
  final fromEnv = Platform.environment[k];
  if (fromEnv != null && fromEnv.isNotEmpty) return fromEnv;
  final f = File('${Directory.systemTemp.path}/bobix-shot');
  if (!f.existsSync()) return d;
  final parts = f.readAsLinesSync().first.trim().split(' ');
  final i = switch (k) {
    'BOBIX_SHOT' => 0,
    'BOBIX_THEME' => 1,
    _ => 2,
  };
  return parts.length > i ? parts[i] : d;
}

/// One screen to draw: how to build it, which tab to open in the shell, what to do once it is up.
class Shot {
  const Shot(this.id, this.build, {this.tab, this.after, this.pairing});
  final String id;
  final Widget Function() build;
  final int? tab;
  final Future<void> Function(BuildContext)? after;
  final PairingState? pairing;
}

/// The sessions the design prototype shows on Home, so the two can be set side by side.
class _DesignData implements PhoneData {
  const _DesignData();
  @override
  Stream<HomeData> watchHome() => Stream.value(
    const HomeData(
      computer: 'Studio Mac',
      conn: ConnState.ok,
      lastContact: '21:14',
      sessions: [
        Session(
          id: 's1',
          name: 'Add coupon field to checkout',
          project: 'shop-app',
          issue: 142,
          stage: Stage.doing,
          needsYou: true,
          waitingId: 'req-bash',
          waitingKind: WaitingKind.request,
          waitingMinutes: 2,
          tool: 'Bash',
          petDesign: 'default',
          petColor: 0,
        ),
        Session(
          id: 's3',
          name: 'Rotate staging certificates',
          project: 'infra',
          issue: 31,
          stage: Stage.doing,
          needsYou: true,
          waitingId: 'plan-long',
          waitingKind: WaitingKind.plan,
          waitingMinutes: 6,
          petDesign: 'tty',
          petColor: 3,
        ),
        Session(
          id: 's7',
          name: 'Add onboarding screens',
          project: 'ios-app',
          issue: 82,
          stage: Stage.doing,
          needsYou: true,
          waitingId: 'q-one',
          waitingKind: WaitingKind.question,
          waitingMinutes: 1,
          petDesign: 'default',
          petColor: 0,
        ),
        Session(
          id: 's2',
          name: 'Fix crash on login',
          project: 'ios-app',
          issue: 77,
          stage: Stage.doing,
          tool: 'Edit',
          petDesign: 'miso',
          petColor: 1,
        ),
        Session(
          id: 's4',
          name: 'Update README setup steps',
          project: 'shop-app',
          issue: 138,
          stage: Stage.verify,
          pullRequest: 139,
          petDesign: 'default',
          petColor: 2,
        ),
        Session(
          id: 's6',
          name: 'Fix flaky checkout test',
          project: 'shop-app',
          issue: 129,
          stage: Stage.stuck,
          petDesign: 'default',
          petColor: 5,
        ),
      ],
    ),
  );
}

class _Lang extends Lang {
  _Lang(this.v);
  final String v;
  @override
  String build() => v;
}

class _Settings extends SettingsController {
  _Settings(this.t);
  final ThemeChoice t;
  @override
  Future<AppSettings> build() async =>
      AppSettings(theme: t, requireUnlock: false);
}

class _Conn extends Conn {
  @override
  ConnState build() => ConnState.ok;
}

class _Pairing extends PairingController {
  _Pairing(this.s);
  final PairingState s;
  @override
  PairingState build() => s;
}

class _Link extends LinkController {
  @override
  Future<AppPhase> build() async => const Unpaired();
}

PairedComputer _computer() => PairedComputer(
  id: 'c',
  name: 'Studio Mac',
  relay: 'wss://relay.example',
  phoneName: 'Sam’s iPhone',
  desktopBoxPublic: Uint8List(32),
  rights: const PhoneRights(
    answer: true,
    plan: true,
    question: true,
    start: true,
    stop: true,
    activity: true,
  ),
  recvState: '',
  eventReservation: BigInt.zero,
  pairedAt: DateTime(2026, 11, 11).millisecondsSinceEpoch,
);

void _selectTab(int i) {
  void walk(Element e) {
    final w = e.widget;
    if (w is GlassTabBar) return w.onSelect(i);
    e.visitChildren(walk);
  }

  WidgetsBinding.instance.rootElement!.visitChildren(walk);
}

Widget _shell() => const AppShell();

Future<void> _unlock(BuildContext c) => showModalBottomSheet<void>(
  context: c,
  isScrollControlled: true,
  backgroundColor: Colors.transparent,
  builder: (_) => const UnlockSheet(
    reason: 'Allow this request',
    command: Command(
      id: 'c1',
      kind: CommandKind.allow,
      target: 'req-bash',
      shown: 'h',
    ),
  ),
);

Command _kind(CommandKind k) =>
    Command(id: 'c2', kind: k, target: 'x', shown: 'h');

final shots = <Shot>[
  Shot('task-log', () => const TaskLogScreen(sessionId: 's1')),
  Shot('app-log', () => const AppLogScreen()),
  Shot('widgets', () => const WidgetsScreen()),
  Shot('hs', () => const HomeScreenPreviewScreen()),
  Shot('lock-widgets', () => const LockWidgetsScreen()),
  Shot('cc', () => const ControlCenterScreen()),
  Shot(
    'pair-nobio',
    () => const PairingFlow(),
    pairing: const PairingState(step: PairStep.noBiometrics),
  ),
  Shot('unlock', () => const RequestScreen(id: 'req-bash'), after: _unlock),
  Shot(
    'plan-note',
    () => const PlanNoteScreen(
      plan: PlanDoc(
        id: 'plan-short',
        sessionName: 'Rotate staging certificates',
        markdown: '# Plan\n1. Renew the certificate\n2. Restart the proxy',
        shownHash: 'h',
      ),
    ),
  ),
  Shot(
    'pair-done',
    () => const PairingFlow(),
    pairing: PairingState(
      step: PairStep.done,
      computer: _computer(),
      asked: _computer().rights,
    ),
  ),
  Shot(
    'result-approve',
    () => ResultScreen(
      command: _kind(CommandKind.approve),
      first: Future.value(const CommandOutcome(OutcomeKind.applied)),
    ),
  ),
  Shot(
    'result-start',
    () => ResultScreen(
      command: _kind(CommandKind.start),
      first: Future.value(const CommandOutcome(OutcomeKind.applied)),
    ),
  ),
  Shot(
    'result-deny',
    () => ResultScreen(
      command: _kind(CommandKind.deny),
      first: Future.value(const CommandOutcome(OutcomeKind.applied)),
    ),
  ),
  Shot('home', _shell, tab: 0),
  Shot('pets', _shell, tab: 1),
  Shot('start', _shell, tab: 2),
  Shot('settings', _shell, tab: 3),
  Shot('perms', () => const PermissionsScreen()),
  Shot(
    'computers-sheet',
    _shell,
    tab: 0,
    after: (c) => showComputersSheet(c, computer: 'Studio Mac', waiting: 3),
  ),
  Shot('lock', () => LockScreen(onUnlock: () {})),
  Shot('lock-failed', () => LockScreen(onUnlock: () {}, failed: true)),
  Shot('app-cover', () => const CoverScreen()),
  Shot('unavailable', () => const UnavailableScreen()),
  Shot(
    'pair-again',
    () => const PairAgainScreen(
      phase: PairAgain(computer: 'Studio Mac', removedByComputer: false),
    ),
  ),
  Shot(
    'pair-again-removed',
    () => const PairAgainScreen(
      phase: PairAgain(computer: 'Studio Mac', removedByComputer: true),
    ),
  ),
  Shot('pair', () => const PairingFlow()),
  Shot(
    'pair-scan',
    () => const PairingFlow(),
    pairing: const PairingState(step: PairStep.scan),
  ),
  Shot(
    'pair-name',
    () => const PairingFlow(),
    pairing: const PairingState(step: PairStep.details, uri: 'x'),
  ),
  Shot(
    'pair-waiting',
    () => const PairingFlow(),
    pairing: const PairingState(step: PairStep.waiting),
  ),
  for (final e in galleryEntries) Shot(e.id, e.build),
];

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final id = _env('BOBIX_SHOT', 'home');
  final shot = shots.firstWhere((s) => s.id == id, orElse: () => shots.first);
  runApp(
    _Shooter(
      shot,
      _env('BOBIX_THEME', 'light') == 'dark',
      _env('BOBIX_LANG', 'en'),
    ),
  );
}

class _Shooter extends StatefulWidget {
  const _Shooter(this.shot, this.dark, this.lang);
  final Shot shot;
  final bool dark;
  final String lang;
  @override
  State<_Shooter> createState() => _ShooterState();
}

class _ShooterState extends State<_Shooter> {
  @override
  void initState() {
    super.initState();
    unawaited(_after());
  }

  Future<void> _after() async {
    final s = widget.shot;
    await Future<void>.delayed(const Duration(milliseconds: 900));
    if (s.tab != null) {
      _selectTab(s.tab!);
      await Future<void>.delayed(const Duration(milliseconds: 600));
    }
    if (s.after != null && mounted) {
      final c = _scaffoldContext();
      if (c != null) unawaited(s.after!(c));
    }
  }

  BuildContext? _scaffoldContext() {
    BuildContext? found;
    void walk(Element e) {
      if (found != null) return;
      if (e.widget is Scaffold) {
        found = e;
        return;
      }
      e.visitChildren(walk);
    }

    WidgetsBinding.instance.rootElement!.visitChildren(walk);
    return found;
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.shot;
    final overrides = <Override>[
      langProvider.overrideWith(() => _Lang(widget.lang)),
      settingsProvider.overrideWith(
        () => _Settings(widget.dark ? ThemeChoice.dark : ThemeChoice.light),
      ),
      phoneDataProvider.overrideWithValue(const _DesignData()),
      actionsSourceProvider.overrideWithValue(FixtureActionsSource()),
      biometricGateProvider.overrideWithValue(
        FixtureBiometricGate(
          approve: true,
          after: const Duration(seconds: 600),
        ),
      ),
      connProvider.overrideWith(_Conn.new),
      scannerBuilderProvider.overrideWithValue(
        (context, {required onCode, required onDenied}) => const ColoredBox(
          color: Color(0xFF14161B),
          child: SizedBox.expand(),
        ),
      ),
      pushPlatformProvider.overrideWithValue(FakePushPlatform()),
      secureKeysProvider.overrideWithValue(MemorySecureKeys()),
      supportDirectoryProvider.overrideWith(
        (ref) async => Directory.systemTemp,
      ),
      pairedComputerProvider.overrideWithValue(_computer()),
      linkStatusProvider.overrideWith(
        (ref) => Stream.value(const LinkStatus(conn: ConnState.ok)),
      ),
      linkControllerProvider.overrideWith(_Link.new),
      if (s.pairing != null)
        pairingControllerProvider.overrideWith(() => _Pairing(s.pairing!)),
    ];
    return ProviderScope(
      overrides: overrides,
      child: BobixApp(home: s.build()),
    );
  }
}
