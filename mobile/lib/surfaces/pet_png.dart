import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter_svg/flutter_svg.dart';

import '../pets/creature.dart';

/// A pet as a PNG (the widgets are not Flutter, they show images): the same drawing as `PetView`, in its still pose.
Future<Uint8List> renderPetPng(
  String design,
  int color,
  String state, {
  int size = 96,
}) async {
  final svg = flatSvg(design, color, state, poseAt(state));
  final info = await vg.loadPicture(SvgStringLoader(svg), null);
  final scale = size / info.size.width;
  final recorder = ui.PictureRecorder();
  final canvas = ui.Canvas(recorder);
  canvas.scale(scale);
  canvas.drawPicture(info.picture);
  final image = await recorder.endRecording().toImage(size, size);
  final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
  info.picture.dispose();
  return bytes!.buffer.asUint8List();
}
