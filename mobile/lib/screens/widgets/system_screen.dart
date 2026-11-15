import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/theme.dart' show minTouch;

/// A screen of the phone's own system (home screen, lock screen, Control Center) as a preview: a wallpaper, a glass
/// "Bobix" pill that goes back to the app, and the [child] below it. The wallpaper is the phone's, so it does not follow the
/// app's light or dark theme.
class SystemScreen extends ConsumerWidget {
  const SystemScreen({
    super.key,
    required this.title,
    required this.child,
    this.colors = const [
      Color(0xFF34406E),
      Color(0xFF6B4A86),
      Color(0xFFD9735F),
    ],
  });

  /// What the screen is, for screen readers.
  final String title;
  final Widget child;
  final List<Color> colors;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: DecoratedBox(
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: colors,
          ),
        ),
        child: SafeArea(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(14, 8, 14, 8),
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: Semantics(
                    button: true,
                    label: s('phone.a.back'),
                    excludeSemantics: true,
                    child: InkWell(
                      borderRadius: BorderRadius.circular(22),
                      onTap: () => Navigator.of(context).maybePop(),
                      child: Container(
                        height: minTouch,
                        padding: const EdgeInsets.fromLTRB(6, 0, 14, 0),
                        decoration: BoxDecoration(
                          color: const Color(0x40FFFFFF),
                          borderRadius: BorderRadius.circular(22),
                          border: Border.all(
                            width: .5,
                            color: const Color(0x4DFFFFFF),
                          ),
                        ),
                        child: const Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Icon(Icons.chevron_left, color: Colors.white),
                            Text(
                              'Bobix',
                              style: TextStyle(
                                fontSize: 15,
                                fontWeight: FontWeight.w600,
                                color: Colors.white,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              Expanded(
                child: Semantics(container: true, label: title, child: child),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
