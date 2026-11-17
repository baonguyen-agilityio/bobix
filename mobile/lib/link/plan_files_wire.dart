import '../features/actions/actions_models.dart';
import '../src/rust/api/core.dart' as core;

/// The files a `detail` of a plan carries, as the screens show them; none from an older computer.
List<PlanFileDoc> planFilesOf(Map<String, dynamic> detail) => [
  for (final f
      in (detail['files'] as List? ?? const []).cast<Map<String, dynamic>>())
    PlanFileDoc(
      name: f['name'] as String,
      path: f['path'] as String,
      bytes: (f['bytes'] as num).toInt(),
      text: f['text'] as String,
      cut: f['cut'] == true,
      sent: f['sent'] == true,
    ),
];

/// The same files for the crate's `shown` hash: everything the phone shows of each.
List<core.ShownFile> shownFilesOf(List<PlanFileDoc> files) => [
  for (final f in files)
    core.ShownFile(
      name: f.name,
      path: f.path,
      bytes: BigInt.from(f.bytes),
      text: f.text,
      cut: f.cut,
      sent: f.sent,
    ),
];
