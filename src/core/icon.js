// The app's pixel creature as raw pixels: used for the tray icon (main.js) and the app icon (scripts/make-icon.js).
const ICON = ['.o....o.', 'obo..obo', 'obboobbo', 'obbbbbbo', 'obebbebo', 'obbbbbbo', '.obbbbo.', '.oo..oo.'];
const COLORS = { o: [90, 50, 70], b: [200, 190, 236], e: [30, 30, 40] }; // RGB

// size×size pixels, 4 bytes each, creature centered with `pad` pixels around it. order 'rgba' or 'bgra'.
function bitmap(size, pad = 0, order = 'rgba') {
  const buf = Buffer.alloc(size * size * 4), cell = (size - 2 * pad) / 8;
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const gx = Math.floor((x - pad) / cell), gy = Math.floor((y - pad) / cell);
    const c = COLORS[ICON[gy]?.[gx]];
    if (!c) continue;
    const [r, g, b] = c;
    buf.set(order === 'bgra' ? [b, g, r, 255] : [r, g, b, 255], (y * size + x) * 4);
  }
  return buf;
}

module.exports = { ICON, bitmap };
