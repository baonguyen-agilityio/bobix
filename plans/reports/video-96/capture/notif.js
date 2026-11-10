ObjC.import('CoreGraphics'); ObjC.import('Foundation');
// notif.js: on-screen windows of Notification Center (id w h), largest first
function run() {
  const l = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0)));
  return l.filter(w => /Notification/i.test(w.kCGWindowOwnerName)).map(w => [w.kCGWindowNumber, w.kCGWindowBounds.Width, w.kCGWindowBounds.Height, w.kCGWindowBounds.X, w.kCGWindowBounds.Y, w.kCGWindowLayer, w.kCGWindowName || ''].join(' ')).join('\n');
}
