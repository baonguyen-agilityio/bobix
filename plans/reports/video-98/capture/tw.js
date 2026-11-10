ObjC.import('CoreGraphics'); ObjC.import('Foundation');
// tw.js PID: on-screen windows of that process, front to back: id name width height
function run(argv) { const l = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0)));
  return l.filter(w => String(w.kCGWindowOwnerPID) === argv[0] && w.kCGWindowLayer === 0).map(w => [w.kCGWindowNumber, w.kCGWindowName, w.kCGWindowBounds.Width, w.kCGWindowBounds.Height].join(' | ')).join('\n'); }
