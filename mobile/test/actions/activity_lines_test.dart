import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/activity_lines.dart';

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
}
