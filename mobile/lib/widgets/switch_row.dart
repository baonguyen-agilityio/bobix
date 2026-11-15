import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// A row of a settings group that holds one switch: a title and a second line on the left, the switch at the end. The
/// whole row is the target, so a thumb need not hit the switch itself. The switch is green through the theme.
class SwitchRow extends StatelessWidget {
  const SwitchRow({
    super.key,
    required this.title,
    required this.subtitle,
    required this.value,
    required this.onChanged,
    this.switchKey,
  });

  final String title, subtitle;
  final bool value;
  final ValueChanged<bool> onChanged;

  /// Names the switch itself, for tests and for finding it on screen.
  final Key? switchKey;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      toggled: value,
      label: title,
      hint: subtitle,
      child: Material(
        type: MaterialType.transparency,
        child: InkWell(
          onTap: () => onChanged(!value),
          child: ExcludeSemantics(
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: 60),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 14,
                  vertical: 8,
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            title,
                            style: TextStyle(
                              fontSize: 15,
                              fontWeight: FontWeight.w500,
                              color: t.ink,
                            ),
                          ),
                          Text(
                            subtitle,
                            style: TextStyle(
                              fontSize: 12.5,
                              height: 1.35,
                              color: t.ink3,
                            ),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(width: 12),
                    Switch(key: switchKey, value: value, onChanged: onChanged),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
