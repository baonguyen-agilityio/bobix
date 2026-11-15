/// Control and format characters that a screen must never draw: they are shown as visible escapes, so the text on
/// screen is the text that runs. Newline and tab stay as they are.
bool _hidden(int c) =>
    (c < 0x20 && c != 0x0A && c != 0x09) ||
    (c >= 0x7F && c <= 0x9F) ||
    c == 0xAD ||
    c == 0x061C ||
    c == 0x180E ||
    (c >= 0x200B && c <= 0x200F) ||
    (c >= 0x202A && c <= 0x202E) ||
    (c >= 0x2060 && c <= 0x206F) ||
    c == 0xFEFF ||
    (c >= 0xFFF9 && c <= 0xFFFB) ||
    (c >= 0xE0000 && c <= 0xE007F);

class TextPart {
  const TextPart(this.text, {this.escaped = false});
  final String text;
  final bool escaped;
}

bool hasHiddenChars(String s) => s.runes.any(_hidden);

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

  for (final c in s.runes) {
    if (_hidden(c)) {
      flush();
      out.add(TextPart(_escape(c), escaped: true));
    } else {
      plain.writeCharCode(c);
    }
  }
  flush();
  return out;
}
