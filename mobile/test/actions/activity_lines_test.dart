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
