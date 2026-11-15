// Made-up data for the screens until the connection exists. Nothing here is a real session, project or path.
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
    this.waitingMinutes = 0,
    this.tool,
    this.pullRequest,
    this.petDesign = 'default',
    this.petColor = 0,
  });

  final String id, name, project;
  final int issue;
  final Stage stage;
  final bool needsYou;
  final int waitingMinutes;
  final String? tool;
  final int? pullRequest;
  final String petDesign;
  final int petColor;
}

const fixtureComputer = 'Studio Mac';

const fixtureSessions = <Session>[
  Session(
    id: 's1',
    name: 'Add a dark mode switch',
    project: 'shop',
    issue: 41,
    stage: Stage.doing,
    needsYou: true,
    waitingMinutes: 4,
    tool: 'Bash',
    petDesign: 'default',
    petColor: 0,
  ),
  Session(
    id: 's2',
    name: 'Fix the cart total rounding',
    project: 'shop',
    issue: 38,
    stage: Stage.doing,
    tool: 'Edit',
    petDesign: 'nota',
    petColor: 2,
  ),
  Session(
    id: 's3',
    name: 'Speed up the search page',
    project: 'blog',
    issue: 12,
    stage: Stage.verify,
    pullRequest: 57,
    petDesign: 'tty',
    petColor: 4,
  ),
  Session(
    id: 's4',
    name: 'Move the docs to a new folder',
    project: 'blog',
    issue: 9,
    stage: Stage.stuck,
    petDesign: 'default',
    petColor: 5,
  ),
  Session(
    id: 's5',
    name: 'Update the sign-up email',
    project: 'shop',
    issue: 35,
    stage: Stage.done,
    petDesign: 'nota',
    petColor: 1,
  ),
];
