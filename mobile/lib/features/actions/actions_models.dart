// What the action screens show and send. Plain data: the real connection fills these from the computer's `detail`,
// `home` and `result` messages; nothing here knows how they travel.

enum InputKind { bash, write, edit, mcp }

/// A permission request held for this phone, as the computer describes it.
class ToolRequest {
  const ToolRequest({
    required this.id,
    required this.sessionName,
    required this.tool,
    required this.kind,
    required this.input,
    required this.folder,
    required this.waitedMinutes,
    required this.shownHash,
    this.destructive = false,
    this.odd = false,
    this.cut = false,
    this.secondsLeft,
    this.released = false,
  });

  final String id, sessionName, tool, input, folder, shownHash;
  final InputKind kind;
  final int waitedMinutes;

  /// The computer judged the input destructive.
  final bool destructive;

  /// The computer escaped hidden characters in the text (it says so with `odd`): they show as visible escapes.
  final bool odd;

  /// The computer had to cut the input (20 000 bytes): the end is missing, so it cannot be allowed from the phone.
  final bool cut;

  /// Seconds the computer will still wait; null when it waits for as long as it takes (no countdown).
  final int? secondsLeft;

  /// Someone is at the computer: no more pushes, but still answerable here.
  final bool released;
}

enum ClosedBy { computer, otherPhone, expired }

/// What opening a request id gives: still loading (the id is not known yet), closed, or the request itself.
sealed class RequestView {
  const RequestView();
}

class RequestLoading extends RequestView {
  const RequestLoading();
}

class RequestClosed extends RequestView {
  const RequestClosed(this.by, {required this.at, this.phone, this.last});
  final ClosedBy by;
  final String at;
  final String? phone;

  /// The request as this phone last showed it, kept in memory only; null when this phone never loaded it.
  final ToolRequest? last;
}

class RequestOpen extends RequestView {
  const RequestOpen(this.request);
  final ToolRequest request;
}

class PlanDoc {
  const PlanDoc({
    required this.id,
    required this.sessionName,
    required this.markdown,
    required this.shownHash,
    this.cut = false,
  });
  final String id, sessionName, markdown, shownHash;

  /// The computer cut the plan (too long, or longer than the cap once hidden characters were escaped): what is shown is
  /// not all of it, so it cannot be approved from the phone. Sending it back stays possible.
  final bool cut;
}

class QuestionOption {
  const QuestionOption(this.label, [this.detail]);
  final String label;
  final String? detail;
}

/// One question of Claude's, with its options.
class QuestionPart {
  const QuestionPart({
    required this.question,
    required this.options,
    this.multiSelect = false,
  });
  final String question;
  final List<QuestionOption> options;
  final bool multiSelect;
}

/// Claude's questions (up to four at once). Every part is shown and answered; the hash covers all of them.
class QuestionDoc {
  const QuestionDoc({
    required this.id,
    required this.sessionName,
    required this.parts,
    required this.shownHash,
  });
  final String id, sessionName, shownHash;
  final List<QuestionPart> parts;
}

enum SessionPhase { working, waiting, idle }

class SessionDetail {
  const SessionDetail({
    required this.id,
    required this.name,
    required this.project,
    this.issue,
    required this.phase,
    required this.tool,
    this.lines,
    this.lineTimes = const [],
    this.linesAt = '',
    this.stopAllowed = false,
    this.waitingOnRequest,
    this.shownHash = '',
  });
  final String id, name, project, linesAt;
  final int? issue;
  final SessionPhase phase;
  final String tool;

  /// The session's last lines; null when the computer did not grant this phone the right to see them.
  final List<String>? lines;

  /// When each of [lines] happened (Unix ms), in the same order; empty when the computer sent no times.
  final List<int> lineTimes;

  /// `home` says Stop is possible: the session waits on a request held for this phone and the phone has both the
  /// Stop and the Answer right.
  final bool stopAllowed;
  final String? waitingOnRequest;

  /// Hash of what is on screen, carried by Stop.
  final String shownHash;
}

class StartIssue {
  const StartIssue(this.id, this.number, this.title, this.project);
  final String id, title, project;
  final int number;
}

enum StartMode { full, quick }

class StartPreview {
  const StartPreview({
    required this.issue,
    required this.mode,
    required this.prompt,
    required this.promptHash,
    required this.folder,
    this.branch,
    this.folderMissing = false,
    this.lowDisk = false,
    this.freeSpace = '',
    this.needSpace = '',
  });
  final StartIssue issue;
  final StartMode mode;
  final String prompt, promptHash, folder;
  final String? branch;

  /// The repository has no clone on the computer: Start is blocked.
  final bool folderMissing;
  final String freeSpace, needSpace;

  /// The computer is almost out of disk: Full is blocked, Quick is not.
  final bool lowDisk;
}

enum CommandKind { allow, deny, approve, back, answer, stop, start }

/// One signed command. [id] is fresh for each; [shown] is the hash of what was on screen; [text] is what the person
/// typed or chose (a note, an answer) and is sent unchanged.
class Command {
  const Command({
    required this.id,
    required this.kind,
    required this.target,
    required this.shown,
    this.text,
    this.answers,
    this.mode,
  });
  final String id, target, shown;

  /// One answer per question of a multi-part question (the order of the parts); [text] is the answer of a single one.
  final List<String>? answers;
  final CommandKind kind;
  final String? text;
  final StartMode? mode;
}

enum OutcomeKind {
  applied,
  queued,
  notReceived,
  latePc,
  latePhone,
  expired,
  released,
  stale,
  clock,
  refused,
  withdrawn,
  withdrawTooLate,
  sessionEnded,
}

enum ReleasedWhy { restarted, settings, answeredAtComputer }

class CommandOutcome {
  const CommandOutcome(this.kind, {this.phone, this.why, this.reason});
  final OutcomeKind kind;

  /// Which phone answered first (for [OutcomeKind.latePhone]).
  final String? phone;
  final ReleasedWhy? why;

  /// The computer's reason (for [OutcomeKind.refused]).
  final String? reason;
}
