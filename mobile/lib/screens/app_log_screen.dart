import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/l10n.dart';
import '../lock/secure_scope.dart';
import '../log/activity_log.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
import 'task_log_screen.dart';

/// The log of what belongs to no task: pairing, disconnects and tickets started from this phone. In memory only.
class AppLogScreen extends ConsumerWidget {
  const AppLogScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return SecureScope(
      child: DetailScaffold(
        title: s('phone.al.title'),
        children: [
          Padding(
            padding: const EdgeInsets.only(bottom: 4),
            child: Text(
              s('phone.al.n'),
              style: TextStyle(fontSize: 14, height: 1.5, color: t.ink2),
            ),
          ),
          LogEntries(
            entries: ref.watch(appLogProvider),
            empty: s('phone.al.empty'),
          ),
        ],
      ),
    );
  }
}
