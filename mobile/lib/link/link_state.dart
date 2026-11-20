import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/foundation.dart' show ValueListenable, ValueNotifier;

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
    required PhoneRights rights,
    required this.recvState,
    required this.eventReservation,
    this.home,
    this.homeAt,
    this.pairedAt,
    Set<String>? pending,
  }) : _rights = ValueNotifier(rights),
       pending = pending ?? {};

  /// The computer's device id.
  final String id;
  final String name;

  /// The relay this phone reaches the computer through. Changes only by a sealed `relayMoved` the phone acknowledged.
  String relay;
  final String phoneName;
  final Uint8List desktopBoxPublic;

  final ValueNotifier<PhoneRights> _rights;

  /// What the computer granted. Changes when a `home` says the owner changed it on the computer.
  PhoneRights get rights => _rights.value;
  set rights(PhoneRights r) => _rights.value = r;

  /// Screens that show the rights listen here, so a change made on the computer redraws them.
  ValueListenable<PhoneRights> get rightsListenable => _rights;

  /// The receive counters of what the computer sends (JSON of the Rust core).
  String recvState;

  /// The last value the send counter asked to store, so counters never go back after a restart.
  BigInt eventReservation;

  /// The last `home` the computer sent (its JSON) and when this phone received it (Unix ms). Never a `detail`: whole
  /// inputs and plans stay in memory.
  String? home;
  int? homeAt;

  /// When the pairing was made (Unix ms); null for a pairing made before this was kept.
  final int? pairedAt;

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
    'pairedAt': pairedAt,
    'pending': pending.toList(),
  };

  /// The cached `home`, kept apart from [toJson]: it is large and changes often, and a lost cache costs nothing.
  Map<String, Object?> homeToJson() => {'id': id, 'home': home, 'homeAt': homeAt};

  /// Reads the state. An older file also holds the `home` and `homeAt` (before the cache had its own file): they are read
  /// too, and [LinkStateStore.load] moves them.
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
    pairedAt: j['pairedAt'] as int?,
    pending: {...?(j['pending'] as List?)?.cast<String>()},
  );
}

/// The stored state could not be read. [corrupt] when the file is there but is not valid (it never will be), otherwise
/// the read failed for now (the next try may work). Never the same as "there is no pairing".
class LinkStateUnreadable implements Exception {
  const LinkStateUnreadable({required this.corrupt});
  final bool corrupt;
}

/// The paired computer, in two small files. `link-state.json` holds the pairing and the counters that must be durable
/// before the next packet is handled; `home-cache.json` holds the last `home` the computer sent, written later and
/// seldom. Writes are whole-file and atomic (a temporary file, then a rename), so a crash leaves the old or the new
/// state, never half of one.
class LinkStateStore {
  LinkStateStore(Directory dir, {this.homeDelay = const Duration(seconds: 5)})
    : _file = File('${dir.path}/link-state.json'),
      _homeFile = File('${dir.path}/home-cache.json');

  final File _file;
  final File _homeFile;

  /// How long a changed `home` waits for the next one before it is written.
  final Duration homeDelay;

  /// Whether any state is stored, whatever its content.
  Future<bool> exists() async =>
      await FileSystemEntity.type(_file.path) != FileSystemEntityType.notFound;

  /// The paired computer; null only when nothing is stored. A file that cannot be read or understood throws
  /// [LinkStateUnreadable]: the app must not start a new pairing over it. The cached `home` is only a cache: one that
  /// cannot be read is left out.
  Future<PairedComputer?> load() async {
    final String text;
    try {
      if (!await exists()) return null;
      text = await _file.readAsString();
    } on FileSystemException {
      throw const LinkStateUnreadable(corrupt: false);
    }
    final Map<String, dynamic> j;
    final PairedComputer c;
    try {
      j =
          (jsonDecode(text) as Map<String, dynamic>)['computer']
              as Map<String, dynamic>;
      c = PairedComputer.fromJson(j);
    } on Object {
      throw const LinkStateUnreadable(corrupt: true);
    }
    if (j['home'] is String) {
      // The state of an install from before the cache had its own file: move the `home` over, once.
      await _migrate(c);
    } else {
      await _readHomeCache(c);
    }
    return c;
  }

  Future<void> _readHomeCache(PairedComputer c) async {
    try {
      final j =
          jsonDecode(await _homeFile.readAsString()) as Map<String, dynamic>;
      final h = j['home'];
      final at = j['homeAt'];
      // A cache left by another pairing (the app was killed during Erase) is not this computer's.
      if (j['id'] == c.id && h is String && at is int) {
        c
          ..home = h
          ..homeAt = at;
      }
    } on Object {
      // No cache, or a broken one: the next `home` fills it.
    }
  }

  Future<void> _migrate(PairedComputer c) async {
    try {
      await _writeHome(c);
      await save(c);
    } on Object {
      // The old file stays as it was and the next start tries again; the `home` is in memory either way.
    }
  }

  Future<void> _writes = Future.value();
  Future<void> _homeWrites = Future.value();
  Timer? _homeTimer;
  PairedComputer? _homeOwner;
  bool _erased = false;

  /// Completes when every write asked for so far is done; a `home` still waiting for its turn is written now.
  Future<void> flush() async {
    final owner = _homeOwner;
    _homeTimer?.cancel();
    _homeTimer = null;
    _homeOwner = null;
    if (owner != null) unawaited(_queueHome(owner));
    await _writes.catchError((Object _) {});
    await _homeWrites.catchError((Object _) {});
  }

  /// Writes are one after another: two at once would share the temporary file.
  Future<void> save(PairedComputer c) {
    final json = jsonEncode({'computer': c.toJson()});
    return _writes = _writes.catchError((Object _) {}).then((_) async {
      if (_erased) {
        return; // nothing is written back after the pairing was erased
      }
      await _atomic(_file, json);
    });
  }

  /// Asks for the last `home` of [c] to be kept: written once [homeDelay] has passed, with whatever it holds by then.
  void saveHome(PairedComputer c) {
    if (_erased) return;
    _homeOwner = c;
    _homeTimer ??= Timer(homeDelay, () {
      final owner = _homeOwner;
      _homeTimer = null;
      _homeOwner = null;
      if (owner != null) unawaited(_queueHome(owner));
    });
  }

  Future<void> _queueHome(PairedComputer c) => _homeWrites = _homeWrites
      .catchError((Object _) {})
      .then((_) => _writeHome(c))
      .catchError((Object _) {});

  Future<void> _writeHome(PairedComputer c) async {
    if (_erased || c.home == null) return;
    await _atomic(_homeFile, jsonEncode(c.homeToJson()));
  }

  Future<void> _atomic(File f, String json) async {
    final tmp = File('${f.path}.tmp');
    await tmp.writeAsString(json, flush: true);
    await tmp.rename(f.path);
  }

  Future<void> erase() {
    _erased = true;
    _homeTimer?.cancel();
    _homeTimer = null;
    _homeOwner = null;
    return Future.wait([
      _writes = _writes.catchError((Object _) {}).then((_) => _delete(_file)),
      _homeWrites = _homeWrites
          .catchError((Object _) {})
          .then((_) => _delete(_homeFile)),
    ]).then((_) {});
  }

  Future<void> _delete(File f) async {
    for (final x in [f, File('${f.path}.tmp')]) {
      if (await x.exists()) await x.delete();
    }
  }
}
