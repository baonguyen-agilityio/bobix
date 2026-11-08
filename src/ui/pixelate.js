// A pet picture (GIF, PNG, JPG or WebP) turned once, at import, into small pixel art that looks like the code-drawn
// creatures (ui/creature.js): flat colours, a 1 px dark outline, hard edges. Plain JavaScript with no Node or browser
// API, so it runs in the panel's Web Worker (pixelate-worker.js) and in the node tests; PNG/JPG/WebP are decoded by the
// page and handed over as pixels (fromRGBA). The GIF helpers in gif/ are CommonJS: the worker loads them into `exports`.
(function (root) {
'use strict';
const { lzw } = typeof require === 'function' ? require('./gif/lzw') : root.exports;
const { deinterlace } = typeof require === 'function' ? require('./gif/deinterlace') : root.exports;
const { GIFEncoder } = typeof require === 'function' ? require('./gif/gifenc') : root.exports;

// Byte helpers (no Node Buffer here).
const latin1 = (b, from, to) => String.fromCharCode(...b.subarray(from, to));
function concat(parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) { out.set(p, at); at += p.length; }
  return out;
}

// whole: the pet is the picture itself, not a face in its project's creature. How the pet wears it; the pixel art ignores it.
const DEFAULTS = { size: 48, colors: 16, accents: 3, clean: true, outline: true, removeBg: false, whole: false };
const SIZES = [32, 48, 64];
const COLORS = [4, 32];            // [min, max] palette size, outline and accents included
const ACCENTS = [0, 8];            // [min, max] accent colours; normalizeOpts also leaves 2 for median cut
const ALPHA_CUT = 128;             // alpha from here up is opaque: the art has no soft edges
const MAX_FRAMES = 200;            // frames past this are dropped (Source.truncated); 200 frames of 256² is ~52 MB held
const SOURCE_SIDE = 256;           // a Source's long side; enough for 64 px art, small enough to hold every frame
const MAX_CANVAS = 4096 * 4096;    // a GIF's logical screen in pixels: a tiny file can claim 65535², which we never allocate
// Pixels decoded and shrunk over all frames (each frame's patch plus the rectangle that changed): a small file can claim
// many full-screen frames, which would take longer than pet-import.js waits for the worker (PIXEL_WAIT_MS); frames past it
// are dropped like those past MAX_FRAMES. A 2 MB GIF of 70 full 480×480 frames uses about 6% of it.
const MAX_WORK = 512 * 1024 * 1024;
const BG_THRESHOLD = 40;           // RGB distance from a corner colour that still counts as background (JPG noise, soft edges)
// A plain background: nearly all of the border is within PLAIN_THRESHOLD of the corners' mean colour.
// Only then is background removal on by default: in a photo the corner colours run into the subject (a dark sea into
// a dark coat), so removing them would cut the subject open.
const PLAIN_THRESHOLD = 24, PLAIN_BORDER = 0.9;
const CLEAN_NEAR = 40 * 40;        // colour distance (see dist) under which a lone pixel is noise, not a detail like an eye

// A colour as a person types it -> '#rrggbb', or null when it is not one: #rgb, #rgba, #rrggbb, #rrggbbaa, rgb()/rgba()
// and hsl()/hsla() with commas or spaces, numbers or percentages. Alpha must be a number but is dropped: the art has no
// soft edges. (The panel parses with the browser, which also knows colour names, and sends #rrggbb; this guards IPC and
// settings.json.)
const HUE = { '': 1, deg: 1, grad: 0.9, rad: 180 / Math.PI, turn: 360 }; // hsl hue units, in degrees
function parseColor(v) {
  if (typeof v !== 'string' || v.length > 64) return null; // short, so no pattern below can take long
  const s = v.trim().toLowerCase(), hex = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/.exec(s);
  if (hex) return '#' + (hex[1].length < 6 ? hex[1].replace(/./g, '$&$&') : hex[1]).slice(0, 6);
  const fn = /^(rgb|hsl)a?\(([^()]*)\)$/.exec(s), hsl = fn?.[1] === 'hsl';
  const parts = fn ? fn[2].trim().split(/\s*[,/]\s*|\s+/).map(a => /^([+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?)(%|deg|g?rad|turn)?$/.exec(a)) : [];
  if (parts.length < 3 || parts.length > 4 || parts.some(p => !p)) return null;
  const n = parts.map(p => +p[1]), unit = parts.map(p => p[2] || '');
  if (!n.every(Number.isFinite) || (hsl && !Object.hasOwn(HUE, unit[0])) || unit.slice(hsl ? 1 : 0).some(u => u && u !== '%')) return null;
  const byte = x => Math.round(Math.min(255, Math.max(0, x)));
  let rgb = n.slice(0, 3).map((x, i) => byte(unit[i] === '%' ? x * 255 / 100 : x));
  if (hsl) { // CSS Color 4's hsl-to-rgb
    const h = (n[0] * HUE[unit[0]] % 360 + 360) % 360, [sat, l] = [n[1], n[2]].map(x => Math.min(100, Math.max(0, x)) / 100), a = sat * Math.min(l, 1 - l);
    rgb = [0, 8, 4].map(k => { k = (k + h / 30) % 12; return byte((l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))) * 255); });
  }
  return '#' + rgb.map(x => x.toString(16).padStart(2, '0')).join('');
}

// Untrusted options (IPC, settings.json) -> options pixelate can use. removeBg defaults to whether the picture has no alpha.
// outlineColor ('#rrggbb') is left out for the automatic dark outline, as in entries saved before it existed; it is kept
// while the outline is off, so turning the outline back on brings the colour back.
function normalizeOpts(input, { plainBg = false } = {}) {
  const o = input && typeof input === 'object' ? input : {};
  const num = v => typeof v === 'number' && Number.isFinite(v);
  const int = (v, lo, hi, def) => num(v) ? Math.min(hi, Math.max(lo, Math.round(v))) : def;
  const bool = (v, def) => typeof v === 'boolean' ? v : def;
  const size = num(o.size) ? SIZES.reduce((a, b) => Math.abs(b - o.size) < Math.abs(a - o.size) ? b : a) : DEFAULTS.size;
  const colors = int(o.colors, ...COLORS, DEFAULTS.colors);
  const outline = bool(o.outline, DEFAULTS.outline);
  const accents = Math.min(int(o.accents, ...ACCENTS, DEFAULTS.accents), colors - (outline ? 1 : 0) - 2);
  const outlineColor = parseColor(o.outlineColor);
  return { size, colors, accents, clean: bool(o.clean, DEFAULTS.clean), outline, removeBg: bool(o.removeBg, !!plainBg), whole: bool(o.whole, DEFAULTS.whole), ...(outlineColor && { outlineColor }) };
}

// ---------- resampling ----------
// For each output pixel along one axis: the source pixels it covers and how much of each (area / box filter).
function boxWeights(src, dst) {
  const k = src / dst, start = new Int32Array(dst + 1), idx = [], wt = [];
  for (let o = 0; o < dst; o++) {
    const a = o * k, b = (o + 1) * k;
    start[o] = idx.length;
    for (let i = Math.floor(a); i < Math.min(src, Math.ceil(b)); i++) {
      const w = Math.min(i + 1, b) - Math.max(i, a);
      if (w > 1e-9) { idx.push(i); wt.push(w); }
    }
  }
  start[dst] = idx.length;
  return { start, idx: Int32Array.from(idx), wt: Float32Array.from(wt), k };
}

// Area-average RGBA from sw×sh to dw×dh (dw ≤ sw, dh ≤ sh), weighted by alpha so transparent pixels lend no colour.
// Two separable passes on (r·a, g·a, b·a, a); alpha comes out soft (pixelate hardens it later). The returned function
// writes into `out` only the output pixels that cover the source rectangle [x0, x1) × [y0, y1): a GIF's delta frame
// redraws a small patch of a big screen, so only that patch is shrunk again.
function shrinker(sw, sh, dw, dh) {
  const X = boxWeights(sw, dw), Y = boxWeights(sh, dh), area = X.k * Y.k;
  return (rgba, out, x0 = 0, y0 = 0, x1 = sw, y1 = sh) => {
    if (dw === sw && dh === sh) { for (let y = y0; y < y1; y++) out.set(rgba.subarray((y * sw + x0) * 4, (y * sw + x1) * 4), (y * sw + x0) * 4); return out; }
    const ox0 = Math.floor(x0 / X.k), ox1 = Math.min(dw, Math.ceil(x1 / X.k)), oy0 = Math.floor(y0 / Y.k), oy1 = Math.min(dh, Math.ceil(y1 / Y.k));
    const sy0 = Y.idx[Y.start[oy0]], sy1 = Y.idx[Y.start[oy1] - 1] + 1, w = ox1 - ox0, tmp = new Float32Array(w * (sy1 - sy0) * 4);
    for (let y = sy0; y < sy1; y++) for (let ox = ox0; ox < ox1; ox++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let j = X.start[ox]; j < X.start[ox + 1]; j++) {
        const p = (y * sw + X.idx[j]) * 4, wt = X.wt[j] * rgba[p + 3];
        r += rgba[p] * wt; g += rgba[p + 1] * wt; b += rgba[p + 2] * wt; a += wt;
      }
      const t = ((y - sy0) * w + ox - ox0) * 4; tmp[t] = r; tmp[t + 1] = g; tmp[t + 2] = b; tmp[t + 3] = a;
    }
    for (let oy = oy0; oy < oy1; oy++) for (let ox = ox0; ox < ox1; ox++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let j = Y.start[oy]; j < Y.start[oy + 1]; j++) {
        const t = ((Y.idx[j] - sy0) * w + ox - ox0) * 4, wt = Y.wt[j];
        r += tmp[t] * wt; g += tmp[t + 1] * wt; b += tmp[t + 2] * wt; a += tmp[t + 3] * wt;
      }
      const p = (oy * dw + ox) * 4;
      if (a > 0) { out[p] = r / a; out[p + 1] = g / a; out[p + 2] = b / a; out[p + 3] = a / area; } else out.fill(0, p, p + 4);
    }
    return out;
  };
}
const resample = (sw, sh, rgba, dw, dh) => shrinker(sw, sh, dw, dh)(rgba, new Uint8ClampedArray(dw * dh * 4));

// The size that fits the long side in `side`, never larger than the picture (upscaling would only blur it).
function fit(w, h, side) {
  const k = Math.min(1, side / Math.max(w, h));
  return [Math.max(1, Math.round(w * k)), Math.max(1, Math.round(h * k))];
}

const isOpaque = rgba => { for (let i = 3; i < rgba.length; i += 4) if (rgba[i] !== 255) return false; return true; };

// ---------- decoders: everything becomes a Source of straight RGBA frames, long side ≤ SOURCE_SIDE ----------
// The GIF block structure (GIF87a / GIF89a), read in one pass without copying: the screen size, the global colour
// table, each image's descriptor, colour table and where its LZW data starts, the graphic control extension before it,
// and the NETSCAPE2.0 loop count. A file cut short keeps the images that are whole; none at all throws.
function parseGif(b) {
  const u16 = p => b[p] | b[p + 1] << 8;
  if (b.length < 13 || latin1(b, 0, 3) !== 'GIF') throw new Error('not a GIF');
  const table = (p, flags) => (flags & 0x80 ? b.subarray(p, p + 3 * (2 << (flags & 7))) : null);
  // A GIF without a loop block plays once in a browser; a pet keeps moving, so it loops forever (0) here too.
  const gif = { width: u16(6), height: u16(8), gct: table(13, b[10]), loop: 0, images: [] };
  let p = 13 + (gif.gct ? gif.gct.length : 0), gce = null;
  // Sub-blocks: a length byte, that many bytes, until a 0 length. Returns where they end, or -1 past the file's end.
  const skip = q => { while (q < b.length && b[q]) q += b[q] + 1; return q < b.length ? q + 1 : -1; };
  while (p < b.length && b[p] !== 0x3b) {
    if (b[p] === 0x21) { // extension
      const label = b[p + 1], end = skip(p + 2);
      if (end < 0) break;
      if (label === 0xf9 && b[p + 2] >= 4) gce = { disposal: b[p + 3] >> 2 & 7, delay: u16(p + 4), transparent: b[p + 3] & 1 ? b[p + 6] : -1 };
      const netscape = label === 0xff && b[p + 2] === 11 && /^(NETSCAPE2\.0|ANIMEXTS1\.0)$/.test(latin1(b, p + 3, p + 14));
      if (netscape && b[p + 14] >= 3 && b[p + 15] === 1) gif.loop = u16(p + 16); // sub-block [1, lo, hi]
      p = end;
    } else if (b[p] === 0x2c) { // image
      if (p + 10 > b.length) break;
      const flags = b[p + 9], lct = table(p + 10, flags), data = p + 10 + (lct ? lct.length : 0) + 1, end = skip(data);
      if (end < 0) break;
      gif.images.push({ left: u16(p + 1), top: u16(p + 3), width: u16(p + 5), height: u16(p + 7), interlaced: !!(flags & 0x40),
        table: lct || gif.gct, minCode: b[data - 1], data, end, gce });
      gce = null; p = end;
    } else break; // not a block: the rest of the file is damaged
  }
  return gif;
}

// One image's colour indexes: its LZW data sub-blocks joined, decoded by gifuct-js's lzw (missing pixels become 0).
function imagePixels(b, img) {
  const parts = [];
  for (let q = img.data; b[q]; q += b[q] + 1) parts.push(b.subarray(q + 1, q + 1 + b[q]));
  const px = lzw(img.minCode, concat(parts), img.width * img.height);
  return img.interlaced ? deinterlace(px, img.width) : px;
}

function decodeGif(buf) {
  const gif = parseGif(buf), W = gif.width, H = gif.height, images = gif.images;
  if (!(W > 0 && H > 0) || !images.length) throw new Error('not a GIF');
  if (W * H > MAX_CANVAS) throw new Error('GIF too large');
  const [sw, sh] = fit(W, H, SOURCE_SIDE), canvas = new Uint8ClampedArray(W * H * 4), frames = [], shrink = shrinker(W, H, sw, sh);
  // Frames are delta patches: composite them one at a time at full size and shrink each right away, so only one
  // full-size canvas is ever held. Each frame starts from the one before and shrinks again only the rectangle that
  // changed: this frame's patch and what the frame before cleared or restored. Only the frames kept are LZW-decoded.
  let work = 0, prev = null, undo = null; // undo: the rectangle the last disposal changed, [x0, y0, x1, y1]
  for (const d of images) {
    if (frames.length >= MAX_FRAMES) break;
    if (d.width * d.height > MAX_CANVAS) throw new Error('GIF too large');
    const x0 = Math.max(0, Math.min(W, d.left)), y0 = Math.max(0, Math.min(H, d.top)), x1 = Math.min(W, d.left + d.width), y1 = Math.min(H, d.top + d.height);
    const r = !prev ? [0, 0, W, H] : undo ? [Math.min(x0, undo[0]), Math.min(y0, undo[1]), Math.max(x1, undo[2]), Math.max(y1, undo[3])] : [x0, y0, x1, y1];
    if (frames.length && (work += (r[2] - r[0]) * (r[3] - r[1]) + d.width * d.height) > MAX_WORK) break;
    const px = imagePixels(buf, d), ct = d.table, n = ct ? ct.length / 3 : 0, tr = d.gce ? d.gce.transparent : -1;
    const disposal = d.gce ? d.gce.disposal : 0, rowBytes = Math.max(0, x1 - x0) * 4;
    const row = i => ((y0 + i) * W + x0) * 4; // where row i of the patch starts on the canvas
    const before = disposal === 3 ? Array.from({ length: Math.max(0, y1 - y0) }, (_, i) => canvas.slice(row(i), row(i) + rowBytes)) : null;
    for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) {
      const ci = px[(y - d.top) * d.width + (x - d.left)];
      if (ci === tr) continue; // a transparent patch pixel shows what is under it
      const p = (y * W + x) * 4, c = ci * 3;
      if (ci < n) { canvas[p] = ct[c]; canvas[p + 1] = ct[c + 1]; canvas[p + 2] = ct[c + 2]; }
      else canvas.fill(0, p, p + 3); // an index past the table is black, as browsers draw it
      canvas[p + 3] = 255;
    }
    // Delays are in centiseconds; 0 or no control block means 100 ms, as browsers play it. gifenc takes ms.
    prev = r[2] > r[0] && r[3] > r[1] ? shrink(canvas, prev ? prev.slice() : new Uint8ClampedArray(sw * sh * 4), ...r) : prev.slice();
    frames.push({ rgba: prev, delay: d.gce?.delay ? d.gce.delay * 10 : 100 });
    if (disposal === 2) for (let y = y0; y < y1; y++) canvas.fill(0, (y * W + x0) * 4, (y * W + x1) * 4);
    else if (disposal === 3) before.forEach((saved, i) => canvas.set(saved, row(i)));
    undo = disposal === 2 || disposal === 3 ? [x0, y0, x1, y1] : null;
  }
  return { width: sw, height: sh, frames, loop: gif.loop, opaque: frames.every(f => isOpaque(f.rgba)), plainBg: plainBackground(sw, sh, frames[0].rgba), truncated: frames.length < images.length };
}

// One still picture the main process decoded (straight RGBA).
function fromRGBA(width, height, rgba) {
  const ok = Number.isInteger(width) && Number.isInteger(height) && width > 0 && height > 0 && rgba?.length === width * height * 4;
  if (!ok) throw new Error('bad pixels');
  const [sw, sh] = fit(width, height, SOURCE_SIDE), out = resample(width, height, rgba, sw, sh);
  return { width: sw, height: sh, frames: [{ rgba: out, delay: 0 }], loop: 0, opaque: isOpaque(out), plainBg: plainBackground(sw, sh, out), truncated: false };
}

// ---------- colour ----------
// Distance in luma / chroma (YCbCr), chroma weighted ×4: a small saturated spot differs a lot from greys around it,
// which plain RGB distance underrates.
const ycc = c => { const y = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]; return [y, 0.564 * (c[2] - y), 0.713 * (c[0] - y)]; };
const dist = (a, b) => (a[0] - b[0]) ** 2 + 4 * ((a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2);
function nearest(q, pal) {
  let best = 0, bd = Infinity;
  for (let i = 0; i < pal.length; i++) { const d = dist(q, pal[i]); if (d < bd) { bd = d; best = i; } }
  return best;
}

// Every opaque colour of every frame, binned to 5 bits per channel: { n, c: mean colour } per non-empty bin.
const BINS = 1 << 15; // 5 bits each of r, g, b
function histogram(frames) {
  const n = new Uint32Array(BINS), s = new Float64Array(BINS * 3), bins = [];
  for (const f of frames) for (let p = 0; p < f.length; p += 4) {
    if (!f[p + 3]) continue;
    const k = (f[p] >> 3) << 10 | (f[p + 1] >> 3) << 5 | f[p + 2] >> 3;
    n[k]++; s[k * 3] += f[p]; s[k * 3 + 1] += f[p + 1]; s[k * 3 + 2] += f[p + 2];
  }
  for (let k = 0; k < BINS; k++) if (n[k]) bins.push({ n: n[k], c: [s[k * 3] / n[k], s[k * 3 + 1] / n[k], s[k * 3 + 2] / n[k]] });
  return bins;
}

// Median cut over the bins: split the box with the most pixels × widest channel range at its pixel median.
function medianCut(bins, count) {
  const range = box => { // [channel, its range] of the widest channel
    let best = [0, -1];
    for (let ch = 0; ch < 3; ch++) {
      let lo = 255, hi = 0;
      for (const b of box) { lo = Math.min(lo, b.c[ch]); hi = Math.max(hi, b.c[ch]); }
      if (hi - lo > best[1]) best = [ch, hi - lo];
    }
    return best;
  };
  const boxes = bins.length ? [bins.slice()] : [];
  while (boxes.length < count) {
    let pick = -1, ps = 0, ch = 0;
    for (let i = 0; i < boxes.length; i++) {
      if (boxes[i].length < 2) continue;
      const [c, r] = range(boxes[i]), s = r * boxes[i].reduce((a, b) => a + b.n, 0);
      if (s > ps) { ps = s; pick = i; ch = c; }
    }
    if (pick < 0) break;
    const box = boxes[pick].sort((a, b) => a.c[ch] - b.c[ch]), half = box.reduce((a, b) => a + b.n, 0) / 2;
    let at = 1, run = box[0].n;
    while (at < box.length - 1 && run + box[at].n <= half) run += box[at++].n;
    boxes.splice(pick, 1, box.slice(0, at), box.slice(at));
  }
  return boxes.map(box => {
    let n = 0; const c = [0, 0, 0];
    for (const b of box) { n += b.n; for (let ch = 0; ch < 3; ch++) c[ch] += b.c[ch] * b.n; }
    return c.map(v => Math.round(v / n));
  });
}

// Accents: colours median cut averages away because few pixels have them (an eye, a red nose). Each round takes the bin
// with the highest sqrt(pixels) × distance to its nearest colour already chosen, so the picks are distinct.
function addAccents(pal, bins, count) {
  const q = bins.map(b => ycc(b.c)), pq = pal.map(ycc), near = q.map(y => Math.min(...pq.map(c => dist(y, c))));
  for (let a = 0; a < count; a++) {
    let best = -1, bs = 0;
    for (let i = 0; i < bins.length; i++) { const s = Math.sqrt(bins[i].n) * near[i]; if (s > bs) { bs = s; best = i; } }
    if (best < 0) break;
    const c = bins[best].c.map(Math.round), y = ycc(c);
    pal.push(c);
    for (let i = 0; i < bins.length; i++) near[i] = Math.min(near[i], dist(q[i], y));
  }
  return pal;
}

// The outline colour: the darkest palette colour with its HSL lightness halved.
function outlineColor(pal) {
  const c = pal.reduce((a, b) => ycc(b)[0] < ycc(a)[0] ? b : a).map(v => v / 255);
  const mx = Math.max(...c), mn = Math.min(...c), l = (mx + mn) / 2, d = mx - mn;
  const s = d ? d / (1 - Math.abs(2 * l - 1)) : 0;
  let h = 0;
  if (d) h = mx === c[0] ? ((c[1] - c[2]) / d + 6) % 6 : mx === c[1] ? (c[2] - c[0]) / d + 2 : (c[0] - c[1]) / d + 4;
  const L = l / 2, C = (1 - Math.abs(2 * L - 1)) * s, X = C * (1 - Math.abs(h % 2 - 1)), m = L - C / 2;
  const rgb = [[C, X, 0], [X, C, 0], [0, C, X], [0, X, C], [X, 0, C], [C, 0, X]][Math.floor(h) % 6];
  return rgb.map(v => Math.round((v + m) * 255));
}

// ---------- background removal ----------
// Flood fill from the edges through pixels close to a corner colour (or already transparent). Only what is connected to
// the border goes, so a white eye inside a face on a white background stays.
// A corner unlike the others pulls the mean away from all of them, and a transparent border is never near it.
function plainBackground(w, h, rgba) {
  const at = i => [rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2], rgba[i * 4 + 3]];
  const corners = [0, w - 1, (h - 1) * w, h * w - 1].map(at);
  const mean = [0, 1, 2].map(ch => corners.reduce((a, c) => a + c[ch], 0) / 4), T2 = PLAIN_THRESHOLD ** 2;
  const near = c => c[3] >= ALPHA_CUT && (c[0] - mean[0]) ** 2 + (c[1] - mean[1]) ** 2 + (c[2] - mean[2]) ** 2 <= T2;
  const border = [];
  for (let x = 0; x < w; x++) border.push(x, (h - 1) * w + x);
  for (let y = 1; y < h - 1; y++) border.push(y * w, y * w + w - 1);
  return border.filter(i => near(at(i))).length >= PLAIN_BORDER * border.length;
}

function removeBackground(w, h, rgba) {
  const seeds = [0, w - 1, (h - 1) * w, h * w - 1].filter(i => rgba[i * 4 + 3] >= ALPHA_CUT).map(i => [rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]]);
  if (!seeds.length) return rgba;
  const T2 = BG_THRESHOLD * BG_THRESHOLD, seen = new Uint8Array(w * h), stack = new Int32Array(w * h);
  const bg = i => {
    const p = i * 4;
    return rgba[p + 3] < ALPHA_CUT || seeds.some(c => (c[0] - rgba[p]) ** 2 + (c[1] - rgba[p + 1]) ** 2 + (c[2] - rgba[p + 2]) ** 2 <= T2);
  };
  let top = 0, gone = 0;
  const push = i => { if (!seen[i] && bg(i)) { seen[i] = 1; stack[top++] = i; } };
  for (let x = 0; x < w; x++) { push(x); push((h - 1) * w + x); }
  for (let y = 0; y < h; y++) { push(y * w); push(y * w + w - 1); }
  while (top) {
    const i = stack[--top], x = i % w; gone++;
    if (x > 0) push(i - 1); if (x < w - 1) push(i + 1); if (i >= w) push(i - w); if (i < (h - 1) * w) push(i + w);
  }
  if (gone === w * h) return rgba; // all of it looked like background: keep the picture rather than show nothing
  const out = new Uint8ClampedArray(rgba);
  for (let i = 0; i < w * h; i++) if (seen[i]) out[i * 4 + 3] = 0;
  return out;
}

// ---------- pixel art ----------
const T = 255; // transparent while the palette is built; becomes palette.length at the end

// Stray pixels the downscale leaves behind, decided from a copy so one fix never triggers another (each rule needs all
// four neighbours, edges count as transparent):
// 1. an opaque pixel with no opaque neighbour goes (a speck the outline would ring);
// 2. a transparent pixel whose four neighbours are one opaque colour takes it (a pinhole the outline would dot);
// 3. an opaque pixel whose four neighbours are one other colour takes it, only when that colour is near its own
//    (dist < CLEAN_NEAR): shading noise goes, a 1 px eye or accent stays.
function clean(w, h, src, pal) {
  const out = new Uint8Array(src), Q = pal.map(ycc);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const i = y * w + x, p = src[i];
    const n = [x > 0 ? src[i - 1] : T, x < w - 1 ? src[i + 1] : T, y > 0 ? src[i - w] : T, y < h - 1 ? src[i + w] : T];
    if (p !== T && n.every(v => v === T)) out[i] = T;
    else if (n[0] !== T && n.every(v => v === n[0]) && n[0] !== p && (p === T || dist(Q[p], Q[n[0]]) < CLEAN_NEAR)) out[i] = n[0];
  }
  return out;
}

// A 1 px outline on transparent pixels that share an edge with the shape (no diagonal-only pixels, like creature.js).
function outline(w, h, src, o) {
  const out = new Uint8Array(src);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const i = y * w + x;
    if (src[i] !== T) continue;
    if ((x > 0 && src[i - 1] !== T) || (x < w - 1 && src[i + 1] !== T) || (y > 0 && src[i - w] !== T) || (y < h - 1 && src[i + w] !== T)) out[i] = o;
  }
  return out;
}

function pixelate(source, input) {
  const opts = normalizeOpts(input, { plainBg: source.plainBg });
  const [dw, dh] = fit(source.width, source.height, opts.size - 2), W = dw + 2, H = dh + 2;
  // background → downscale → alpha 0/255; the art keeps a 1 px border, so the outline never clips (also when it is off,
  // so the art does not change size when the option is toggled)
  const small = source.frames.map(f => {
    const rgba = resample(source.width, source.height, opts.removeBg ? removeBackground(source.width, source.height, f.rgba) : f.rgba, dw, dh);
    for (let p = 3; p < rgba.length; p += 4) rgba[p] = rgba[p] < ALPHA_CUT ? 0 : 255;
    return rgba;
  });
  const bins = histogram(small);
  const palette = addAccents(medianCut(bins, opts.colors - opts.accents - (opts.outline ? 1 : 0)), bins, opts.accents);
  const Q = palette.map(ycc), cache = new Map();
  let frames = small.map(rgba => {
    const index = new Uint8Array(W * H).fill(T);
    for (let y = 0; y < dh; y++) for (let x = 0; x < dw; x++) {
      const p = (y * dw + x) * 4;
      if (!rgba[p + 3]) continue;
      const key = rgba[p] << 16 | rgba[p + 1] << 8 | rgba[p + 2];
      let k = cache.get(key);
      if (k === undefined) cache.set(key, k = nearest(ycc([rgba[p], rgba[p + 1], rgba[p + 2]]), Q));
      index[(y + 1) * W + x + 1] = k;
    }
    return opts.clean ? clean(W, H, index, palette) : index;
  });
  if (opts.outline && palette.length) {
    // a chosen colour is used as it is: an equal palette entry is shared, a near one never stands in for it
    const c = opts.outlineColor ? [1, 3, 5].map(i => parseInt(opts.outlineColor.slice(i, i + 2), 16)) : outlineColor(palette);
    let o = palette.findIndex(p => p[0] === c[0] && p[1] === c[1] && p[2] === c[2]);
    if (o < 0) o = palette.push(c) - 1;
    frames = frames.map(f => outline(W, H, f, o));
  }
  for (const f of frames) for (let i = 0; i < f.length; i++) if (f[i] === T) f[i] = palette.length;
  return { width: W, height: H, palette, frames: frames.map((index, i) => ({ index, delay: source.frames[i].delay })), loop: source.loop };
}

// ---------- encoders ----------
const CRC = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
function crc32(b) { let c = 0xffffffff; for (const x of b) c = CRC[(c ^ x) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; }
const u32 = n => new Uint8Array([n >>> 24, n >>> 16 & 255, n >>> 8 & 255, n & 255]);

// zlib data in stored (uncompressed) blocks: no compressor needed, and 64 px art stays a few KB.
function zlibStored(raw) {
  const parts = [new Uint8Array([0x78, 0x01])];
  for (let i = 0; i < raw.length || i === 0; i += 65535) {
    const block = raw.subarray(i, i + 65535), n = block.length, last = i + 65535 >= raw.length;
    parts.push(new Uint8Array([last ? 1 : 0, n & 255, n >>> 8, ~n & 255, ~n >>> 8 & 255]), block);
  }
  let a = 1, b = 0;
  for (const x of raw) { a = (a + x) % 65521; b = (b + a) % 65521; }
  parts.push(u32((b << 16 | a) >>> 0));
  return concat(parts);
}

const PNG_SIGNATURE = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
function chunk(type, data) {
  const body = concat([Uint8Array.from(type, c => c.charCodeAt(0)), data]);
  return concat([u32(data.length), body, u32(crc32(body))]);
}
// A Node Buffer in the tests (they read it with Buffer methods), plain bytes in the browser.
const bytes = u8 => (typeof Buffer === 'function' ? Buffer.from(u8.buffer, u8.byteOffset, u8.length) : u8);

// One frame: an indexed PNG (palette + one fully transparent entry). Several: a GIF with one global palette.
function encode(art) {
  const pal = [...art.palette, [0, 0, 0]], tp = art.palette.length;
  if (art.frames.length > 1) {
    const gif = GIFEncoder();
    // gifenc writes the global table from the first frame's palette and a local table for any later frame given one
    art.frames.forEach((f, i) => gif.writeFrame(f.index, art.width, art.height,
      { palette: i ? undefined : pal, transparent: true, transparentIndex: tp, delay: f.delay, repeat: art.loop, dispose: 2 }));
    gif.finish();
    return { data: bytes(gif.bytes()), ext: 'gif' };
  }
  const { width: w, height: h } = art, idx = art.frames[0].index, raw = new Uint8Array((w + 1) * h);
  for (let y = 0; y < h; y++) raw.set(idx.subarray(y * w, (y + 1) * w), y * (w + 1) + 1); // filter byte 0 per row
  const ihdr = concat([u32(w), u32(h), new Uint8Array([8, 3, 0, 0, 0])]); // 8-bit indexed, no interlace
  const trns = new Uint8Array(pal.length).fill(255); trns[tp] = 0;
  const data = concat([PNG_SIGNATURE, chunk('IHDR', ihdr), chunk('PLTE', new Uint8Array(pal.flat())),
    chunk('tRNS', trns), chunk('IDAT', zlibStored(raw)), chunk('IEND', new Uint8Array(0))]);
  return { data: bytes(data), ext: 'png' };
}

const api = { DEFAULTS, MAX_FRAMES, BG_THRESHOLD, normalizeOpts, parseGif, decodeGif, fromRGBA, pixelate, encode };
if (typeof module === 'object' && module.exports) module.exports = api; else root.Pixelate = api;
})(typeof self !== 'undefined' ? self : this);
