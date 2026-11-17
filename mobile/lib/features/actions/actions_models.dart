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

enum ClosedBy { computer, thisPhone, otherPhone, expired }

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

/// One Markdown file that comes with a plan, as the computer sent it: read only.
class PlanFileDoc {
  const PlanFileDoc({
    required this.name,
    required this.path,
    required this.bytes,
    required this.text,
    this.cut = false,
    this.sent = true,
  });
  final String name, path, text;

  /// The file's size on the computer, not of [text].
  final int bytes;

  /// [text] is the start of the file only.
  final bool cut;

  /// [text] was sent. Otherwise the file is only listed (too large, unreadable, not text).
  final bool sent;

  /// The folder of [path] ("./" when the file is at the top of the project).
  String get folder {
    final i = path.lastIndexOf('/');
    return i < 0 ? './' : path.substring(0, i + 1);
  }

  /// A file the person cannot read whole here.
  bool get partial => cut || !sent;
}

class PlanDoc {
  const PlanDoc({
    required this.id,
    required this.sessionName,
    required this.markdown,
    required this.shownHash,
    this.cut = false,
    this.files = const [],
  });
  final String id, sessionName, markdown, shownHash;

  /// The files the plan comes with; the hash covers them, so an approval covers exactly these as shown.
  final List<PlanFileDoc> files;

  /// A file is shortened or not sent: the plan cannot be approved from the phone (sending it back still works).
  bool get filesPartial => files.any((f) => f.partial);

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
  const StartIssue(
    this.id,
    this.number,
    this.title,
    this.project, {
    this.hasSession = false,
  });
  final String id, title, project;
  final int number;

  /// A session on the computer already works on it: the computer refuses a second start.
  final bool hasSession;
}

/// A repository ticked on the computer (Work → Repos), with its open issues. [cloned]: a clone is on the computer,
/// so an issue of it can start; the phone cannot change which repositories are listed.
class StartRepo {
  const StartRepo(
    this.path,
    this.host, {
    this.cloned = true,
    this.issues = const [],
  });
  final String path, host;
  final bool cloned;
  final List<StartIssue> issues;

  /// Names the repository across hosts; what the phone remembers as its choice.
  String get key => '$host/$path';
}

/// Why the list may be short or empty.
enum StartListError {
  /// The computer could not read the issues (gh / glab missing, signed out or offline).
  read,

  /// The computer has not finished its first read.
  reading,

  /// This phone does not hold the Start right.
  noRight,
}

/// What the Start screen lists: the repositories ticked on the computer, each with its issues.
class StartList {
  const StartList(this.repos, {this.error});
  final List<StartRepo> repos;
  final StartListError? error;

  static const empty = StartList([]);

  /// Issues that can be picked from Home's "To do": the ones nothing works on yet.
  List<StartIssue> get todo => [
    for (final r in repos)
      for (final i in r.issues)
        if (!i.hasSession) i,
  ];
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
