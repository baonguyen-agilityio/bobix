import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter_svg/flutter_svg.dart';

import '../pets/creature.dart';

/// A pet as a PNG, drawn from the same SVG the app shows (`flatSvg`), for the widgets: they cannot run Dart.
Future<Uint8List> renderPetPng(
  String design,
  int color,
  String state, {
  int px = 144,
}) async {
  // `flatSvg` is a whole <svg> on a 36 x 36 box.
  final info = await vg.loadPicture(
    SvgStringLoader(flatSvg(design, color, state, poseAt(state))),
    null,
  );
  final recorder = ui.PictureRecorder();
  final canvas = ui.Canvas(recorder);
  canvas.scale(px / info.size.width, px / info.size.height);
  canvas.drawPicture(info.picture);
  final image = await recorder.endRecording().toImage(px, px);
  final data = await image.toByteData(format: ui.ImageByteFormat.png);
  info.picture.dispose();
  return data!.buffer.asUint8List();
}
