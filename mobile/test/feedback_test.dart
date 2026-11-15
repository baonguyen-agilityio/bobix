// Feedback: what is redacted, what the person sees before it leaves, and that nothing leaves without the press.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/feedback/feedback_text.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/screens/feedback_screen.dart';
import 'package:phone/theme/theme.dart';

class _Lang extends Lang {
  _Lang(this.value);
  final String value;
  @override
  String build() => value;
}

void main() {
  group('redaction', () {
    test(
      'folders, home, pairing tokens, device ids and keys are taken out',
      () {
        expect(
          redact(
            'failed /Users/Jo Doe/work/acme-secret/a.js',
            home: '/Users/Jo Doe',
          ),
          'failed .../a.js',
        );
        expect(redact('failed /home/sam/acme/a.js'), 'failed .../a.js');
        expect(redact(r'failed C:\Users\sam\acme\a.js'), 'failed .../a.js');
        expect(
          redact('bobix://pair?r=wss%3A%2F%2Fx&t=abcDEF123&z=1'),
          contains('t=[redacted]&z=1'),
        );
        expect(redact('device ${'a1' * 16} here'), 'device [redacted] here');
        expect(redact('key ${'a' * 48}'), 'key [redacted]');
        expect(redact('sk-abcdefghijklmnop'), '[redacted]');
        expect(
          redact('nothing to hide, 12 files'),
          'nothing to hide, 12 files',
        );
      },
    );
  });

  group('the text', () {
    test(
      'a bug carries the description and the listed facts, all redacted',
      () {
        final f = buildFeedback(
          idea: false,
          title: 'It fails at /Users/sam/code/x',
          description: 'Opening /Users/sam/code/x/y.dart crashed with t=sekret',
          facts: const ['- Bobix phone 1.0+1, ios 18', '- Connection: ok'],
        );
        expect(f.title, '[Bug] It fails at .../x');
        expect(f.body, contains('## What happened'));
        expect(f.body, contains('.../y.dart'));
        expect(f.body, contains('## Technical details (added by Bobix)'));
        expect(f.body, contains('- Connection: ok'));
        expect(f.body, isNot(contains('sam')));
        expect(
          f.url,
          startsWith('https://github.com/$feedbackRepo/issues/new?title='),
        );
        expect(
          Uri.parse(f.url).queryParameters['body'],
          f.body,
          reason: 'the link carries exactly the text that was shown',
        );
        expect(f.truncated, isFalse);
      },
    );

    test('an idea carries no technical details', () {
      final f = buildFeedback(
        idea: true,
        title: 'Dark pets',
        description: 'More pets',
        facts: const ['- Connection: ok'],
      );
      expect(f.title, '[Idea] Dark pets');
      expect(f.body, '## The idea\nMore pets');
    });

    test(
      'a long description is shortened until the link fits, and says so',
      () {
        final f = buildFeedback(
          idea: true,
          title: 't',
          description: 'word ' * 4000,
        );
        expect(f.url.length, lessThanOrEqualTo(maxFeedbackUrl));
        expect(f.truncated, isTrue);
      },
    );
  });

  group('the screen', () {
    final opened = <Uri>[];
    var openOk = true;
    setUp(() {
      opened.clear();
      openOk = true;
    });

    Future<void> pump(WidgetTester tester, {String lang = 'en'}) async {
      tester.view.physicalSize = const Size(400, 1600);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        ProviderScope(
          key: UniqueKey(),
          overrides: [
            langProvider.overrideWith(() => _Lang(lang)),
            urlOpenerProvider.overrideWithValue((u) async {
              opened.add(u);
              return openOk;
            }),
            feedbackFactsProvider.overrideWith(
              (ref) async => [
                '- Bobix phone 1.0+1, ios 18',
                '- Connection: ok',
              ],
            ),
          ],
          child: MaterialApp(
            theme: bobixTheme(Brightness.light),
            home: const FeedbackScreen(),
          ),
        ),
      );
      await tester.pump();
      await tester.pump();
    }

    testWidgets(
      'Review needs a title; the preview shows exactly the text; nothing is opened before the press',
      (tester) async {
        await pump(tester);
        expect(find.text('Something went wrong'), findsOneWidget);
        expect(
          tester
              .widget<InkWell>(find.widgetWithText(InkWell, 'Review the text'))
              .onTap,
          isNull,
        );
        await tester.enterText(
          find.byType(TextField).first,
          'It fails at /Users/sam/code/x',
        );
        await tester.enterText(find.byType(TextField).last, 'It crashed');
        await tester.pump();
        await tester.tap(find.text('Review the text'));
        await tester.pump();
        expect(find.text('This is exactly what will be sent'), findsOneWidget);
        final shown = tester
            .widget<SelectableText>(
              find.byKey(const ValueKey('feedback-preview')),
            )
            .data!;
        expect(shown, contains('[Bug] It fails at .../x'));
        expect(shown, contains('It crashed'));
        expect(shown, contains('- Connection: ok'));
        expect(shown, isNot(contains('sam')));
        expect(opened, isEmpty, reason: 'looking at it sends nothing');
      },
    );

    testWidgets(
      'going back and leaving sends nothing; the press opens the link with the text that was shown',
      (tester) async {
        await pump(tester);
        await tester.enterText(find.byType(TextField).first, 'Title');
        await tester.pump();
        await tester.tap(find.text('Review the text'));
        await tester.pump();
        await tester.tap(find.text('Back to edit'));
        await tester.pump();
        expect(opened, isEmpty);
        await tester.tap(find.text('Review the text'));
        await tester.pump();
        final shown = tester
            .widget<SelectableText>(
              find.byKey(const ValueKey('feedback-preview')),
            )
            .data!;
        await tester.tap(find.text('Open GitHub'));
        await tester.pump();
        expect(opened, hasLength(1));
        expect(
          '${opened.single.queryParameters['title']}\n\n${opened.single.queryParameters['body']}',
          shown,
        );
      },
    );

    testWidgets('an idea has no technical facts in its preview', (
      tester,
    ) async {
      await pump(tester);
      await tester.tap(find.text('An idea'));
      await tester.enterText(find.byType(TextField).first, 'Dark pets');
      await tester.pump();
      await tester.tap(find.text('Review the text'));
      await tester.pump();
      final shown = tester
          .widget<SelectableText>(
            find.byKey(const ValueKey('feedback-preview')),
          )
          .data!;
      expect(shown, contains('[Idea] Dark pets'));
      expect(shown, isNot(contains('Connection')));
    });

    testWidgets(
      'a browser that cannot be opened says so, and nothing was sent',
      (tester) async {
        openOk = false;
        await pump(tester);
        await tester.enterText(find.byType(TextField).first, 'Title');
        await tester.pump();
        await tester.tap(find.text('Review the text'));
        await tester.pump();
        await tester.tap(find.text('Open GitHub'));
        await tester.pump();
        await tester.pump();
        expect(
          find.text('The browser could not be opened. Nothing was sent.'),
          findsOneWidget,
        );
      },
    );

    testWidgets('Vietnamese, and the largest font', (tester) async {
      tester.platformDispatcher.textScaleFactorTestValue = 2.0;
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      await pump(tester, lang: 'vi');
      expect(find.text('Có gì đó bị lỗi'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.enterText(find.byType(TextField).first, 'Tiêu đề');
      await tester.pump();
      await tester.tap(find.text('Xem lại nội dung'));
      await tester.pump();
      expect(find.text('Đây đúng là nội dung sẽ được gửi'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  });
}
