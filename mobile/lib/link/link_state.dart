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
  });

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
  );
}

/// The paired computer, in one small file. Writes are whole-file and atomic (a temporary file, then a rename), so a crash
/// leaves the old or the new state, never half of one.
class LinkStateStore {
  LinkStateStore(Directory dir) : _file = File('${dir.path}/link-state.json');

  final File _file;

  Future<PairedComputer?> load() async {
    try {
      if (!await _file.exists()) return null;
      final j = jsonDecode(await _file.readAsString());
      return PairedComputer.fromJson(j['computer'] as Map<String, dynamic>);
    } on Object {
      // An unreadable file is the same as none: the phone asks to pair again, and the Keychain keys are erased with it.
      return null;
    }
  }

  Future<void> save(PairedComputer c) async {
    final tmp = File('${_file.path}.tmp');
    await tmp.writeAsString(jsonEncode({'computer': c.toJson()}), flush: true);
    await tmp.rename(_file.path);
  }

  Future<void> erase() async {
    for (final f in [_file, File('${_file.path}.tmp')]) {
      if (await f.exists()) await f.delete();
    }
  }
}
