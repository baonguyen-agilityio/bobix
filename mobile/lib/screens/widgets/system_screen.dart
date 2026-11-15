import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../l10n/l10n.dart';
import '../../theme/theme.dart' show minTouch;

/// The clock every system preview shows, so a preview can be compared with the design.
const previewClock = '21:14';

/// The phone's own wallpaper: the same in light and dark, as the design draws it (a 165 degree gradient).
const _wallpaper = LinearGradient(
  begin: Alignment(-.61, -1.05),
  end: Alignment(.61, 1.05),
  colors: [Color(0xFF2F3D6B), Color(0xFF7B4F86), Color(0xFFD9785C)],
  stops: [0, .55, 1],
);

/// A screen of the phone's own system (home screen, lock screen, Control Center) as a preview: a wallpaper, a glass
/// "Bobix" pill that goes back to the app, and the [child] below it. The wallpaper is the phone's, so it does not follow the
/// app's light or dark theme. [overlay] darkens it (the Control Center). The child starts [top] below the status bar and
/// ends [bottom] above the screen's edge (by default the home indicator's zone plus 10).
class SystemScreen extends ConsumerWidget {
  const SystemScreen({
    super.key,
    required this.title,
    required this.child,
    this.overlay,
    this.topOverlay,
    this.top = 42,
    this.bottom,
  });

  /// What the screen is, for screen readers.
  final String title;
  final Widget child;
  final Color? overlay;

  /// Drawn at the very top, over the status bar and centred: the Dynamic Island.
  final Widget? topOverlay;
  final double top;
  final double? bottom;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final below = bottom ?? MediaQuery.viewPaddingOf(context).bottom + 10;
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Stack(
        fit: StackFit.expand,
        children: [
          const DecoratedBox(decoration: BoxDecoration(gradient: _wallpaper)),
          if (overlay != null) ColoredBox(color: overlay!),
          SafeArea(
            bottom: false,
            child: Stack(
              children: [
                Padding(
                  padding: EdgeInsets.only(top: top, bottom: below),
                  child: Semantics(container: true, label: title, child: child),
                ),
                Positioned(
                  top: -5,
                  left: 14,
                  child: Semantics(
                    button: true,
                    label: s('phone.a.back'),
                    excludeSemantics: true,
                    child: GestureDetector(
                      behavior: HitTestBehavior.opaque,
                      onTap: () => Navigator.of(context).maybePop(),
                      child: SizedBox(
                        height: minTouch,
                        child: Center(
                          child: Container(
                            height: 30,
                            padding: const EdgeInsets.fromLTRB(6, 0, 11, 0),
                            decoration: BoxDecoration(
                              color: const Color(0x2EFFFFFF),
                              borderRadius: BorderRadius.circular(15),
                              border: Border.all(
                                width: .5,
                                color: const Color(0x59FFFFFF),
                              ),
                            ),
                            child: const Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Icon(
                                  Icons.chevron_left,
                                  size: 20,
                                  color: Colors.white,
                                ),
                                Text(
                                  'Bobix',
                                  style: TextStyle(
                                    fontSize: 13,
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
                ),
              ],
            ),
          ),
          if (topOverlay != null)
            Positioned(
              top: 11,
              left: 0,
              right: 0,
              child: Center(child: topOverlay),
            ),
        ],
      ),
    );
  }
}
