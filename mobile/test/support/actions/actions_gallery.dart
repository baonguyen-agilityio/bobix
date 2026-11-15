import 'dart:io' show Directory, File, Platform;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:phone/features/actions/actions_models.dart';
import 'package:phone/features/actions/plan_screen.dart';
import 'package:phone/features/actions/question_screen.dart';
import 'package:phone/features/actions/request_screen.dart';
import 'package:phone/features/actions/result_screen.dart';
import 'package:phone/features/actions/session_screen.dart';
import 'package:phone/features/actions/start_screen.dart';
import 'package:phone/theme/tokens.dart';

import 'fixture_actions_source.dart';

/// One scenario of the gallery: a screen and, for the screens that send, the result the fixture will give.
class GalleryEntry {
  const GalleryEntry(this.id, this.title, this.build, {this.outcome});
  final String id, title;
  final Widget Function() build;
  final CommandOutcome? outcome;
}

const _cmd = Command(
  id: 'c-demo',
  kind: CommandKind.allow,
  target: 'req-bash',
  shown: 'h',
);
Widget _result(CommandOutcome o, [Command c = _cmd]) =>
    ResultScreen(command: c, first: Future.value(o));

final galleryEntries = <GalleryEntry>[
  GalleryEntry(
    'request',
    'Request: Bash',
    () => const RequestScreen(id: 'req-bash'),
  ),
  GalleryEntry(
    'request-long',
    'Request: long input',
    () => const RequestScreen(id: 'req-long'),
  ),
  GalleryEntry(
    'request-destructive',
    'Request: destructive',
    () => const RequestScreen(id: 'req-destructive'),
  ),
  GalleryEntry(
    'request-hidden',
    'Request: hidden characters',
    () => const RequestScreen(id: 'req-hidden'),
  ),
  GalleryEntry(
    'request-cut',
    'Request: cut input',
    () => const RequestScreen(id: 'req-cut'),
  ),
  GalleryEntry(
    'request-write',
    'Request: Write',
    () => const RequestScreen(id: 'req-write'),
  ),
  GalleryEntry(
    'request-edit',
    'Request: Edit',
    () => const RequestScreen(id: 'req-edit'),
  ),
  GalleryEntry(
    'request-mcp',
    'Request: MCP tool',
    () => const RequestScreen(id: 'req-mcp'),
  ),
  GalleryEntry(
    'request-released',
    'Request: handled at the computer',
    () => const RequestScreen(id: 'req-released'),
  ),
  GalleryEntry(
    'request-nowait',
    'Request: no countdown',
    () => const RequestScreen(id: 'req-nowait'),
  ),
  GalleryEntry(
    'request-expiring',
    'Request: expiring',
    () => const RequestScreen(id: 'req-expiring'),
  ),
  GalleryEntry(
    'request-loading',
    'Request: loading',
    () => const RequestScreen(id: 'req-loading'),
  ),
  GalleryEntry(
    'request-closed-pc',
    'Request: answered at the computer',
    () => const RequestScreen(id: 'req-closed-pc'),
  ),
  GalleryEntry(
    'request-closed-phone',
    'Request: another phone first',
    () => const RequestScreen(id: 'req-closed-phone'),
  ),
  GalleryEntry(
    'request-closed-expired',
    'Request: expired',
    () => const RequestScreen(id: 'req-closed-expired'),
  ),
  GalleryEntry('plan', 'Plan: long', () => const PlanScreen(id: 'plan-long')),
  GalleryEntry(
    'plan-short',
    'Plan: short',
    () => const PlanScreen(id: 'plan-short'),
  ),
  GalleryEntry(
    'question',
    'Question: options',
    () => const QuestionScreen(id: 'q-one'),
  ),
  GalleryEntry(
    'question-multi',
    'Question: multi-select',
    () => const QuestionScreen(id: 'q-multi'),
  ),
  GalleryEntry(
    'question-many',
    'Question: many options',
    () => const QuestionScreen(id: 'q-many'),
  ),
  GalleryEntry(
    'session',
    'Session: last lines',
    () => const SessionScreen(id: 'ses-working'),
  ),
  GalleryEntry(
    'session-stop',
    'Session: Stop',
    () => const SessionScreen(id: 'ses-stoppable'),
  ),
  GalleryEntry(
    'session-no-lines',
    'Session: no right to see lines',
    () => const SessionScreen(id: 'ses-no-lines'),
  ),
  GalleryEntry(
    'start',
    'Start: pick an issue',
    () => const Scaffold(body: SafeArea(child: StartScreen())),
  ),
  GalleryEntry(
    'start-preview',
    'Start: preview',
    () => const StartPreviewScreen(issueId: 'i-cart', mode: StartMode.full),
  ),
  GalleryEntry(
    'start-quick',
    'Start: Quick preview',
    () => const StartPreviewScreen(issueId: 'i-cart', mode: StartMode.quick),
  ),
  GalleryEntry(
    'start-missing',
    'Start: folder missing',
    () => const StartPreviewScreen(issueId: 'i-docs', mode: StartMode.full),
  ),
  GalleryEntry(
    'start-lowdisk',
    'Start: low disk',
    () => const StartPreviewScreen(issueId: 'i-search', mode: StartMode.full),
  ),
  GalleryEntry(
    'result-applied',
    'Result: applied',
    () => _result(const CommandOutcome(OutcomeKind.applied)),
  ),
  GalleryEntry(
    'result-queued',
    'Result: not delivered yet',
    () => _result(const CommandOutcome(OutcomeKind.queued)),
  ),
  GalleryEntry(
    'result-late-pc',
    'Result: too late (computer)',
    () => _result(const CommandOutcome(OutcomeKind.latePc)),
  ),
  GalleryEntry(
    'result-late-phone',
    'Result: too late (another phone)',
    () => _result(
      const CommandOutcome(OutcomeKind.latePhone, phone: 'Anna’s iPhone'),
    ),
  ),
  GalleryEntry(
    'result-expired',
    'Result: expired',
    () => _result(const CommandOutcome(OutcomeKind.expired)),
  ),
  GalleryEntry(
    'result-released',
    'Result: released',
    () => _result(
      const CommandOutcome(
        OutcomeKind.released,
        why: ReleasedWhy.answeredAtComputer,
      ),
    ),
  ),
  GalleryEntry(
    'result-stale',
    'Result: text changed',
    () => _result(const CommandOutcome(OutcomeKind.stale)),
  ),
  GalleryEntry(
    'result-clock',
    'Result: clocks differ',
    () => _result(const CommandOutcome(OutcomeKind.clock)),
  ),
  GalleryEntry(
    'result-not-received',
    'Result: not received',
    () => _result(const CommandOutcome(OutcomeKind.notReceived)),
  ),
  GalleryEntry(
    'result-withdrawn',
    'Result: withdrawn',
    () => _result(const CommandOutcome(OutcomeKind.withdrawn)),
  ),
  GalleryEntry(
    'result-stop-late',
    'Result: session ended',
    () => _result(const CommandOutcome(OutcomeKind.sessionEnded)),
  ),
];

/// A list of every action screen and state on made-up data, to look at them one by one. Opened with
/// `--dart-define=BOBIX_GALLERY=true`; the environment variable `BOBIX_FIXTURE_SCREEN=<id>` (or a dart-define) opens one directly.
class ActionsGallery extends ConsumerStatefulWidget {
  const ActionsGallery({super.key});

  static const enabled = bool.fromEnvironment('BOBIX_GALLERY');

  /// The scenario to open at start: a dart-define, the environment, or the first line of `bobix-screen` in the
  /// app's temporary folder (the last one lets a simulator be driven without a rebuild).
  static String get direct {
    const defined = String.fromEnvironment('BOBIX_FIXTURE_SCREEN');
    if (defined.isNotEmpty) return defined;
    final env = Platform.environment['BOBIX_FIXTURE_SCREEN'];
    if (env != null && env.isNotEmpty) return env;
    final f = File('${Directory.systemTemp.path}/bobix-screen');
    return f.existsSync() ? f.readAsStringSync().trim() : '';
  }

  @override
  ConsumerState<ActionsGallery> createState() => _ActionsGalleryState();
}

class _ActionsGalleryState extends ConsumerState<ActionsGallery> {
  @override
  void initState() {
    super.initState();
    if (ActionsGallery.direct.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        final e = galleryEntries.where((e) => e.id == ActionsGallery.direct);
        if (e.isNotEmpty && mounted) _open(e.first);
      });
    }
  }

  void _open(GalleryEntry e) {
    ref.read(fixtureOutcomeProvider.notifier).set(e.outcome);
    Navigator.of(context)
        .push(MaterialPageRoute<void>(builder: (_) => e.build()));
  }

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Scaffold(
      backgroundColor: t.bg,
      appBar: AppBar(title: const Text('Action screens (fixtures)')),
      body: ListView(
        children: [
          for (final e in galleryEntries)
            ListTile(
              title: Text(e.title),
              subtitle: Text(e.id),
              onTap: () => _open(e),
            ),
        ],
      ),
    );
  }
}
