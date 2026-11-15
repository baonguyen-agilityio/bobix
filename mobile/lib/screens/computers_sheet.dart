import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
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
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(8),
        child: Container(
          padding: const EdgeInsets.fromLTRB(20, 14, 20, 16),
          decoration: BoxDecoration(
            color: t.surface,
            borderRadius: BorderRadius.circular(32),
            border: Border.all(width: .5, color: t.line),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Semantics(
                header: true,
                child: Text(
                  s('phone.sheet.t'),
                  style: TextStyle(
                    fontSize: 18,
                    fontWeight: FontWeight.w700,
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
              const SizedBox(height: 8),
              Text(
                s('phone.sheet.one'),
                style: TextStyle(fontSize: 12, color: t.ink3),
              ),
              const SizedBox(height: 12),
              SizedBox(
                width: double.infinity,
                child: PillButton(
                  label: s('phone.a.close'),
                  onPressed: () => Navigator.of(context).pop(),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
