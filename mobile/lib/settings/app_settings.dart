import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../link/link_providers.dart';

enum ThemeChoice { light, dark, system }

/// What the person chose in Settings. Nothing secret: kept in an app-private file that is not backed up.
class AppSettings {
  const AppSettings({
    this.theme = ThemeChoice.system,
    this.language,
    this.requireUnlock = true,
  });

  final ThemeChoice theme;

  /// `en` or `vi`; null follows the phone's language.
  final String? language;

  /// "Require Face ID / fingerprint to open": asked when the app comes to the foreground. On by default.
  final bool requireUnlock;

  AppSettings copyWith({
    ThemeChoice? theme,
    String? language,
    bool? requireUnlock,
  }) => AppSettings(
    theme: theme ?? this.theme,
    language: language ?? this.language,
    requireUnlock: requireUnlock ?? this.requireUnlock,
  );

  ThemeMode get themeMode => switch (theme) {
    ThemeChoice.light => ThemeMode.light,
    ThemeChoice.dark => ThemeMode.dark,
    ThemeChoice.system => ThemeMode.system,
  };

  Map<String, Object?> toJson() => {
    'theme': theme.name,
    'language': language,
    'requireUnlock': requireUnlock,
  };

  static AppSettings fromJson(Map<String, dynamic> j) => AppSettings(
    theme: ThemeChoice.values.firstWhere(
      (t) => t.name == j['theme'],
      orElse: () => ThemeChoice.system,
    ),
    language: j['language'] == 'en' || j['language'] == 'vi'
        ? j['language'] as String
        : null,
    requireUnlock: j['requireUnlock'] != false,
  );
}

/// The settings file. A file that is missing or cannot be read gives the defaults (the lock stays on).
class AppSettingsStore {
  AppSettingsStore(Directory dir)
    : _file = File('${dir.path}/app-settings.json');

  final File _file;

  Future<AppSettings> load() async {
    try {
      return AppSettings.fromJson(
        jsonDecode(await _file.readAsString()) as Map<String, dynamic>,
      );
    } on Object {
      return const AppSettings();
    }
  }

  Future<void> save(AppSettings s) async {
    final tmp = File('${_file.path}.tmp');
    await tmp.writeAsString(jsonEncode(s.toJson()), flush: true);
    await tmp.rename(_file.path);
  }
}

class SettingsController extends AsyncNotifier<AppSettings> {
  Future<void> _saving = Future.value();

  @override
  Future<AppSettings> build() async {
    final AppSettings s;
    try {
      s = await AppSettingsStore(
        await ref.read(supportDirectoryProvider.future),
      ).load();
    } on Object {
      return const AppSettings(); // no folder to read (a test): the defaults
    }
    if (s.language != null) ref.read(langProvider.notifier).set(s.language!);
    return s;
  }

  /// A change waits for the saved settings to be read first, so it is made on them and the late read cannot undo it.
  Future<void> _change(AppSettings Function(AppSettings) f) async {
    final loaded = await future;
    final s = f(
      state.value ?? loaded,
    ); // the latest, when another change came in meanwhile
    state = AsyncData(s);
    // One write after another: two at once would share the temporary file.
    _saving = _saving.catchError((Object _) {}).then((_) async {
      await AppSettingsStore(await ref.read(supportDirectoryProvider.future))
          .save(s);
    });
    try {
      await _saving;
    } on Object {
      // The choice holds for this run; it is kept again at the next change.
    }
  }

  Future<void> setTheme(ThemeChoice t) => _change((s) => s.copyWith(theme: t));

  Future<void> setLanguage(String lang) {
    ref.read(langProvider.notifier).set(lang);
    return _change((s) => s.copyWith(language: lang));
  }

  Future<void> setRequireUnlock(bool on) =>
      _change((s) => s.copyWith(requireUnlock: on));
}

final settingsProvider = AsyncNotifierProvider<SettingsController, AppSettings>(
  SettingsController.new,
);
