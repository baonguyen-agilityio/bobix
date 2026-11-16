import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/activity_lines.dart';

void main() {
  test('an activity line splits into the tool and what it did', () {
    expect(splitActivityLine('Bash ls -la'), (tool: 'Bash', text: 'ls -la'));
    expect(splitActivityLine('SendMessage'), (tool: 'SendMessage', text: ''));
    expect(splitActivityLine('mcp__server__send_message hello'), (
      tool: 'send_message',
      text: 'hello',
    ));
    expect(splitActivityLine('./run --all'), (tool: '', text: './run --all'));
  });
}
