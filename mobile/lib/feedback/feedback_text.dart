/// Feedback goes the way the desktop's feedback goes: a link that opens GitHub's "new issue" form, filled in. The person
/// reads the text there and submits it themselves, so the app needs no token and no server, and sends nothing itself.
/// Issues are public: only listed, non-identifying facts go in, and everything is redacted first.
const feedbackRepo = 'baonguyen-agilityio/bobix';

/// GitHub answered a 6,500-character link and failed on 6,900: stay under that (the desktop's limit too).
const maxFeedbackUrl = 6000;

final _token = RegExp(r'([?&]t=)[^\s&]+');
final _key = RegExp(
  r'\b(?:sk-[A-Za-z0-9_-]{10,}|[a-f0-9]{32,})\b',
  caseSensitive: false,
);
final _path = RegExp(r'''(?:~|[A-Za-z]:)?(?:[\\/][^\s"'`:,()\\/]+){2,}''');

/// Takes out what could identify someone: the home folder, the folders of a path, pairing tokens, device ids and keys.
/// The same rules as the desktop's.
String redact(String text, {String home = ''}) {
  var out = home.isEmpty ? text : text.replaceAll(home, '~');
  out = out.replaceAllMapped(_token, (m) => '${m[1]}[redacted]');
  out = out.replaceAll(_key, '[redacted]');
  return out.replaceAllMapped(_path, (m) {
    final all = m[0]!;
    return '.../${all.split(RegExp(r'[\\/]')).last}';
  });
}

/// What will be sent: the title and the body as the form will show them, and the link that opens it.
class FeedbackText {
  const FeedbackText({
    required this.title,
    required this.body,
    required this.url,
    required this.truncated,
  });
  final String title, body, url;

  /// The description was shortened so the link fits.
  final bool truncated;
}

/// Builds the text from what the person wrote and [facts] (listed, non-identifying lines; none for an idea). Everything
/// is redacted. A long description is shortened until the link fits.
FeedbackText buildFeedback({
  required bool idea,
  required String title,
  required String description,
  List<String> facts = const [],
  String home = '',
}) {
  var text = redact(description.trim(), home: home);
  final heading =
      '[${idea ? 'Idea' : 'Bug'}] ${_head(redact(title.trim(), home: home), 200)}';
  String body(String t) {
    final shown = t.isEmpty ? '_(not described)_' : t;
    if (idea) return '## The idea\n$shown';
    return [
      '## What happened',
      shown,
      '',
      '## Technical details (added by Bobix)',
      ...facts.map((f) => redact(f, home: home)),
    ].join('\n');
  }

  String url(String t) =>
      'https://github.com/$feedbackRepo/issues/new?title=${Uri.encodeComponent(heading)}&body=${Uri.encodeComponent(body(t))}';

  var truncated = false;
  while (url(text).length > maxFeedbackUrl && text.isNotEmpty) {
    text = _head(text, text.length - 100);
    truncated = true;
  }
  return FeedbackText(
    title: heading,
    body: body(text),
    url: url(text),
    truncated: truncated,
  );
}

String _head(String s, int max) =>
    s.length <= max ? s : s.substring(0, max < 0 ? 0 : max);
