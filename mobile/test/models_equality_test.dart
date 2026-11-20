import 'package:flutter_test/flutter_test.dart';
import 'package:phone/data/models.dart';

Session session({String name = 'Fix it', bool needsYou = false}) =>
    Session(id: 's1', name: name, project: 'shop', stage: Stage.doing, needsYou: needsYou);

HomeData home(List<Session> s, {ConnState conn = ConnState.ok, bool live = true}) =>
    HomeData(computer: 'Mac', conn: conn, lastContact: '10:00', sessions: s, live: live);

void main() {
  test('a session and a home are equal by value', () {
    expect(session(), session());
    expect(session().hashCode, session().hashCode);
    expect(session(), isNot(session(name: 'Other')));
    expect(home([session()]), home([session()]));
    expect(home([session()]).hashCode, home([session()]).hashCode);
    expect(home([session()]), isNot(home([session(needsYou: true)])));
    expect(home([session()]), isNot(home([session(), session()])));
  });

  test('a reconnect still counts as a change: conn and live are part of the value', () {
    expect(home([session()]), isNot(home([session()], conn: ConnState.asleep)));
    expect(home([session()]), isNot(home([session()], live: false)));
  });
}
