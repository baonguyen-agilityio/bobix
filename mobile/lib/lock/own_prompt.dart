import 'dart:async';

import 'package:flutter/services.dart';

const _channel = MethodChannel('dev.bobix.phone/window');

int _depth = 0;

/// Whether a system prompt that the app asked for itself (the biometric check, the notification dialog) is up. The system
/// makes the app inactive while it shows, so the privacy cover must not be drawn over the screen that asked: the cover
/// stays for every other resign (Control Center, the shade, the app switcher) and for the app going to the background.
bool get ownPromptActive => _depth > 0;

/// Runs [ask] (the call that makes the system show its prompt) with [ownPromptActive] set, in Dart and in the native
/// cover (iOS), and clears it when the prompt ends, however it ends. The native call is not waited for: platform
/// messages arrive in order, so the native flag is set before the prompt's own call reaches the platform.
Future<T> duringOwnPrompt<T>(Future<T> Function() ask) async {
  _depth++;
  unawaited(_tell());
  try {
    return await ask();
  } finally {
    _depth--;
    unawaited(_tell());
  }
}

Future<void> _tell() async {
  try {
    await _channel.invokeMethod<void>('setOwnPrompt', {'active': _depth > 0});
  } on MissingPluginException {
    // No native cover on this platform.
  } on PlatformException {
    // The native cover keeps its state: nothing more to do.
  }
}
