/// Control and format characters that a screen must never draw: they are shown as visible escapes, so the text on
/// screen is the text that runs. Newline and tab stay as they are. The same list as the crate's `is_odd_char` (`bobix-link`,
/// `messages.rs`), which the computer uses for what it sends: the format characters, the invisible letters (Hangul fillers),
/// the combining grapheme joiner and the variation selectors.
bool _hidden(int c) =>
    (c < 0x20 && c != 0x0A && c != 0x09) ||
    (c >= 0x7F && c <= 0x9F) ||
    c == 0xAD ||
    c == 0x034F ||
    (c >= 0x0600 && c <= 0x0605) ||
    c == 0x061C ||
    c == 0x06DD ||
    c == 0x070F ||
    c == 0x08E2 ||
    c == 0x115F ||
    c == 0x1160 ||
    (c >= 0x180B && c <= 0x180F) ||
    (c >= 0x200B && c <= 0x200F) ||
    (c >= 0x2028 && c <= 0x202E) ||
    (c >= 0x2060 && c <= 0x206F) ||
    c == 0x3164 ||
    (c >= 0xFE00 && c <= 0xFE0D) ||
    c == 0xFEFF ||
    c == 0xFFA0 ||
    (c >= 0xFFF9 && c <= 0xFFFB) ||
    c == 0x110BD ||
    c == 0x110CD ||
    (c >= 0x13430 && c <= 0x1343F) ||
    (c >= 0x1BCA0 && c <= 0x1BCA3) ||
    (c >= 0x1D173 && c <= 0x1D17A) ||
    (c >= 0xE0000 && c <= 0xE007F) ||
    (c >= 0xE0100 && c <= 0xE01EF);

bool _selector(int c) => c == 0xFE0E || c == 0xFE0F;

/// For each rune of [s], whether it is hidden. An emoji presentation selector (U+FE0E, U+FE0F) is allowed right after a visible
/// character (`⚠️`), and hidden first, after white space or a hidden character, or stacked on another one.
List<bool> _flags(List<int> runes) {
  var prevVisible = false;
  return [
    for (final c in runes)
      () {
        final odd = _selector(c) ? !prevVisible : _hidden(c);
        prevVisible = !odd && !_selector(c) && !_space(c);
        return odd;
      }(),
  ];
}

bool _space(int c) => c == 0x20 || (c >= 0x09 && c <= 0x0D) || c == 0xA0 || c == 0x1680 || (c >= 0x2000 && c <= 0x200A) || c == 0x2028 || c == 0x2029 || c == 0x202F || c == 0x205F || c == 0x3000 || c == 0x85;

class TextPart {
  const TextPart(this.text, {this.escaped = false});
  final String text;
  final bool escaped;
}

bool hasHiddenChars(String s) => _flags(s.runes.toList()).contains(true);

String _escape(int c) => c > 0xFFFF
    ? '\\u{${c.toRadixString(16).toUpperCase()}}'
    : '\\u${c.toRadixString(16).toUpperCase().padLeft(4, '0')}';

/// [s] cut into runs of plain text and runs of visible escapes.
List<TextPart> showEscapes(String s) {
  final out = <TextPart>[];
  final plain = StringBuffer();
  void flush() {
    if (plain.isNotEmpty) out.add(TextPart(plain.toString()));
    plain.clear();
  }

  final runes = s.runes.toList();
  final hidden = _flags(runes);
  for (var i = 0; i < runes.length; i++) {
    final c = runes[i];
    if (hidden[i]) {
      flush();
      out.add(TextPart(_escape(c), escaped: true));
    } else {
      plain.writeCharCode(c);
    }
  }
  flush();
  return out;
}
