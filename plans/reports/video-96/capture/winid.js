ObjC.import('CoreGraphics'); ObjC.import('Foundation');
// winid.js PID WIDTH: the on-screen window titled Bobix of that process with that width
function run(argv) {
  const l = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0)));
  const w = l.find(w => String(w.kCGWindowOwnerPID) === argv[0] && w.kCGWindowName === 'Bobix' && String(w.kCGWindowBounds.Width) === argv[1]);
  return w ? String(w.kCGWindowNumber) : '';
}
