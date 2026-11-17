/// What the screens show. The link to the computer produces these (`LinkPhoneData`).
enum Stage { doing, verify, stuck, todo, done }

/// What a waiting item is: a permission request, a plan to approve, or a question of Claude's.
enum WaitingKind { request, plan, question }

enum ConnState { ok, noNetwork, relayDown, asleep, offline }

class Session {
  const Session({
    required this.id,
    required this.name,
    required this.project,
    this.issue,
    required this.stage,
    this.needsYou = false,
    this.answerAtComputer = false,
    this.atComputerWhy,
    this.answeredAtComputer = false,
    this.handledAtComputer = false,
    this.waitingId,
    this.waitingKind,
    this.waitingMinutes = 0,
    this.holdSecs,
    this.tool,
    this.pullRequest,
    this.petDesign = 'default',
    this.petColor = 0,
    this.activityLines = 0,
    this.canStop = false,
  });

  final String id, name, project;

  /// The ticket number, when the computer says one.
  final int? issue;
  final Stage stage;

  /// A permission request waits for the person.
  final bool needsYou;

  /// It waits, but the computer does not hold the request for phones: it can only be answered at the computer.
  final bool answerAtComputer;

  /// Why the computer does not hold it for phones (`present`, `tooLarge`, `unsupported`, `notShown`), when it says.
  final String? atComputerWhy;

  /// It needs the person and can be answered from this phone: what the "needs you" count and look are for.
  bool get answersHere => needsYou && !answerAtComputer;

  /// The request was answered at the computer first (the first answer wins), so it is withdrawn here.
  final bool answeredAtComputer;

  /// Someone is at the computer: the phone gets no pushes for the request, but it can still be answered here and the
  /// first answer wins. Not the same as [answeredAtComputer].
  final bool handledAtComputer;

  /// The id of what waits (the item the action screens open), and what it is. Null when nothing is held for this phone.
  final String? waitingId;
  final WaitingKind? waitingKind;
  final int waitingMinutes;

  /// Seconds the computer still holds the waiting request for phones when `home` came; null when it waits for ever.
  final int? holdSecs;
  final String? tool;
  final int? pullRequest;
  final String petDesign;
  final int petColor;

  /// How many lines of activity the computer has for this session (0 without the right to read them).
  final int activityLines;

  /// The computer lets this phone stop the session (a request is held, and it has both the Stop and the Answer right).
  final bool canStop;

  /// `project #12`, or just the project when there is no ticket.
  String get ref => issue == null ? project : '$project #$issue';

  /// The pet's state: what it acts out.
  String get petState => needsYou
      ? 'waiting'
      : switch (stage) {
          Stage.doing => 'working',
          Stage.verify => 'done',
          Stage.stuck => 'failed',
          _ => 'done',
        };
}

class HomeData {
  const HomeData({
    required this.computer,
    required this.conn,
    required this.lastContact,
    required this.sessions,
    this.live = true,
  });

  final String computer;
  final ConnState conn;

  /// Whether this `home` came over the connection that is up now. The one kept from an earlier run (shown at once, while
  /// the link connects) is not: its zero may be old.
  final bool live;

  /// When the computer last sent anything, as the clock time to show ("21:02").
  final String lastContact;
  final List<Session> sessions;
}
