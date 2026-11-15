import 'package:flutter_riverpod/flutter_riverpod.dart';

/// What happened on this phone that the log lists.
enum LogKind {
  allow,
  deny,
  approve,
  back,
  answer,
  stop,
  start,
  startQuick,
  pair,
  disconnect,
}

/// One line of the log: a command sent from this phone (with the session it was for) or a pairing event. Kept in memory
/// only, for this run of the app: what a command said is never written to disk (see the privacy notes), so the log
/// starts empty when the app starts.
class LogEntry {
  const LogEntry({
    required this.at,
    required this.kind,
    this.sessionId,
    this.project = '',
    this.tool = '',
    this.detail = '',
    this.applied = true,
  });

  final DateTime at;
  final LogKind kind;

  /// The session the command was for; null for a pairing event.
  final String? sessionId;
  final String project;

  /// The tool a permission command was about ("Bash"); empty for the other kinds.
  final String tool;

  /// What the command was about (the tool and its input, the issue); empty when there is nothing to show.
  final String detail;

  /// Whether the computer applied it (false: it arrived too late, or was not received).
  final bool applied;
}

class ActivityLog extends Notifier<List<LogEntry>> {
  @override
  List<LogEntry> build() => const [];

  /// Adds [e] at the top (newest first).
  void record(LogEntry e) => state = [e, ...state];

  void clear() => state = const [];
}

final activityLogProvider = NotifierProvider<ActivityLog, List<LogEntry>>(
  ActivityLog.new,
);

/// The entries of one session, newest first.
final taskLogProvider = Provider.family<List<LogEntry>, String>(
  (ref, id) => [
    for (final e in ref.watch(activityLogProvider))
      if (e.sessionId == id) e,
  ],
);

/// The entries that belong to no session (pairing, disconnecting), newest first.
final appLogProvider = Provider<List<LogEntry>>(
  (ref) => [
    for (final e in ref.watch(activityLogProvider))
      if (e.sessionId == null) e,
  ],
);
