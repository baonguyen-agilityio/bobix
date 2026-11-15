// Made-up data. Nothing here is a real session, project or path.
import 'models.dart';
import 'phone_data.dart';

const _sessions = <Session>[
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
    id: 's6',
    name: 'Clean the build folders',
    project: 'blog',
    issue: 14,
    stage: Stage.doing,
    needsYou: true,
    waitingMinutes: 9,
    tool: 'Bash',
    petDesign: 'miso',
    petColor: 3,
  ),
  Session(
    id: 's7',
    name: 'Rename the settings page',
    project: 'shop',
    issue: 40,
    stage: Stage.doing,
    needsYou: true,
    answerAtComputer: true,
    waitingMinutes: 12,
    tool: 'Edit',
    petDesign: 'ship',
    petColor: 1,
  ),
  Session(
    id: 's8',
    name: 'Add the order export',
    project: 'shop',
    issue: 36,
    stage: Stage.doing,
    answeredAtComputer: true,
    tool: 'Bash',
    petDesign: 'tty',
    petColor: 2,
  ),
  Session(
    id: 's9',
    name: 'Tidy the image uploads',
    project: 'blog',
    issue: 17,
    stage: Stage.doing,
    needsYou: true,
    handledAtComputer: true,
    waitingMinutes: 2,
    tool: 'Edit',
    petDesign: 'nota',
    petColor: 5,
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

const _longCommand =
    'npm test -- --coverage --runInBand \\\n'
    '  && npm run lint -- --max-warnings 0 \\\n'
    '  && npm run build -- --mode production \\\n'
    '  && git add -A \\\n'
    '  && git commit -m "Add a dark mode switch to the settings page" \\\n'
    '  && git push origin feature/dark-mode-switch \\\n'
    '  && echo done';

const _requests = <String, RequestDetail>{
  's9': RequestDetail(
    sessionId: 's9',
    tool: 'Edit',
    command: 'src/uploads/resize.ts',
    folder: '~/work/blog',
    expiresMinutes: 7,
  ),
  's8': RequestDetail(
    sessionId: 's8',
    tool: 'Bash',
    command: 'npm run export -- --orders',
    folder: '~/work/shop',
    expiresMinutes: 0,
  ),
  's1': RequestDetail(
    sessionId: 's1',
    tool: 'Bash',
    command: _longCommand,
    folder: '~/work/shop',
    expiresMinutes: 8,
  ),
  's6': RequestDetail(
    sessionId: 's6',
    tool: 'Bash',
    command: 'rm -rf ./build ./out ./node_modules/.cache',
    folder: '~/work/blog',
    expiresMinutes: 6,
    destructive: true,
    dangerous: ['rm -rf'],
  ),
};

const _activity = <String>[
  '21:13  Edit  src/checkout/form.ts',
  '21:12  Bash  npm test -- checkout',
  '21:11  Read  src/checkout/total.ts',
  '21:10  Grep  "applyCoupon"',
  '21:08  Prompt  #38 Fix the cart total rounding',
];

/// The `BOBIX_FIXTURE_CONN` define picks the connection state a run shows (ok, noNetwork, relayDown, asleep, offline).
const _conn = String.fromEnvironment('BOBIX_FIXTURE_CONN');

class FixturePhoneData implements PhoneData {
  const FixturePhoneData({
    this.connection,
    this.mayAnswer = true,
    this.mayStop = true,
  });

  final ConnState? connection;

  /// The rights the computer granted this phone.
  final bool mayAnswer, mayStop;

  ConnState get _state =>
      connection ??
      ConnState.values.firstWhere(
        (c) => c.name == _conn,
        orElse: () => ConnState.ok,
      );

  String get _contact => _state == ConnState.ok ? '21:14' : '21:02';

  @override
  Stream<HomeData> watchHome() => Stream.value(
    HomeData(
      computer: 'Studio Mac',
      conn: _state,
      lastContact: _contact,
      sessions: _sessions,
    ),
  );

  @override
  Future<RequestDetail?> request(String sessionId) async =>
      _requests[sessionId];

  @override
  Future<SessionDetail?> session(String sessionId) async {
    final s = _sessions.where((x) => x.id == sessionId).firstOrNull;
    return s == null
        ? null
        : SessionDetail(
            session: s,
            activity: _activity,
            lastContact: _contact,
            canStop: mayAnswer && mayStop && s.needsYou && !s.answerAtComputer,
          );
  }
}
