import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/actions_models.dart';

import '../support/actions/fixture_actions_source.dart';

import 'package:phone/features/actions/hidden_chars.dart';
import 'package:phone/features/actions/request_screen.dart';
import 'package:phone/data/models.dart';

import 'support.dart';

bool drawsRaw(WidgetTester tester, String char) => tester
    .widgetList<RichText>(find.byType(RichText))
    .any((w) => w.text.toPlainText().contains(char));

void main() {
  group('hidden characters', () {
    test('control and format characters become visible escapes', () {
      final parts = showEscapes('a\u202Eb\u200B\u001B[2K\n\tc');
      expect(parts.where((p) => p.escaped).map((p) => p.text), [
        '\\u202E',
        '\\u200B',
        '\\u001B',
      ]);
      expect(parts.map((p) => p.text).join(), contains('b'));
      expect(hasHiddenChars('plain text\nwith a tab\t'), isFalse);
      expect(hasHiddenChars('x\u2066y'), isTrue);
      for (final ok in ['\u26A0\uFE0F careful', 'done \u2705\uFE0F', 'a\uFE0F']) {
        expect(hasHiddenChars(ok), isFalse, reason: ok);
      }
      for (final bad in ['\uFE0F', 'x \uFE0F', '\u26A0\uFE0F\uFE0F', 'a\u3164b', 'a\uFE00', 'a\u{E0100}']) {
        expect(hasHiddenChars(bad), isTrue, reason: bad);
      }
    });

    test('line separators, variation selectors and fillers are escaped', () {
      final cases = {
        '\u2028': '\\u2028',
        '\u2029': '\\u2029',
        '\uFE00': '\\uFE00',
        '\uFE0F': '\\uFE0F',
        '\u{E0100}': '\\u{E0100}',
        '\u{E01EF}': '\\u{E01EF}',
        '\u034F': '\\u034F',
        '\u115F': '\\u115F',
        '\u1160': '\\u1160',
        '\u3164': '\\u3164',
        '\uFFA0': '\\uFFA0',
      };
      cases.forEach((char, escape) {
        expect(hasHiddenChars('a${char}b'), isTrue, reason: escape);
        final parts = showEscapes('a${char}b');
        expect(parts.where((p) => p.escaped).map((p) => p.text), [
          escape,
        ], reason: escape);
        expect(parts.where((p) => !p.escaped).map((p) => p.text).join(), 'ab');
      });
      // Their neighbours are ordinary text.
      expect(hasHiddenChars('\u2027'), isFalse);
      expect(hasHiddenChars('\uFDFF\uFE10\u{E00FF}\u{E01F0}'), isFalse);
    });
  });

  for (final lang in langs) {
    for (final b in brightnesses) {
      group('request screen ($lang, ${b.name})', () {
        testWidgets(
          'Bash: the whole input, folder, countdown, Allow and Deny',
          (tester) async {
            await pumpScreen(
              tester,
              const RequestScreen(id: 'req-bash'),
              lang: lang,
              brightness: b,
            );
            expect(find.text('npm test -- checkout'), findsOneWidget);
            expect(find.textContaining('~/code/shop'), findsOneWidget);
            expect(find.text(tr(lang, 'rq.exp', {'n': 9})), findsOneWidget);
            expect(off(tester, tr(lang, 'ask.allow')), isFalse);
            expect(off(tester, tr(lang, 'ask.deny')), isFalse);
            expect(find.text(tr(lang, 'rq.never')), findsOneWidget);
          },
        );

        testWidgets('a destructive request has the band and puts Deny first', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            const RequestScreen(id: 'req-destructive'),
            lang: lang,
            brightness: b,
          );
          expect(find.text(tr(lang, 'rq.destr')), findsOneWidget);
          final deny = tester.getTopLeft(button(tr(lang, 'ask.deny')).first);
          final allow = tester.getTopLeft(button(tr(lang, 'rq.allow.d')).first);
          expect(deny.dx, lessThan(allow.dx));
        });

        testWidgets('hidden characters are shown as escapes, never drawn', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            const RequestScreen(id: 'req-hidden'),
            lang: lang,
            brightness: b,
          );
          expect(find.text(tr(lang, 'rq.hid')), findsOneWidget);
          expect(drawsRaw(tester, '\u202E'), isFalse);
          expect(drawsRaw(tester, '\u200B'), isFalse);
          expect(drawsRaw(tester, '\u001B'), isFalse);
          expect(
            find.textContaining('\\u202E', findRichText: true),
            findsOneWidget,
          );
        });

        testWidgets('a cut input shows the banner and offers Deny only', (
          tester,
        ) async {
          await pumpScreen(
            tester,
            const RequestScreen(id: 'req-cut'),
            lang: lang,
            brightness: b,
          );
          expect(find.text(tr(lang, 'rq.cut')), findsOneWidget);
          expect(button(tr(lang, 'ask.allow')), findsNothing);
          expect(button(tr(lang, 'rq.allow.d')), findsNothing);
          expect(off(tester, tr(lang, 'ask.deny')), isFalse);
        });

        testWidgets(
          'a request being handled at the computer is still answerable',
          (tester) async {
            await pumpScreen(
              tester,
              const RequestScreen(id: 'req-released'),
              lang: lang,
              brightness: b,
            );
            expect(find.text(tr(lang, 'rq.rel')), findsOneWidget);
            expect(off(tester, tr(lang, 'ask.allow')), isFalse);
          },
        );

        for (final c in [
          ('req-closed-pc', 'cl.pc'),
          ('req-closed-phone', 'cl.ph'),
          ('req-closed-expired', 'cl.ex'),
        ]) {
          testWidgets('${c.$1} is shown as closed', (tester) async {
            await pumpScreen(
              tester,
              RequestScreen(id: c.$1),
              lang: lang,
              brightness: b,
            );
            expect(find.text(tr(lang, c.$2)), findsOneWidget);
            expect(button(tr(lang, 'ask.allow')), findsNothing);
          });
        }
      });
    }
  }

  group('request screen behaviour', () {
    testWidgets(
      'a long input is collapsed and Allow stays locked until opened',
      (tester) async {
        await pumpScreen(tester, const RequestScreen(id: 'req-long'));
        expect(off(tester, tr('en', 'ask.allow')), isTrue);
        expect(find.text(tr('en', 'rq.openfirst')), findsOneWidget);
        await tester.tap(find.textContaining('Show the whole command'));
        await tester.pump();
        expect(off(tester, tr('en', 'ask.allow')), isFalse);
        expect(find.textContaining('git commit'), findsOneWidget);
      },
    );

    testWidgets(
      'no countdown when the computer waits for as long as it takes',
      (tester) async {
        await pumpScreen(tester, const RequestScreen(id: 'req-nowait'));
        expect(find.textContaining('Expires in'), findsNothing);
      },
    );

    testWidgets('the countdown counts from seconds left', (tester) async {
      await pumpScreen(tester, const RequestScreen(id: 'req-expiring'));
      expect(find.text('Expires in 45 s'), findsOneWidget);
      await tester.pump(const Duration(seconds: 5));
      expect(find.text('Expires in 40 s'), findsOneWidget);
      await tester.pump(const Duration(seconds: 40));
      expect(find.text('Expired'), findsOneWidget);
      expect(off(tester, 'Allow'), isTrue);
    });

    testWidgets('an unknown id shows loading, then the request', (
      tester,
    ) async {
      await pumpScreen(
        tester,
        const RequestScreen(id: 'req-unknown'),
        source: FixtureActionsSource(
          loadingFor: const Duration(milliseconds: 300),
        ),
      );
      expect(find.text(tr('en', 'rq.loading')), findsOneWidget);
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.text('npm test -- checkout'), findsOneWidget);
      expect(find.text(tr('en', 'rq.loading')), findsNothing);
    });

    testWidgets('an id that never resolves is not shown as closed', (
      tester,
    ) async {
      await pumpScreen(tester, const RequestScreen(id: 'req-loading'));
      await tester.pump(const Duration(seconds: 5));
      expect(find.text(tr('en', 'rq.loading')), findsOneWidget);
      expect(find.text(tr('en', 'cl.pc')), findsNothing);
    });

    testWidgets(
      'Deny: confirm, send once with what was shown, then the result',
      (tester) async {
        final src = await pumpScreen(
          tester,
          const RequestScreen(id: 'req-bash'),
        );
        await tester.tap(button('Deny'));
        await tester.pump();
        await tester.pump();
        expect(find.text(tr('en', 'un.h')), findsOneWidget);
        expect(
          find.text(
            tr('en', 'un.deny', {'tool': 'Bash', 'c': fixtureComputer}),
          ),
          findsOneWidget,
        );
        await settle(tester);
        expect(src.sent, hasLength(1));
        expect(src.sent.single.kind, CommandKind.deny);
        expect(src.sent.single.shown, 'h-bash');
        expect(src.sent.single.target, 'req-bash');
        expect(find.text(tr('en', 'res.ok.deny.t')), findsOneWidget);
      },
    );

    testWidgets('a cancelled confirm sends nothing and leaves the screen', (
      tester,
    ) async {
      final src = await pumpScreen(
        tester,
        const RequestScreen(id: 'req-bash'),
        approve: false,
      );
      await tester.tap(button('Allow'));
      await settle(tester);
      expect(src.sent, isEmpty);
      expect(find.text(tr('en', 'un.cancelled')), findsOneWidget);
      expect(find.text('npm test -- checkout'), findsOneWidget);
    });

    testWidgets('a command id is fresh for every command', (tester) async {
      final src = FixtureActionsSource();
      expect({for (var i = 0; i < 50; i++) src.newCommandId()}.length, 50);
    });

    for (final conn in [ConnState.noNetwork, ConnState.relayDown]) {
      testWidgets('sending is off with ${conn.name}', (tester) async {
        final src = await pumpScreen(
          tester,
          const RequestScreen(id: 'req-bash'),
          conn: conn,
        );
        expect(off(tester, 'Allow'), isTrue);
        expect(off(tester, 'Deny'), isTrue);
        expect(
          find.text(
            tr('en', conn == ConnState.noNetwork ? 'sn.nonet' : 'sn.relay'),
          ),
          findsOneWidget,
        );
        expect(src.sent, isEmpty);
      });
    }

    for (final id in ['req-write', 'req-edit', 'req-mcp']) {
      testWidgets('$id shows its whole input', (tester) async {
        await pumpScreen(tester, RequestScreen(id: id));
        final text = tester
            .widgetList<RichText>(find.byType(RichText))
            .map((w) => w.text.toPlainText())
            .join('\n');
        expect(text, switch (id) {
          'req-write' => contains('Math.round(n * 100) / 100'),
          'req-edit' => contains('roundCents(price * qty)'),
          _ => contains('"issue": 12'),
        });
      });
    }
  });
}
