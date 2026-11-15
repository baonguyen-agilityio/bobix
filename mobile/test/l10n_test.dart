import 'package:flutter_test/flutter_test.dart';
import 'package:phone/l10n/l10n.dart';

void main() {
  test('placeholders are filled in one pass: a value that holds {name} stays as it is', () {
    // `phone.bn.nonet.d` has {tm}; a computer called "{tm}" must not be read as the time.
    expect(
      translate('en', 'phone.bn.asleep.d', {'tm': '21:02'}),
      contains('21:02'),
    );
    expect(
      translate('en', 'phone.bn.asleep', {'c': '{tm}', 'tm': '21:02'}),
      '{tm} is asleep',
    );
    expect(
      translate('en', 'phone.bn.asleep', {'c': r'$1 \1'}),
      r'$1 \1 is asleep',
      reason: 'regex characters in a value are plain text',
    );
  });

  test('a placeholder without a value stays visible, a missing key is the key, vi falls back to en', () {
    expect(translate('en', 'phone.bn.asleep'), '{c} is asleep');
    expect(translate('en', 'no.such.key'), 'no.such.key');
    expect(translate('vi', 'phone.bn.asleep', {'c': 'Mac'}), 'Mac đang ngủ');
    expect(translate('fr', 'phone.bn.asleep', {'c': 'Mac'}), 'Mac is asleep');
  });
}
