import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/settings/app_settings.dart';
import 'package:phone/widgets/floating_sheet.dart';
import 'package:phone/features/actions/actions_models.dart';

import '../support/actions/fixture_actions_source.dart';

import 'package:phone/features/actions/question_screen.dart';
import 'package:phone/features/actions/session_screen.dart';
import 'package:phone/features/actions/start_screen.dart';
import 'package:phone/data/models.dart';

import 'support.dart';

String richText(WidgetTester tester) => tester
    .widgetList<RichText>(find.byType(RichText))
    .map((w) => w.text.toPlainText())
    .join('\n');

/// Scrolls the page until [f] is built and on screen.
Future<void> reveal(WidgetTester tester, Finder f) async {
  await tester.scrollUntilVisible(
    f,
    300,
    scrollable: find.byType(Scrollable).first,
  );
  await tester.ensureVisible(f);
  await tester.pump();
}

void main() {
  group('question screen', () {
    for (final lang in langs) {
      for (final b in brightnesses) {
        testWidgets('options and Other ($lang, ${b.name})', (tester) async {
          await pumpScreen(
            tester,
            const QuestionScreen(id: 'q-one'),
            lang: lang,
            brightness: b,
          );
          expect(
            find.text('Round half up, or round half to even?'),
            findsOneWidget,
          );
          expect(find.text('Half up'), findsOneWidget);
          expect(find.text(tr(lang, 'qs.other')), findsOneWidget);
          expect(find.text(tr(lang, 'qs.pick')), findsOneWidget);
          expect(off(tester, tr(lang, 'qs.send')), isTrue);
        });
      }
    }

    testWidgets('choosing an option sends that option exactly', (tester) async {
      final src = await pumpScreen(tester, const QuestionScreen(id: 'q-one'));
      await tester.tap(find.text('Half to even'));
      await tester.pump();
      await tester.tap(button(tr('en', 'qs.send')));
      await settle(tester);
      expect(src.sent, hasLength(1));
      expect(src.sent.single.kind, CommandKind.answer);
      expect(src.sent.single.text, 'Half to even');
      expect(src.sent.single.shown, 'h-q-one');
      expect(find.text(tr('en', 'res.ok.answer.t')), findsOneWidget);
    });

    testWidgets('Other with empty text cannot be sent', (tester) async {
      await pumpScreen(tester, const QuestionScreen(id: 'q-one'));
      await tester.tap(find.text(tr('en', 'qs.other')));
      await tester.pump();
      expect(off(tester, tr('en', 'qs.send')), isTrue);
      await tester.enterText(find.byType(TextField), '   ');
      await tester.pump();
      expect(off(tester, tr('en', 'qs.send')), isTrue);
    });

    testWidgets('Vietnamese free text is sent unchanged', (tester) async {
      final src = await pumpScreen(tester, const QuestionScreen(id: 'q-one'));
      await tester.tap(find.text(tr('en', 'qs.other')));
      await tester.pump();
      const text = 'Làm tròn xuống, giữ nguyên dấu: ắ ằ ẳ ẵ ặ, ế, ộ, ữ.';
      await tester.enterText(find.byType(TextField), text);
      await tester.pump();
      await tester.tap(button(tr('en', 'qs.send')));
      await settle(tester);
      expect(src.sent.single.text, text);
    });

    testWidgets('free text is limited to 2 000 bytes, not characters', (
      tester,
    ) async {
      await pumpScreen(tester, const QuestionScreen(id: 'q-one'));
      await tester.tap(find.text(tr('en', 'qs.other')));
      await tester.pump();
      // 667 three-byte characters are 2 001 bytes.
      await tester.enterText(find.byType(TextField), 'ế' * 667);
      await tester.pump();
      expect(
        find.text(tr('en', 'qs.toolong', {'max': 2000}), skipOffstage: false),
        findsOneWidget,
      );
      expect(off(tester, tr('en', 'qs.send')), isTrue);
      await tester.enterText(find.byType(TextField), 'ế' * 666);
      await tester.pump();
      expect(off(tester, tr('en', 'qs.send')), isFalse);
      expect(
        find.text(
          tr('en', 'qs.bytes', {'n': 1998, 'max': 2000}),
          skipOffstage: false,
        ),
        findsOneWidget,
      );
    });

    testWidgets('multi-select sends every picked option', (tester) async {
      final src = await pumpScreen(tester, const QuestionScreen(id: 'q-multi'));
      expect(find.text(tr('en', 'qs.multi')), findsOneWidget);
      await tester.tap(find.text('Euro'));
      await tester.tap(find.text('Dinar'));
      await tester.pump();
      await tester.tap(button(tr('en', 'qs.send')));
      await settle(tester);
      expect(src.sent.single.text, 'Euro, Dinar');
    });

    testWidgets('many options scroll and Send stays reachable', (tester) async {
      await pumpScreen(tester, const QuestionScreen(id: 'q-many'));
      await tester.tap(find.text('Suite 2'));
      await tester.pump();
      expect(find.text('Suite 9'), findsNothing);
      await tester.scrollUntilVisible(
        find.text(tr('en', 'qs.other')),
        200,
        scrollable: find.byType(Scrollable).first,
      );
      expect(button(tr('en', 'qs.send')), findsOneWidget);
      expect(off(tester, tr('en', 'qs.send')), isFalse);
    });
  });

  group('session screen', () {
    for (final lang in langs) {
      for (final b in brightnesses) {
        testWidgets('last lines, read only, no Stop ($lang, ${b.name})', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            const SessionScreen(id: 'ses-working'),
            lang: lang,
            brightness: b,
          );
          expect(find.text(tr(lang, 'sv.act')), findsOneWidget);
          // Three of the five lines, the newest, and a way to the rest.
          expect(richText(tester), contains('src/checkout/total.ts'));
          expect(richText(tester), isNot(contains('applyCoupon')));
          expect(find.text(tr(lang, 'sv.all')), findsOneWidget);
          expect(button(tr(lang, 'sv.stop')), findsNothing);
          expect(find.byType(TextField), findsNothing);
        });

        testWidgets('Stop when home allows it ($lang, ${b.name})', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            const SessionScreen(id: 'ses-stoppable'),
            lang: lang,
            brightness: b,
          );
          expect(button(tr(lang, 'sv.stop')), findsOneWidget);
          expect(button(tr(lang, 'sv.open')), findsOneWidget);
        });
      }
    }

    testWidgets('without the right to see lines the area is absent', (
      tester,
    ) async {
      await pumpScreen(tester, const SessionScreen(id: 'ses-no-lines'));
      expect(find.text(tr('en', 'sv.act')), findsNothing);
      expect(find.text(tr('en', 'sv.lines.empty')), findsNothing);
    });

    testWidgets('Stop: confirm, then one stop command', (tester) async {
      final src = await pumpScreen(
        tester,
        const SessionScreen(id: 'ses-stoppable'),
      );
      await tester.tap(button(tr('en', 'sv.stop')));
      await settle(tester);
      expect(src.sent, hasLength(1));
      expect(src.sent.single.kind, CommandKind.stop);
      expect(src.sent.single.target, 'ses-stoppable');
      expect(find.text(tr('en', 'res.ok.stop.t')), findsOneWidget);
    });

    testWidgets('stopping a session that ended says it was too late', (
      tester,
    ) async {
      await pumpScreen(
        tester,
        const SessionScreen(id: 'ses-stoppable'),
        source: FixtureActionsSource(
          forced: const CommandOutcome(OutcomeKind.sessionEnded),
        ),
      );
      await tester.tap(button(tr('en', 'sv.stop')));
      await settle(tester);
      expect(find.text(tr('en', 'res.ended.t')), findsOneWidget);
    });

    testWidgets('Stop is off while sending is off', (tester) async {
      await pumpScreen(
        tester,
        const SessionScreen(id: 'ses-stoppable'),
        conn: ConnState.noNetwork,
      );
      expect(off(tester, tr('en', 'sv.stop')), isTrue);
    });

    testWidgets('Stop is absent for a session home does not allow it for', (
      tester,
    ) async {
      await pumpScreen(tester, const SessionScreen(id: 'ses-released'));
      expect(button(tr('en', 'sv.stop')), findsNothing);
    });
  });

  group('start screen', () {
    Future<void> toPreview(
      WidgetTester tester,
      String issue, {
      bool quick = false,
      String lang = 'en',
      Brightness b = Brightness.light,
    }) async {
      await pumpScreen(
        tester,
        const Scaffold(body: SafeArea(child: StartScreen())),
        lang: lang,
        brightness: b,
      );
      await tester.tap(find.textContaining(issue));
      await tester.pump();
      if (quick) {
        await reveal(tester, find.text(tr(lang, 'sp.quick')));
        await tester.tap(find.text(tr(lang, 'sp.quick')));
        await tester.pump();
      }
      await tester.tap(button(tr(lang, quick ? 'sp.go.quick' : 'sp.go.full')));
      await tester.pumpAndSettle();
    }

    for (final lang in langs) {
      for (final b in brightnesses) {
        testWidgets('pick, preview, whole prompt ($lang, ${b.name})', (
          tester,
        ) async {
          await toPreview(tester, 'cart total', lang: lang, b: b);
          await reveal(tester, find.text(tr(lang, 'sp.prompt.all')));
          expect(find.text(tr(lang, 'sp.prompt')), findsOneWidget);
          // Start is locked until the long prompt is opened.
          expect(off(tester, tr(lang, 'sp.go.full')), isTrue);
          await tester.tap(find.text(tr(lang, 'sp.prompt.all')));
          await tester.pump();
          await reveal(
            tester,
            find.textContaining('make them agree.', findRichText: true),
          );
          expect(richText(tester), contains('make them agree.'));
          expect(off(tester, tr(lang, 'sp.go.full')), isFalse);
        });
      }
    }

    testWidgets('the first issue is picked at the start, so the button is on', (
      tester,
    ) async {
      await pumpScreen(
        tester,
        const Scaffold(body: SafeArea(child: StartScreen())),
      );
      expect(off(tester, tr('en', 'sp.go.full')), isFalse);
      expect(find.text(tr('en', 'sp.go.full')), findsOneWidget);
    });

    Finder inSheet(String text) => find.descendant(
      of: find.byType(FloatingSheet),
      matching: find.text(text),
    );

    Future<FixtureActionsSource> pumpList(
      WidgetTester tester,
      StartList list, {
      String lang = 'en',
    }) => pumpScreen(
      tester,
      const Scaffold(body: SafeArea(child: StartScreen())),
      lang: lang,
      source: _Listed(list),
    );

    for (final lang in langs) {
      testWidgets(
        'nothing ticked, a failed read, a first read, no right: only the reason, no mode, no steps, no button ($lang)',
        (tester) async {
          final states = <(StartList, String)>[
            (StartList.empty, tr(lang, 'rp.none.h', {'c': fixtureComputer})),
            (
              const StartList([], error: StartListError.read),
              tr(lang, 'rp.err.h', {'c': fixtureComputer}),
            ),
            (
              const StartList([], error: StartListError.reading),
              tr(lang, 'rp.reading.h', {'c': fixtureComputer}),
            ),
            (
              const StartList([], error: StartListError.noRight),
              tr(lang, 'rp.right.h'),
            ),
          ];
          for (final (list, title) in states) {
            await pumpList(tester, list, lang: lang);
            expect(find.text(title), findsOneWidget, reason: title);
            expect(find.text(tr(lang, 'rp.sec')), findsNothing);
            expect(find.text(tr(lang, 'sp.how')), findsNothing);
            expect(find.text(tr(lang, 'sp.go.full')), findsNothing);
          }
        },
      );
    }

    testWidgets('a failed read hides a list it still holds', (tester) async {
      await pumpList(
        tester,
        const StartList([
          StartRepo(
            'shop',
            'github.com',
            issues: [StartIssue('a#1', 1, 'Fix it', 'shop')],
          ),
        ], error: StartListError.read),
      );
      expect(find.text('Fix it'), findsNothing);
      expect(
        find.text(tr('en', 'rp.err.h', {'c': fixtureComputer})),
        findsOneWidget,
      );
    });

    testWidgets(
      'All repositories groups the issues under their repository; the sheet filters and the choice is remembered',
      (tester) async {
        await pumpScreen(
          tester,
          const Scaffold(body: SafeArea(child: StartScreen())),
        );
        String tr_(String k, [Map<String, Object> v = const {}]) =>
            tr('en', k, v);
        expect(
          find.text('${tr_('rp.all')} · ${tr_('rp.n', {'n': 4})}'),
          findsOneWidget,
        );
        // a header for each repository that has issues, none for the one with no issue
        for (final r in ['shop', 'blog', 'wiki']) {
          expect(find.text(r), findsOneWidget, reason: r);
        }
        expect(find.text('infra'), findsNothing);
        await tester.tap(
          find.text('${tr_('rp.all')} · ${tr_('rp.n', {'n': 4})}'),
        );
        await tester.pumpAndSettle();
        // All first, then each repository with its host and open issues; the one with no clone says so
        expect(find.text(tr_('rp.pick')), findsOneWidget);
        expect(find.text(tr_('rp.open', {'n': 4})), findsOneWidget);
        expect(
          find.text('github.com · ${tr_('rp.open', {'n': 2})}'),
          findsOneWidget,
        );
        expect(
          find.text('gitlab.com · ${tr_('rp.open0')}'),
          findsOneWidget,
          reason: 'infra',
        );
        expect(find.textContaining(tr_('rp.noclone')), findsOneWidget);
        expect(
          find.text(tr_('rp.note', {'c': fixtureComputer})),
          findsOneWidget,
        );
        await tester.tap(inSheet('blog'));
        await tester.pumpAndSettle();
        // one repository: just its issues, no header, and its host under the row
        expect(find.text('Speed up the search page'), findsOneWidget);
        expect(find.text('Fix the cart total rounding'), findsNothing);
        expect(find.text('github.com'), findsWidgets);
        expect(find.text('${tr_('rp.all')} · ${tr_('rp.n1')}'), findsNothing);
        expect(find.text('blog · ${tr_('rp.n1')}'), findsOneWidget);
        final container = ProviderScope.containerOf(
          tester.element(find.byType(StartScreen)),
        );
        expect(
          container.read(settingsProvider).value?.startRepo,
          'github.com/blog',
        );
        // "Fix it in" names the repository, and says it is chosen on the computer
        await reveal(tester, find.text(tr_('rp.fix')));
        expect(
          find.text(tr_('rp.fix.n', {'c': fixtureComputer})),
          findsOneWidget,
        );
      },
    );

    testWidgets('a repository with no open issue says so and offers no start', (
      tester,
    ) async {
      await pumpScreen(
        tester,
        const Scaffold(body: SafeArea(child: StartScreen())),
      );
      await tester.tap(find.textContaining(tr('en', 'rp.all')).first);
      await tester.pumpAndSettle();
      await tester.tap(inSheet('infra'));
      await tester.pumpAndSettle();
      expect(find.text(tr('en', 'rp.empty.h', {'r': 'infra'})), findsOneWidget);
      expect(find.text(tr('en', 'sp.go.full')), findsNothing);
    });

    testWidgets(
      'an issue in a repository with no clone, or one that has a session, is shown but cannot start',
      (tester) async {
        await pumpList(
          tester,
          const StartList([
            StartRepo(
              'shop',
              'github.com',
              issues: [
                StartIssue(
                  'shop#1',
                  1,
                  'Has a session',
                  'shop',
                  hasSession: true,
                ),
                StartIssue('shop#2', 2, 'Free to start', 'shop'),
              ],
            ),
            StartRepo(
              'wiki',
              'gitlab.com',
              cloned: false,
              issues: [StartIssue('wiki#9', 9, 'Move the docs', 'wiki')],
            ),
          ]),
        );
        // the first is picked: it has a session
        expect(off(tester, tr('en', 'sp.go.full')), isTrue);
        expect(
          find.text(
            tr('en', 'rp.session.h', {
              'r': 'shop',
              'n': 1,
              'c': fixtureComputer,
            }),
          ),
          findsOneWidget,
        );
        expect(find.textContaining(tr('en', 'rp.session')), findsWidgets);
        await tester.tap(find.text('Free to start'));
        await tester.pump();
        expect(off(tester, tr('en', 'sp.go.full')), isFalse);
        await tester.tap(find.text('Move the docs'));
        await tester.pump();
        expect(off(tester, tr('en', 'sp.go.full')), isTrue);
        expect(find.text(tr('en', 'sw.miss', {'p': 'wiki'})), findsOneWidget);
        expect(
          find.textContaining('· ${tr('en', 'rp.nofolder')}'),
          findsOneWidget,
        );
      },
    );

    testWidgets('the Full steps name no branch the phone cannot know', (
      tester,
    ) async {
      await pumpScreen(
        tester,
        const Scaffold(body: SafeArea(child: StartScreen())),
      );
      await reveal(
        tester,
        find.textContaining('Makes a worktree and a branch for the issue on'),
      );
      expect(
        find.textContaining('Makes a worktree and a branch for the issue on'),
        findsOneWidget,
      );
      expect(find.textContaining('and the branch'), findsNothing);
    });

    testWidgets(
      'the button says Start Full or Start Quick and follows the choice',
      (tester) async {
        await pumpScreen(
          tester,
          const Scaffold(body: SafeArea(child: StartScreen())),
        );
        await reveal(tester, find.text(tr('en', 'sp.quick')));
        await tester.tap(find.text(tr('en', 'sp.quick')));
        await tester.pump();
        expect(find.text(tr('en', 'sp.go.quick')), findsOneWidget);
        expect(find.text(tr('en', 'sp.go.full')), findsNothing);
        expect(find.text(tr('en', 'sp.q1', {'d': 'shop'})), findsOneWidget);
      },
    );

    testWidgets(
      'Full: the biometric step comes after the prompt, then one start',
      (tester) async {
        final src = await pumpScreen(
          tester,
          const Scaffold(body: SafeArea(child: StartScreen())),
        );
        await tester.tap(find.textContaining('cart total'));
        await tester.pump();
        await tester.tap(button(tr('en', 'sp.go.full')));
        await tester.pumpAndSettle();
        expect(src.sent, isEmpty);
        await reveal(tester, find.text(tr('en', 'sp.prompt.all')));
        await tester.tap(find.text(tr('en', 'sp.prompt.all')));
        await tester.pump();
        await tester.tap(button(tr('en', 'sp.go.full')));
        await settle(tester);
        expect(src.sent, hasLength(1));
        expect(src.sent.single.kind, CommandKind.start);
        expect(src.sent.single.mode, StartMode.full);
        expect(src.sent.single.shown, 'h-start-i-cart');
        expect(find.text(tr('en', 'res.ok.start.t')), findsOneWidget);
      },
    );

    testWidgets('Quick starts the right kind of session', (tester) async {
      final src = await pumpScreen(
        tester,
        const Scaffold(body: SafeArea(child: StartScreen())),
      );
      await tester.tap(find.textContaining('sign-up email'));
      await reveal(tester, find.text(tr('en', 'sp.quick')));
      await tester.tap(find.text(tr('en', 'sp.quick')));
      await tester.pump();
      await tester.tap(button(tr('en', 'sp.go.quick')));
      await tester.pumpAndSettle();
      await reveal(tester, find.text(tr('en', 'sp.prompt.all')));
      await tester.tap(find.text(tr('en', 'sp.prompt.all')));
      await tester.pump();
      await tester.tap(button(tr('en', 'sp.go.quick')));
      await settle(tester);
      expect(src.sent.single.mode, StartMode.quick);
      expect(find.text(tr('en', 'res.ok.start-quick.t')), findsOneWidget);
    });

    testWidgets('a missing folder is shown before sending and blocks Start', (
      tester,
    ) async {
      final src = await pumpScreen(
        tester,
        const StartPreviewScreen(issueId: 'i-docs', mode: StartMode.full),
      );
      await tester.pumpAndSettle();
      expect(find.text(tr('en', 'sw.miss', {'p': 'wiki'})), findsOneWidget);
      expect(find.text(tr('en', 'sp.blocked')), findsOneWidget);
      expect(off(tester, tr('en', 'sp.go.full')), isTrue);
      expect(src.sent, isEmpty);
    });

    testWidgets('low disk blocks Full, and "Use Quick instead" switches', (
      tester,
    ) async {
      await pumpScreen(
        tester,
        const StartPreviewScreen(issueId: 'i-search', mode: StartMode.full),
      );
      await tester.pumpAndSettle();
      expect(
        find.text(tr('en', 'sw.low', {'c': fixtureComputer})),
        findsOneWidget,
      );
      expect(off(tester, tr('en', 'sp.go.full')), isTrue);
      await tester.tap(find.text(tr('en', 'sw.switch')));
      await tester.pumpAndSettle();
      expect(
        find.text(tr('en', 'sw.low', {'c': fixtureComputer})),
        findsNothing,
      );
      expect(button(tr('en', 'sp.go.quick')), findsOneWidget);
    });

    testWidgets('Start is off while sending is off', (tester) async {
      await pumpScreen(
        tester,
        const StartPreviewScreen(issueId: 'i-mail', mode: StartMode.quick),
        conn: ConnState.relayDown,
      );
      await tester.pumpAndSettle();
      expect(off(tester, tr('en', 'sp.go.quick')), isTrue);
    });
  });
}

class _Listed extends FixtureActionsSource {
  _Listed(this.list);
  final StartList list;
  @override
  Future<StartList> issues() async => list;
}
