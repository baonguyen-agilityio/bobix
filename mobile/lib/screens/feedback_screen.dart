import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:url_launcher/url_launcher.dart';

import '../config/flavor.dart';
import '../feedback/feedback_text.dart';
import '../l10n/l10n.dart';
import '../link/link_providers.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/glass.dart';
import '../widgets/status_banner.dart';

/// Opens [url] in the browser; false when it could not. A seam: tests give a recorder.
typedef UrlOpener = Future<bool> Function(Uri url);

final urlOpenerProvider = Provider<UrlOpener>(
  (ref) =>
      (url) => launchUrl(url, mode: LaunchMode.externalApplication),
);

/// The listed, non-identifying facts a bug report carries: the app, the system, the flavor, the language, the connection.
/// No name, id, folder or key.
final feedbackFactsProvider = FutureProvider<List<String>>((ref) async {
  final lang = ref.read(langProvider);
  final conn = ref.read(linkStatusProvider).value?.conn.name ?? 'unknown';
  String app = 'unknown';
  try {
    final p = await PackageInfo.fromPlatform();
    app = '${p.version}+${p.buildNumber}';
  } on Object {
    // No version to report.
  }
  return [
    '- Bobix phone $app, ${Platform.operatingSystem} ${Platform.operatingSystemVersion}',
    '- Flavor: ${ref.read(flavorProvider).name}, language: $lang',
    '- Connection: $conn',
  ];
});

/// "Send feedback": the person writes, reads exactly what would be sent (redacted), and only the press of "Open GitHub"
/// leaves the app: GitHub's new-issue form opens with that text and the person submits it there themselves.
class FeedbackScreen extends ConsumerStatefulWidget {
  const FeedbackScreen({super.key});

  @override
  ConsumerState<FeedbackScreen> createState() => _FeedbackScreenState();
}

class _FeedbackScreenState extends ConsumerState<FeedbackScreen> {
  bool _idea = false;
  bool _review = false;
  bool _failed = false;
  final _title = TextEditingController();
  final _desc = TextEditingController();

  @override
  void dispose() {
    _title.dispose();
    _desc.dispose();
    super.dispose();
  }

  FeedbackText _text(List<String> facts) => buildFeedback(
    idea: _idea,
    title: _title.text,
    description: _desc.text,
    facts: _idea ? const [] : facts,
  );

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final facts = ref.watch(feedbackFactsProvider).value ?? const [];
    final text = _text(facts);
    final canReview = _title.text.trim().isNotEmpty;
    return Scaffold(
      backgroundColor: Colors.transparent,
      appBar: AppBar(
        backgroundColor: Colors.transparent,
        elevation: 0,
        foregroundColor: t.ink,
        title: Text(s('phone.fb.title')),
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
          children: _review
              ? _preview(context, s, t, text)
              : _form(context, s, t, canReview),
        ),
      ),
    );
  }

  List<Widget> _form(BuildContext context, Strings s, BobixTokens t, bool ok) =>
      [
        Row(
          children: [
            for (final (idea, key) in const [
              (false, 'phone.fb.bug'),
              (true, 'phone.fb.idea'),
            ]) ...[
              Expanded(
                child: Semantics(
                  inMutuallyExclusiveGroup: true,
                  selected: _idea == idea,
                  button: true,
                  label: s(key),
                  child: ExcludeSemantics(
                    child: InkWell(
                      borderRadius: BorderRadius.circular(16),
                      onTap: () => setState(() => _idea = idea),
                      child: Glass(
                        tint: _idea == idea ? t.accentBg : null,
                        border: _idea == idea ? t.accent : null,
                        padding: const EdgeInsets.symmetric(vertical: 14),
                        child: Center(
                          child: Text(
                            s(key),
                            style: TextStyle(
                              fontWeight: FontWeight.w600,
                              color: _idea == idea ? t.accentInk : t.ink2,
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              if (!idea) const SizedBox(width: 10),
            ],
          ],
        ),
        const SizedBox(height: 16),
        TextField(
          controller: _title,
          maxLength: 120,
          decoration: InputDecoration(
            labelText: s('phone.fb.title.l'),
            border: const OutlineInputBorder(),
          ),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 8),
        TextField(
          controller: _desc,
          minLines: 5,
          maxLines: 10,
          decoration: InputDecoration(
            labelText: s('phone.fb.desc.l'),
            alignLabelWithHint: true,
            border: const OutlineInputBorder(),
          ),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 16),
        PillButton(
          label: s('phone.fb.review'),
          primary: true,
          onPressed: ok ? () => setState(() => _review = true) : null,
        ),
      ];

  List<Widget> _preview(
    BuildContext context,
    Strings s,
    BobixTokens t,
    FeedbackText text,
  ) => [
    Semantics(
      header: true,
      child: Text(
        s('phone.fb.preview.t'),
        style: TextStyle(
          fontSize: 20,
          fontWeight: FontWeight.w700,
          color: t.ink,
        ),
      ),
    ),
    const SizedBox(height: 8),
    Text(s('phone.fb.preview.d'), style: TextStyle(color: t.ink2, height: 1.4)),
    const SizedBox(height: 12),
    Glass(
      padding: const EdgeInsets.all(14),
      child: SelectableText(
        '${text.title}\n\n${text.body}',
        key: const ValueKey('feedback-preview'),
        style: const TextStyle(fontFamily: 'Menlo', fontSize: 13, height: 1.45),
      ),
    ),
    if (text.truncated)
      Padding(
        padding: const EdgeInsets.only(top: 10),
        child: StatusBanner(
          kind: BannerKind.info,
          icon: Icons.content_cut,
          title: s('phone.fb.cut'),
        ),
      ),
    if (_failed)
      Padding(
        padding: const EdgeInsets.only(top: 10),
        child: StatusBanner(
          kind: BannerKind.warn,
          icon: Icons.error_outline,
          title: s('phone.fb.failed'),
        ),
      ),
    const SizedBox(height: 8),
    Text(s('phone.fb.public'), style: TextStyle(fontSize: 12, color: t.ink3)),
    const SizedBox(height: 16),
    PillButton(
      label: s('phone.fb.open'),
      primary: true,
      icon: Icons.open_in_new,
      onPressed: () async {
        final ok = await ref.read(urlOpenerProvider)(Uri.parse(text.url));
        if (mounted) setState(() => _failed = !ok);
      },
    ),
    const SizedBox(height: 10),
    PillButton(
      label: s('phone.fb.back'),
      onPressed: () => setState(() => _review = false),
    ),
  ];
}
