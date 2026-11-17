import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import '../data/models.dart' show ConnState;
import '../features/actions/actions_models.dart';
import '../features/actions/actions_source.dart';
import '../keys/secure_keys.dart';
import '../src/rust/api/core.dart' as core;
import '../src/rust/api/messages.dart' as msg;
import '../src/rust/api/relay.dart';
import 'computer_link.dart';

/// A command that was confirmed (signed in the secure hardware) and waits to be sent.
class _Signed {
  _Signed(this.input, this.counter, this.issuedAt, this.signature);
  final msg.CommandInput input;
  final BigInt counter;
  final BigInt issuedAt;
  final Uint8List signature;
}

/// What the action screens read and send, over the real connection. The computer's `home` and `detail` fill the screens;
/// a command is signed in secure hardware (the biometric step), sealed by the crate and sent once, and its `result` is
/// waited for. A command is never sent twice: when its result does not come, [checkStatus] asks with a `sync`.
class LinkActionsSource implements ActionsSource, BiometricGate {
  LinkActionsSource({
    required this.link,
    required this.keys,
    required this.onKeyLost,
    DateTime Function()? now,
    this.resultWithin = const Duration(seconds: 20),
    this.answerWithin = const Duration(seconds: 10),
  }) : _now = now ?? DateTime.now {
    _results = link.results.listen((r) {
      _outcomes[r.cid] = _outcomeOf(r);
      unawaited(link.forget(r.cid));
    });
    _homes = link.homeChanges.listen(_track);
    if (link.home != null) _track(link.home!);
  }

  final ComputerLink link;
  final SecureKeys keys;

  /// The signing key is really gone: the phone must pair again.
  final void Function() onKeyLost;
  final DateTime Function() _now;

  /// How long a command's result is waited for before it is shown as "held" (the computer may be asleep).
  final Duration resultWithin;

  /// How long an answer to a question of the phone (a `detail`, a start preview) is waited for.
  final Duration answerWithin;

  late final StreamSubscription<Object> _results, _homes;
  final _signed = <String, _Signed>{};
  final _pids = <String, String>{};
  final _outcomes = <String, CommandOutcome>{};
  final _closed = <String, RequestClosed>{};

  /// The last open request this phone loaded, per id. Memory only: never written to disk or a log, dropped on dispose.
  final _lastOpen = <String, ToolRequest>{};
  Set<String> _held = {};

  Future<void> dispose() async {
    await _results.cancel();
    await _homes.cancel();
    _lastOpen.clear();
    _closed.clear();
  }

  int get _nowMs => _now().millisecondsSinceEpoch;

  // ---- what home says ----

  Map<String, dynamic>? get _home => link.home?.home;

  Map<String, dynamic>? _waiting(String id) {
    for (final w
        in (_home?['waiting'] as List? ?? const [])
            .cast<Map<String, dynamic>>()) {
      if (w['id'] == id) return w;
    }
    return null;
  }

  Map<String, dynamic>? _sessionInfo(String id) {
    for (final x
        in (_home?['sessions'] as List? ?? const [])
            .cast<Map<String, dynamic>>()) {
      if (x['id'] == id) return x;
    }
    return null;
  }

  /// An item that was in the last `home` and is not any more is closed: the computer (or another phone) answered it, or
  /// its time ran out.
  void _track(HomeSnapshot snap) {
    final now = {
      for (final w
          in (snap.home['waiting'] as List).cast<Map<String, dynamic>>())
        w['id'] as String: w,
    };
    for (final id in _held) {
      if (now.containsKey(id)) continue;
      _closed[id] = RequestClosed(
        ClosedBy.computer,
        at: _clock(snap.receivedAtMs),
        last: _lastOpen[id],
      );
    }
    _held = now.keys.toSet();
  }

  static String _clock(int ms) {
    final t = DateTime.fromMillisecondsSinceEpoch(ms);
    return '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}';
  }

  @override
  String get computerName => link.computer.name;

  /// Waits for [find] to give something, as `home` changes, for at most [within].
  Future<T?> _until<T>(T? Function() find, Duration within) async {
    final first = find();
    if (first != null) return first;
    final done = Completer<T?>();
    final sub = link.homeChanges.listen((_) {
      final v = find();
      if (v != null && !done.isCompleted) done.complete(v);
    });
    try {
      return await done.future.timeout(within, onTimeout: () => null);
    } finally {
      await sub.cancel();
    }
  }

  // ---- requests, plans, questions, sessions ----

  @override
  Future<RequestView> request(String id) async {
    final closed = _closed[id];
    if (closed != null) return closed;
    final w = await _until(() => _waiting(id), answerWithin);
    if (w == null) {
      return _closed[id] ??
          RequestClosed(
            ClosedBy.computer,
            at: _clock(_nowMs),
            last: _lastOpen[id],
          );
    }
    final d = await link.detail(id, within: answerWithin);
    if (d == null) return const RequestLoading();
    final snap = link.home!;
    final left = w['secondsLeft'] as int?;
    final since = link.waitingSince[id] ?? snap.receivedAtMs;
    final cut = d['cut'] == true;
    final request = ToolRequest(
      id: id,
      sessionName:
          _sessionInfo(w['session'] as String)?['title'] as String? ?? '',
      // The computer names neither the tool nor its kind: the text is shown as a command.
      tool: '',
      kind: InputKind.bash,
      input: d['text'] as String,
      folder: _sessionInfo(w['session'] as String)?['project'] as String? ?? '',
      waitedMinutes: (_nowMs - since) ~/ 60000,
      shownHash: await _shown(d),
      destructive: w['risky'] == true && !cut,
      odd: d['odd'] == true,
      cut: cut,
      secondsLeft: left == null
          ? null
          : left - (_nowMs - snap.receivedAtMs) ~/ 1000,
      released: w['atComputer'] == true,
    );
    _lastOpen[id] = request;
    return RequestOpen(request);
  }

  /// The hash of what this phone shows of an item: of the text as it is displayed, whether it was cut, and the questions.
  Future<String> _shown(Map<String, dynamic> d) => core.shownHash(
    text: d['text'] as String,
    cut: d['cut'] == true,
    questions: [
      for (final q
          in (d['questions'] as List? ?? const []).cast<Map<String, dynamic>>())
        core.ShownQuestion(
          text: q['text'] as String,
          options: (q['options'] as List).cast<String>(),
          multiSelect: q['multiSelect'] == true,
        ),
    ],
  );

  Future<Map<String, dynamic>> _item(String id) async {
    await _until(() => _waiting(id), answerWithin);
    final d = await link.detail(id, within: answerWithin);
    if (d == null) throw StateError('the computer did not send $id');
    return d;
  }

  String _titleOf(String id) {
    final w = _waiting(id);
    return _sessionInfo(w?['session'] as String? ?? '')?['title'] as String? ??
        '';
  }

  @override
  Future<PlanDoc> plan(String id) async {
    final d = await _item(id);
    return PlanDoc(
      id: id,
      sessionName: _titleOf(id),
      markdown: d['text'] as String,
      shownHash: await _shown(d),
      cut: d['cut'] == true,
    );
  }

  @override
  Future<QuestionDoc> question(String id) async {
    final d = await _item(id);
    final qs = (d['questions'] as List).cast<Map<String, dynamic>>();
    return QuestionDoc(
      id: id,
      sessionName: _titleOf(id),
      // Every question is shown, so the hash (over all of them) is of what the person saw.
      parts: [
        for (final q in qs)
          QuestionPart(
            question: q['text'] as String,
            options: [
              for (final o in (q['options'] as List).cast<String>())
                QuestionOption(o),
            ],
            multiSelect: q['multiSelect'] == true,
          ),
      ],
      shownHash: await _shown(d),
    );
  }

  @override
  Future<SessionDetail> session(String id) async {
    final info = _sessionInfo(id);
    final held = (_home?['waiting'] as List? ?? const [])
        .cast<Map<String, dynamic>>()
        .where((w) => w['session'] == id && w['what'] == 'request')
        .firstOrNull;
    final rights = link.computer.rights;
    List<String>? lines;
    var lineTimes = const <int>[];
    var linesAt = '';
    if (rights.activity && link.connected) {
      final d = await link.detail(id, fresh: true, within: answerWithin);
      if (d != null) {
        // The computer sends them oldest first; the screen lists the newest first. `times` (absent from an older
        // computer) has one entry per line; unless it counts the lines it is not used.
        final all = (d['text'] as String).split('\n');
        final raw = d['times'];
        final times = raw is List && raw.length == all.length
            ? [
                for (final x in raw)
                  if (x is num) x.toInt(),
              ]
            : const <int>[];
        final keep = [
          for (var i = 0; i < all.length; i++)
            if (all[i].isNotEmpty) i,
        ].reversed.toList();
        lines = [for (final i in keep) all[i]];
        if (times.length == all.length) {
          lineTimes = [for (final i in keep) times[i]];
        }
        linesAt = _clock(_nowMs);
      }
    }
    // The computer says whether Stop is possible (`canStop`): a held request, and both the Stop and the Answer right.
    final stop = info?['canStop'] == true && held != null;
    return SessionDetail(
      id: id,
      name: info?['title'] as String? ?? '',
      project: info?['project'] as String? ?? '',
      phase: switch (info?['state']) {
        'working' => SessionPhase.working,
        'waiting' || 'atComputer' => SessionPhase.waiting,
        _ => SessionPhase.idle,
      },
      tool: '',
      lines: lines,
      lineTimes: lineTimes,
      linesAt: linesAt,
      stopAllowed: stop,
      waitingOnRequest: held?['id'] as String?,
    );
  }

  // ---- start ----

  /// The computer sends no list of issues a phone could start (`home` has none): the picker is empty until it does.
  @override
  Future<List<StartIssue>> issues() async => const [];

  @override
  Future<StartPreview> startPreview(String issueId, StartMode mode) async {
    final (project, iid) = _splitIssue(issueId);
    final bridgeMode = mode == StartMode.full
        ? msg.StartMode.full
        : msg.StartMode.quick;
    final want = mode == StartMode.full ? 'full' : 'quick';
    final answer = Completer<Map<String, dynamic>>();
    final sub = link.messages.listen((m) async {
      if (m.kind != 'startPrompt') return;
      final j = jsonDecode(m.json) as Map<String, dynamic>;
      if (j['project'] == project &&
          j['iid'] == iid &&
          j['mode'] == want &&
          !answer.isCompleted) {
        answer.complete(j);
      }
    });
    try {
      await link.send(
        await msg.startPreviewMessage(
          project: project,
          iid: iid,
          mode: bridgeMode,
        ),
      );
      final j = await answer.future.timeout(answerWithin);
      final free = (j['freeDisk'] as num).toInt();
      return StartPreview(
        issue: StartIssue(issueId, int.tryParse(iid) ?? 0, '', project),
        mode: mode,
        prompt: j['prompt'] as String,
        promptHash: j['promptHash'] as String,
        folder: project,
        folderMissing: j['folderKnown'] != true,
        lowDisk: free < _needBytes,
        freeSpace: _gb(free),
        needSpace: _gb(_needBytes),
      );
    } finally {
      await sub.cancel();
    }
  }

  /// A Full start needs room for a worktree; the computer's own check is the one that counts (it refuses with `disk`).
  static const _needBytes = 2500000000;
  static String _gb(int bytes) => '${(bytes / 1e9).toStringAsFixed(1)} GB';

  (String, String) _splitIssue(String issueId) {
    final i = issueId.lastIndexOf('#');
    return i < 0
        ? (issueId, '')
        : (issueId.substring(0, i), issueId.substring(i + 1));
  }

  // ---- commands ----

  @override
  String newCommandId() => core.newCommandId();

  msg.CommandInput? _input(Command c) => switch (c.kind) {
    CommandKind.allow || CommandKind.deny => msg.CommandInput.answer(
      id: c.target,
      allow: c.kind == CommandKind.allow,
      shown: c.shown,
    ),
    CommandKind.approve || CommandKind.back => msg.CommandInput.plan(
      id: c.target,
      approve: c.kind == CommandKind.approve,
      note: (c.text == null || c.text!.isEmpty) ? null : c.text,
      shown: c.shown,
    ),
    CommandKind.answer => msg.CommandInput.question(
      id: c.target,
      answers: c.answers ?? [c.text ?? ''],
      shown: c.shown,
    ),
    CommandKind.stop => msg.CommandInput.stop(
      session: c.target,
      request: c.text ?? '',
    ),
    CommandKind.start => () {
      final (project, iid) = _splitIssue(c.target);
      return msg.CommandInput.start(
        project: project,
        iid: iid,
        mode: c.mode == StartMode.quick
            ? msg.StartMode.quick
            : msg.StartMode.full,
        promptHash: c.shown,
      );
    }(),
  };

  /// The biometric step, which is the signature itself: the hardware signs the bytes of this command and asks for the
  /// enrolled biometrics to do it. Nothing is sent here; [send] sends what was signed.
  @override
  Future<GateResult> confirm(String reason, Command command) async {
    final input = _input(command);
    if (input == null ||
        !await msg.rightsAllow(rights: link.computer.rights, input: input)) {
      return GateResult.failed;
    }
    final msg.PreparedCommand prepared;
    final BigInt counter, issuedAt;
    try {
      counter = await link.takeCounter();
      issuedAt = BigInt.from(_nowMs);
      prepared = await msg.prepareCommandFor(
        cid: command.id,
        input: input,
        phoneId: link.identity.deviceId,
        computerId: link.computer.id,
        counter: counter,
        issuedAt: issuedAt,
      );
    } on core.CoreError {
      // The crate refused the command (a field above its limit): nothing was signed.
      return GateResult.failed;
    }
    switch (await keys.sign(
      Uint8List.fromList(prepared.bytesToSign),
      reason: reason,
    )) {
      case Signed(:final signature):
        _signed[command.id] = _Signed(input, counter, issuedAt, signature);
        return GateResult.confirmed;
      case SignCancelled():
        return GateResult.cancelled;
      case SignLockedOut():
        return GateResult.lockedOut;
      case BiometryUnavailable():
        return GateResult.unavailable;
      case KeyInvalidated():
        onKeyLost();
        return GateResult.keyLost;
      case SignFailed():
        return GateResult.failed;
    }
  }

  @override
  Future<CommandOutcome> send(Command command) async {
    final s = _signed.remove(command.id);
    if (s == null) {
      return const CommandOutcome(OutcomeKind.refused, reason: 'not confirmed');
    }
    final message = await msg.finishCommand(
      input: s.input,
      cid: command.id,
      phoneId: link.identity.deviceId,
      computerId: link.computer.id,
      counter: s.counter,
      issuedAt: s.issuedAt,
      signatureDer: s.signature,
    );
    final first = Completer<CommandOutcome>();
    String? pid;
    final events = link.relayEvents.listen((f) {
      if (first.isCompleted) return;
      switch (f) {
        // The relay has it (it says so for every packet). Held means the computer is not there to take it now.
        case RelayFrame_Queued(pid: final p)
            when p == pid &&
                (link.status.conn == ConnState.asleep ||
                    link.status.conn == ConnState.offline):
          first.complete(const CommandOutcome(OutcomeKind.queued));
        case RelayFrame_Dropped(pid: final p) when p == pid:
          unawaited(link.forget(command.id));
          first.complete(
            const CommandOutcome(OutcomeKind.refused, reason: 'dropped'),
          );
        default:
          break;
      }
    });
    final results = link.results.listen((r) {
      if (r.cid == command.id && !first.isCompleted) {
        first.complete(_outcomeOf(r));
      }
    });
    try {
      try {
        pid = await link.sendCommand(
          command.id,
          s.counter,
          Uint8List.fromList(message),
        );
      } on SocketLost {
        return const CommandOutcome(OutcomeKind.refused, reason: 'offline');
      }
      _pids[command.id] = pid;
      return await first.future.timeout(
        resultWithin,
        onTimeout: () =>
            _outcomes[command.id] ?? const CommandOutcome(OutcomeKind.queued),
      );
    } finally {
      await events.cancel();
      await results.cancel();
    }
  }

  @override
  Future<CommandOutcome> checkStatus(String commandId) async {
    final known = _outcomes[commandId];
    if (known != null) return known;
    final answer = Completer<CommandOutcome>();
    final sub = link.results.listen((r) {
      if (r.cid == commandId && !answer.isCompleted) {
        answer.complete(_outcomeOf(r));
      }
    });
    try {
      await link.send(
        await msg.syncMessage(
          waiting: [commandId],
          versionMin: link.versions.min,
          versionMax: link.versions.max,
        ),
      );
      return await answer.future.timeout(
        answerWithin,
        onTimeout: () => const CommandOutcome(OutcomeKind.queued),
      );
    } on SocketLost {
      return const CommandOutcome(OutcomeKind.queued);
    } finally {
      await sub.cancel();
    }
  }

  @override
  Future<CommandOutcome> withdraw(String commandId) async {
    final pid = _pids[commandId];
    if (pid == null) return const CommandOutcome(OutcomeKind.withdrawTooLate);
    final answer = Completer<CommandOutcome>();
    final sub = link.relayEvents.listen((f) {
      if (answer.isCompleted) return;
      switch (f) {
        case RelayFrame_Withdrawn(pid: final p) when p == pid:
          unawaited(link.forget(commandId));
          answer.complete(const CommandOutcome(OutcomeKind.withdrawn));
        case RelayFrame_TooLate(pid: final p) when p == pid:
          answer.complete(const CommandOutcome(OutcomeKind.withdrawTooLate));
        default:
          break;
      }
    });
    try {
      await link.withdraw(pid);
      return await answer.future.timeout(
        answerWithin,
        onTimeout: () => const CommandOutcome(OutcomeKind.withdrawTooLate),
      );
    } on SocketLost {
      return const CommandOutcome(OutcomeKind.withdrawTooLate);
    } finally {
      await sub.cancel();
    }
  }

  /// What the computer's result means for the screen.
  CommandOutcome _outcomeOf(msg.CommandResultInfo r) => switch (r.outcome) {
    msg.ResultKind.ok => const CommandOutcome(OutcomeKind.applied),
    msg.ResultKind.latePc => const CommandOutcome(OutcomeKind.latePc),
    msg.ResultKind.latePhone => CommandOutcome(
      OutcomeKind.latePhone,
      phone: r.by,
    ),
    msg.ResultKind.expired => const CommandOutcome(OutcomeKind.expired),
    msg.ResultKind.released => CommandOutcome(
      OutcomeKind.released,
      why: switch (r.why) {
        'restart' => ReleasedWhy.restarted,
        'settings' => ReleasedWhy.settings,
        _ => ReleasedWhy.answeredAtComputer,
      },
    ),
    msg.ResultKind.stale => const CommandOutcome(OutcomeKind.stale),
    msg.ResultKind.unknown => const CommandOutcome(OutcomeKind.notReceived),
    msg.ResultKind.refused => switch (r.why) {
      'clock' => const CommandOutcome(OutcomeKind.clock),
      'unknown' || 'not-held' => const CommandOutcome(OutcomeKind.sessionEnded),
      _ => CommandOutcome(OutcomeKind.refused, reason: r.why),
    },
  };
}
