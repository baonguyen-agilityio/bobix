import 'dart:async';
import 'dart:convert';

import 'package:phone/link/relay_session.dart';

class ScriptedSocket implements RelaySocket {
  final _in = StreamController<String>();
  final sent = <Map<String, dynamic>>[];
  bool closed = false;

  @override
  Stream<String> get frames => _in.stream;

  @override
  void send(String text) => sent.add(jsonDecode(text) as Map<String, dynamic>);

  @override
  Future<void> close() async {
    closed = true;
    if (!_in.isClosed) await _in.close();
  }

  void push(Map<String, Object?> frame) => _in.add(jsonEncode(frame));

  Iterable<Map<String, dynamic>> ofType(String t) =>
      sent.where((f) => f['type'] == t);
}
