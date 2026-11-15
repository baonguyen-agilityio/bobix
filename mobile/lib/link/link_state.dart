import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import '../src/rust/api/messages.dart';

/// URL-safe base64 without padding: how the protocol writes keys, ids and packets.
String b64(List<int> bytes) => base64Url.encode(bytes).replaceAll('=', '');
Uint8List b64d(String text) => base64Url.decode(base64Url.normalize(text));

PhoneRights rightsFromJson(Map<String, dynamic> j) => PhoneRights(
  answer: j['answer'] == true,
  plan: j['plan'] == true,
  question: j['question'] == true,
  start: j['start'] == true,
  stop: j['stop'] == true,
  activity: j['activity'] == true,
);

Map<String, bool> rightsToJson(PhoneRights r) => {
  'answer': r.answer,
  'plan': r.plan,
  'question': r.question,
  'start': r.start,
  'stop': r.stop,
  'activity': r.activity,
};

/// The one computer this phone is paired with. Nothing secret: the `auth` and `box` keys are in the Keychain, the `sign`
/// key in the Secure Enclave. Kept in an app-private file that is not backed up.
class PairedComputer {
  PairedComputer({
    required this.id,
    required this.name,
    required this.relay,
    required this.phoneName,
    required this.desktopBoxPublic,
    required this.rights,
    required this.recvState,
    required this.eventReservation,
    this.home,
    this.homeAt,
    Set<String>? pending,
  }) : pending = pending ?? {};

  /// The computer's device id.
  final String id;
  final String name;

  /// The relay this phone reaches the computer through. Changes only by a sealed `relayMoved` the phone acknowledged.
  String relay;
  final String phoneName;
  final Uint8List desktopBoxPublic;

  /// What the computer granted.
  final PhoneRights rights;

  /// The receive counters of what the computer sends (JSON of the Rust core).
  String recvState;

  /// The last value the send counter asked to store, so counters never go back after a restart.
  BigInt eventReservation;

  /// The last `home` the computer sent (its JSON) and when this phone received it (Unix ms). Never a `detail`: whole
  /// inputs and plans stay in memory.
  String? home;
  int? homeAt;

  /// The ids of commands sent whose results have not come (ids only, never what they asked): the next `sync` names them.
  final Set<String> pending;

  Map<String, Object?> toJson() => {
    'id': id,
    'name': name,
    'relay': relay,
    'phoneName': phoneName,
    'desktopBoxPublic': b64(desktopBoxPublic),
    'rights': rightsToJson(rights),
    'recvState': recvState,
    'eventReservation': eventReservation.toString(),
    'home': home,
    'homeAt': homeAt,
    'pending': pending.toList(),
  };

  static PairedComputer fromJson(Map<String, dynamic> j) => PairedComputer(
    id: j['id'] as String,
    name: j['name'] as String,
    relay: j['relay'] as String,
    phoneName: j['phoneName'] as String,
    desktopBoxPublic: b64d(j['desktopBoxPublic'] as String),
    rights: rightsFromJson(j['rights'] as Map<String, dynamic>),
    recvState: j['recvState'] as String,
    eventReservation: BigInt.parse(j['eventReservation'] as String),
    home: j['home'] as String?,
    homeAt: j['homeAt'] as int?,
    pending: {...?(j['pending'] as List?)?.cast<String>()},
  );
}

/// The stored state could not be read. [corrupt] when the file is there but is not valid (it never will be), otherwise
/// the read failed for now (the next try may work). Never the same as "there is no pairing".
class LinkStateUnreadable implements Exception {
  const LinkStateUnreadable({required this.corrupt});
  final bool corrupt;
}

/// The paired computer, in one small file. Writes are whole-file and atomic (a temporary file, then a rename), so a crash
/// leaves the old or the new state, never half of one.
class LinkStateStore {
  LinkStateStore(Directory dir) : _file = File('${dir.path}/link-state.json');

  final File _file;

  /// Whether any state is stored, whatever its content.
  Future<bool> exists() async =>
      await FileSystemEntity.type(_file.path) != FileSystemEntityType.notFound;

  /// The paired computer; null only when nothing is stored. A file that cannot be read or understood throws
  /// [LinkStateUnreadable]: the app must not start a new pairing over it.
  Future<PairedComputer?> load() async {
    final String text;
    try {
      if (!await exists()) return null;
      text = await _file.readAsString();
    } on FileSystemException {
      throw const LinkStateUnreadable(corrupt: false);
    }
    try {
      final j = jsonDecode(text) as Map<String, dynamic>;
      return PairedComputer.fromJson(j['computer'] as Map<String, dynamic>);
    } on Object {
      throw const LinkStateUnreadable(corrupt: true);
    }
  }

  Future<void> _writes = Future.value();
  bool _erased = false;

  /// Completes when every write asked for so far is done.
  Future<void> flush() => _writes.catchError((Object _) {});

  /// Writes are one after another: two at once would share the temporary file.
  Future<void> save(PairedComputer c) {
    final json = jsonEncode({'computer': c.toJson()});
    return _writes = _writes.catchError((Object _) {}).then((_) async {
      if (_erased) {
        return; // nothing is written back after the pairing was erased
      }
      final tmp = File('${_file.path}.tmp');
      await tmp.writeAsString(json, flush: true);
      await tmp.rename(_file.path);
    });
  }

  Future<void> erase() {
    _erased = true;
    return _writes = _writes.catchError((Object _) {}).then((_) async {
      for (final f in [_file, File('${_file.path}.tmp')]) {
        if (await f.exists()) await f.delete();
      }
    });
  }
}
