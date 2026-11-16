import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/activity_lines.dart';

void main() {
  test('an activity line splits into the tool and what it did', () {
    expect(splitActivityLine('Bash ls -la'), (
      time: '',
      tool: 'Bash',
      text: 'ls -la',
    ));
    expect(splitActivityLine('SendMessage'), (
      time: '',
      tool: 'SendMessage',
      text: '',
    ));
    expect(splitActivityLine('mcp__server__send_message hello'), (
      time: '',
      tool: 'send_message',
      text: 'hello',
    ));
    expect(splitActivityLine('./run --all'), (
      time: '',
      tool: '',
      text: './run --all',
    ));
  });
}
