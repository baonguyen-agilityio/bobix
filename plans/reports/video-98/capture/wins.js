ObjC.import('CoreGraphics'); ObjC.import('Foundation');
function run(argv) { const l = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(0, 0)));
  return l.filter(w => String(w.kCGWindowOwnerPID) === argv[0]).map(w => [w.kCGWindowNumber, w.kCGWindowName, w.kCGWindowBounds.Width, w.kCGWindowBounds.Height, w.kCGWindowIsOnscreen, w.kCGWindowLayer].join(' ')).join('\n'); }
