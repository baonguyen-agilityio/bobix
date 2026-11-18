import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/activity_lines.dart';
import 'package:phone/theme/theme.dart';

void main() {
  // The computer sends `{tool} {text}` for a tool and the bare text for anything else.
  test('a line names a tool only when it starts with one', () {
    expect(splitActivityLine('Bash ls -la'), (tool: 'Bash', text: 'ls -la'));
    expect(splitActivityLine('SendMessage'), (tool: 'SendMessage', text: ''));
    expect(splitActivityLine('mcp__server__send_message hello'), (
      tool: 'send_message',
      text: 'hello',
    ));
    expect(splitActivityLine('startup'), (tool: '', text: 'startup'));
    expect(splitActivityLine('Claude needs your permission to use Bash'), (
      tool: '',
      text: 'Claude needs your permission to use Bash',
    ));
  });

  test(
    'a line time is HH:mm or h:mm AM, with the short date when it is not today',
    () {
      final now = DateTime(2026, 11, 17, 18, 0);
      int at(int d, int h, int m) =>
          DateTime(2026, 11, d, h, m).millisecondsSinceEpoch;
      String f(int ms, {bool is24h = true, String lang = 'en'}) =>
          activityTime(ms, now: now, is24h: is24h, lang: lang);
      expect(f(at(17, 9, 5)), '09:05');
      expect(f(at(17, 21, 40), is24h: false), '9:40 PM');
      expect(f(at(17, 0, 7), is24h: false), '12:07 AM');
      expect(f(at(16, 23, 59)), 'Nov 16 23:59');
      expect(f(at(16, 8, 30), is24h: false, lang: 'vi'), '16/11 8:30 SA');
      expect(f(at(17, 13, 0), is24h: false, lang: 'vi'), '1:00 CH');
    },
  );

  testWidgets(
    'each line shows its time; with none sent the list is as before',
    (tester) async {
      final n = DateTime.now();
      final ms = DateTime(n.year, n.month, n.day, 9, 5).millisecondsSinceEpoch;
      Future<void> show(List<int> times) => tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(alwaysUse24HourFormat: true),
          child: MaterialApp(
            theme: bobixTheme(Brightness.light),
            home: Scaffold(
              body: ActivityLines(
                const ['Bash ls', 'Edit a.rs'],
                empty: 'none',
                times: times,
              ),
            ),
          ),
        ),
      );
      await show([ms, ms]);
      expect(find.text('09:05'), findsNWidgets(2));
      await show(const []);
      expect(find.text('09:05'), findsNothing);
      expect(find.text('ls'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a long line is cut at two lines, a tap opens it and a second folds it',
    (tester) async {
      final long = List.filled(40, 'Claude explains what it found').join(' ');
      await tester.pumpWidget(
        MaterialApp(
          theme: bobixTheme(Brightness.light),
          home: Scaffold(
            body: SingleChildScrollView(
              child: SizedBox(
                width: 300,
                child: ActivityLines(['Bash $long'], empty: 'none'),
              ),
            ),
          ),
        ),
      );
      double height() => tester.getSize(find.byType(InkWell)).height;
      final folded = height();
      await tester.tap(find.byType(InkWell));
      await tester.pumpAndSettle();
      expect(height(), greaterThan(folded * 2), reason: 'the whole text shows');
      await tester.tapAt(
        tester.getTopLeft(find.byType(InkWell)) + const Offset(20, 10),
      );
      await tester.pumpAndSettle();
      expect(height(), folded);
    },
  );
}
