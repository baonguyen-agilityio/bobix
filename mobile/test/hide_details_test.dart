import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phone/l10n/l10n.dart';
import 'package:phone/push/hide_details.dart';
import 'package:phone/push/push_providers.dart';
import 'package:phone/theme/theme.dart';

import 'support/fake_push_platform.dart';

void main() {
  testWidgets(
    'Hide details is off by default and a flip is written where the extension reads it',
    (tester) async {
      final platform = FakePushPlatform();
      await tester.pumpWidget(
        ProviderScope(
          overrides: [pushPlatformProvider.overrideWithValue(platform)],
          child: MaterialApp(
            theme: bobixTheme(Brightness.light),
            home: const Scaffold(body: HideDetailsSwitch()),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Hide details on the lock screen'), findsOneWidget);
      expect(tester.widget<Switch>(find.byType(Switch)).value, isFalse);
      await tester.tap(find.byType(Switch));
      await tester.pumpAndSettle();
      expect(platform.hide, isTrue);
      expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
    },
  );

  testWidgets('the switch speaks Vietnamese when the language is Vietnamese', (
    tester,
  ) async {
    final platform = FakePushPlatform()..hide = true;
    final container = ProviderContainer(
      overrides: [pushPlatformProvider.overrideWithValue(platform)],
    );
    addTearDown(container.dispose);
    container.read(langProvider.notifier).set('vi');
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(
          theme: bobixTheme(Brightness.light),
          home: const Scaffold(body: HideDetailsSwitch()),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Ẩn chi tiết trên màn hình khoá'), findsOneWidget);
    expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
  });
}
