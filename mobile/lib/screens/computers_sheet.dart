import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/floating_sheet.dart';
import '../widgets/glass.dart';

/// The sheet the computer chip on Home opens: the computers this phone is connected to, with how many things wait on each.
/// One computer at a time for now, so it says so.
Future<void> showComputersSheet(
  BuildContext context, {
  required String computer,
  required int waiting,
}) => showModalBottomSheet<void>(
  context: context,
  isScrollControlled: true,
  backgroundColor: Colors.transparent,
  builder: (_) => _ComputersSheet(computer: computer, waiting: waiting),
);

class _ComputersSheet extends ConsumerWidget {
  const _ComputersSheet({required this.computer, required this.waiting});
  final String computer;
  final int waiting;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return FloatingSheet(
      children: [
        Semantics(
          header: true,
          child: Text(
            s('phone.sheet.t'),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 18,
              fontWeight: FontWeight.w700,
              height: 1.25,
              color: t.ink,
            ),
          ),
        ),
        const SizedBox(height: 12),
        Semantics(
          checked: true,
          inMutuallyExclusiveGroup: true,
          label: computer,
          child: Glass(
            tint: t.accentBg,
            border: t.accent,
            padding: const EdgeInsets.all(14),
            child: Row(
              children: [
                Icon(Icons.radio_button_checked, color: t.accentSolid),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        computer,
                        style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w600,
                          color: t.ink,
                        ),
                      ),
                      Text(
                        s('phone.sheet.need', {'n': waiting}),
                        style: TextStyle(fontSize: 13, color: t.ink2),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 10),
        Text(
          s('phone.sheet.one'),
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 12, color: t.ink3),
        ),
        const SizedBox(height: 10),
        PillButton(
          label: s('phone.a.close'),
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
    );
  }
}
