/// What the screens show. The connection to the computer produces these; until it exists [FixturePhoneData] does.
enum Stage { doing, verify, stuck, todo, done }

enum ConnState { ok, noNetwork, relayDown, asleep, offline }

class Session {
  const Session({
    required this.id,
    required this.name,
    required this.project,
    required this.issue,
    required this.stage,
    this.needsYou = false,
    this.answerAtComputer = false,
    this.waitingMinutes = 0,
    this.tool,
    this.pullRequest,
    this.petDesign = 'default',
    this.petColor = 0,
  });

  final String id, name, project;
  final int issue;
  final Stage stage;

  /// A permission request waits for the person.
  final bool needsYou;

  /// It waits, but the computer does not hold the request for phones: it can only be answered at the computer.
  final bool answerAtComputer;
  final int waitingMinutes;
  final String? tool;
  final int? pullRequest;
  final String petDesign;
  final int petColor;

  /// The pet's state: what it acts out.
  String get petState => needsYou
      ? 'waiting'
      : switch (stage) {
          Stage.doing => 'working',
          Stage.verify => 'idle',
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
  });

  final String computer;
  final ConnState conn;

  /// When the computer last sent anything, as the clock time to show ("21:02").
  final String lastContact;
  final List<Session> sessions;
}

class RequestDetail {
  const RequestDetail({
    required this.sessionId,
    required this.tool,
    required this.command,
    required this.folder,
    required this.expiresMinutes,
    this.destructive = false,
    this.hiddenCharacters = false,
    this.dangerous = const [],
  });

  final String sessionId, tool, command, folder;
  final int expiresMinutes;

  /// The command deletes things that cannot be brought back.
  final bool destructive;

  /// The command holds hidden characters (shown in [command] as visible escapes).
  final bool hiddenCharacters;

  /// Parts of [command] to mark.
  final List<String> dangerous;

  bool get isLong =>
      command.length > 240 || '\n'.allMatches(command).length > 5;
}

class SessionDetail {
  const SessionDetail({
    required this.session,
    required this.activity,
    required this.lastContact,
  });

  final Session session;

  /// The last lines the computer sent, newest first.
  final List<String> activity;
  final String lastContact;
}
