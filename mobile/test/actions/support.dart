import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/features/actions/actions_source.dart';
import 'package:phone/features/actions/fixture_actions_source.dart';
import 'package:phone/fixtures/fixtures.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/screens/home_screen.dart';
import 'package:phone/theme/theme.dart';

class _Conn extends Conn {
  _Conn(this.value);
  final ConnState value;
  @override
  ConnState build() => value;
}

class _Lang extends Lang {
  _Lang(this.value);
  final String value;
  @override
  String build() => value;
}

/// Every language and brightness the screens must work in.
const langs = ['en', 'vi'];
const brightnesses = [Brightness.light, Brightness.dark];

String tr(String lang, String key, [Map<String, Object> vars = const {}]) =>
    translate(lang, 'phone.$key', vars);

/// Pumps [screen] as the home route, on a phone-sized surface, over [source] and a gate that answers at once.
Future<FixtureActionsSource> pumpScreen(
  WidgetTester tester,
  Widget screen, {
  String lang = 'en',
  Brightness brightness = Brightness.light,
  FixtureActionsSource? source,
  ConnState conn = ConnState.ok,
  bool approve = true,
}) async {
  tester.view
    ..physicalSize = const Size(390 * 3, 844 * 3)
    ..devicePixelRatio = 3;
  addTearDown(tester.view.reset);
  final src = source ?? FixtureActionsSource();
  await tester.pumpWidget(
    ProviderScope(
      key: UniqueKey(),
      overrides: [
        langProvider.overrideWith(() => _Lang(lang)),
        connProvider.overrideWith(() => _Conn(conn)),
        actionsSourceProvider.overrideWithValue(src),
        biometricGateProvider.overrideWithValue(
          FixtureBiometricGate(approve: approve, after: Duration.zero),
        ),
      ],
      child: MaterialApp(theme: bobixTheme(brightness), home: screen),
    ),
  );
  await tester.pump();
  await tester.pump();
  return src;
}

/// The button of the action bar with this label, or null when none is there.
Finder button(String label) => find.widgetWithText(InkWell, label);

/// Whether the button with [label] is switched off.
bool off(WidgetTester tester, String label) =>
    tester.widget<InkWell>(button(label).first).onTap == null;

/// Lets the confirm step and the send finish.
Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 8; i++) {
    await tester.pump(const Duration(milliseconds: 200));
  }
}
