import 'dart:ui' as ui;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'strings.g.dart';

/// `{name}` placeholders are filled from [vars]; a missing key falls back to English, then to the key itself.
String translate(
  String lang,
  String key, [
  Map<String, Object> vars = const {},
]) {
  final s = phoneStrings[lang]?[key] ?? phoneStrings['en']![key] ?? key;
  // One pass over the template: a value that itself holds `{name}` (a computer's name comes from the desktop) is never
  // read as a placeholder. A name without a value stays as written.
  return s.replaceAllMapped(
    RegExp(r'\{(\w+)\}'),
    (m) => vars.containsKey(m[1]) ? '${vars[m[1]]}' : m[0]!,
  );
}

class Lang extends Notifier<String> {
  @override
  String build() =>
      ui.PlatformDispatcher.instance.locale.languageCode == 'vi' ? 'vi' : 'en';

  void set(String lang) => state = lang;
}

final langProvider = NotifierProvider<Lang, String>(Lang.new);

/// `ref.watch(stringsProvider)('phone.home.need')`
class Strings {
  const Strings(this.lang);
  final String lang;
  String call(String key, [Map<String, Object> vars = const {}]) =>
      translate(lang, key, vars);
}

final stringsProvider = Provider<Strings>(
  (ref) => Strings(ref.watch(langProvider)),
);
