import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

const _window = MethodChannel('dev.bobix.phone/window');

/// While one or more [SecureScope]s are on screen the window is marked secure (Android's `FLAG_SECURE`): the recents screen
/// and screenshots show nothing of a request, a plan or a question. iOS has no such flag: the cover and the app lock do
/// that job there, and the call is a no-op.
class SecureScope extends StatefulWidget {
  const SecureScope({super.key, required this.child});

  final Widget child;

  @override
  State<SecureScope> createState() => _SecureScopeState();
}

int _open = 0;

Future<void> _apply() async {
  try {
    await _window.invokeMethod<void>('setSecure', {'secure': _open > 0});
  } on MissingPluginException {
    // No such channel on this platform.
  } on PlatformException {
    // The window could not be marked: nothing more to do.
  }
}

class _SecureScopeState extends State<SecureScope> {
  @override
  void initState() {
    super.initState();
    _open++;
    _apply();
  }

  @override
  void dispose() {
    _open--;
    _apply();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
