// Pixel effects for the pets overlay (pets.html), drawn into one small canvas per pet while an effect plays.
// Technique from Sprite Fusion's MIT VFX by Hugo Duprez (MIT, see MIT-sprite-fusion-vfx.txt): a colour layer of hard pixels
// and a light layer, Bayer dithering instead of transparency, 7-step colour ramps, particles with drag and gravity,
// silhouette flashes and an aura grown from the sprite's outline. Grid: 1 cell = half a pixel of the 9×9 creature
// = 2 pixels of the fine 36×36 pet); the canvas is drawn at twice the grid so the pet keeps its fine pixels.
// A flat-icon pet (creature.js) is not drawn here: its own SVG stays on screen and keeps acting its state, and the
// effect draws round it (o.flat). Its silhouette still marks the cells it covers, so light and effect pixels stay off it.
// Needs creature.js (flatArt, bigHeadLook, packRGB).
const GW = 88, GH = 60, FEET = 54, CX = 44, SIM = 1 / 60, TOP = FEET - 18, MID = FEET - 9;
const XP_PER_LV = 3; // XP_PER_LV in src-tauri/src/state.rs: finished tasks per level

// ---- when effects play (pure: tested in test/core.test.js) ------------------------------------------------------
// The prop a pet holds for the tool its session is running: 0 pen and paper, 1 terminal, 2 book, 3 magnifier.
const TOOL_PROP = { Edit: 0, Write: 0, MultiEdit: 0, NotebookEdit: 0, Bash: 1, PowerShell: 1, Read: 2, WebFetch: 2, Grep: 3, Glob: 3, WebSearch: 3 };
const lowCtx = s => s.ctx != null && s.ctx < 25;
// The one-shot effects a change of a session starts. prev: the session as its pet last saw it, null for a pet just made.
function fxEvents(prev, s) {
  const out = [];
  if (prev && s.state === 'done' && prev.state !== 'done') out.push(['xp', { f0: (prev.xp || 0) / XP_PER_LV, f1: s.lv > prev.lv ? 1 : (s.xp || 0) / XP_PER_LV, lv: s.lv }]);
  if (lowCtx(s) && !(prev && lowCtx(prev))) out.push(['ctx']);
  return out;
}
// Queue a one-shot on a pet: one of each kind waits at most. A second XP keeps the first's starting bar.
function fxQueue(queue, playing, kind, o) {
  const i = queue.findIndex(e => e[0] === kind);
  if (i < 0) return [...queue, o ? [kind, o] : [kind]];
  return queue.map((e, j) => (j !== i ? e : kind === 'xp' ? [kind, { ...o, f0: e[1].f0 }] : o ? [kind, o] : [kind]));
}
// The effect that repeats while a state lasts: the aura while it waits for you, the tool's prop while it works.
function fxLoop(s, st) {
  if (st === 'waiting') return ['aura'];
  if (st === 'working' && TOOL_PROP[s.tool] != null) return ['tool', TOOL_PROP[s.tool]];
  return null;
}

// ---- colours ------------------------------------------------------------------------------------------------------
const hex = h => packRGB(parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)), ramp = a => a.map(hex);
const WARM = ramp(['#ffffff', '#fffbe2', '#fff0a6', '#ffd862', '#f6aa3c', '#cf722b', '#7e3c1f']);
const COOL = ramp(['#ffffff', '#dcfbff', '#a6ecff', '#62c8f4', '#3a8bd8', '#2c58aa', '#212f6b']);
const CORAL = ramp(['#ffffff', '#ffe4d6', '#ffb08a', '#ff7a4d', '#f04a2a', '#b8301f', '#6e1c16']);
const RED = ramp(['#ffffff', '#ffd9d9', '#ff8a8a', '#ff4d4d', '#e02828', '#a3161b', '#5e0d12']);
const GREEN = ramp(['#ffffff', '#e6ffdc', '#acff92', '#62e26a', '#30b55b', '#20804c', '#175237']);
const WHITE = hex('#ffffff'), INK = hex('#1a1422'), SHADOW = hex('#0b0814'), PAPER = hex('#fbf8f1'), LINE = hex('#9aa0b4'), SCREEN = hex('#161b2b');
const lum = c => ((c & 255) * .3 + ((c >>> 8) & 255) * .59 + ((c >>> 16) & 255) * .11) / 255;
const scale = (c, k) => ((c >>> 24) << 24 | Math.min(255, ((c >>> 16) & 255) * k) << 16 | Math.min(255, ((c >>> 8) & 255) * k) << 8 | Math.min(255, (c & 255) * k)) >>> 0;
const mix = (a, b, t) => { const ch = (c, s) => (c >>> s) & 255, m = s => Math.round(ch(a, s) * (1 - t) + ch(b, s) * t); return ((255 << 24) | (m(16) << 16) | (m(8) << 8) | m(0)) >>> 0; };

// ---- maths --------------------------------------------------------------------------------------------------------
const TAU = Math.PI * 2, clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const smooth = (a, b, x) => { const t = clamp((x - a) / (b - a)); return t * t * (3 - 2 * t); };
const outBack = (t, s = 1.7) => { const u = clamp(t) - 1; return 1 + (s + 1) * u * u * u + s * u * u; };
const outCubic = t => 1 - Math.pow(1 - clamp(t), 3), outExpo = t => (t >= 1 ? 1 : 1 - Math.pow(2, -10 * clamp(t)));
const gauss = (t, c, w) => Math.exp(-(((t - c) / w) ** 2));
const hash = (n, k = 0) => ((Math.imul(n + 1 + k * 7919, 2654435761) >>> 0) % 100000) / 100000;
class Rng {
  constructor(s = 1) { this.s = s >>> 0 || 1; }
  next() { let t = (this.s = (this.s + 0x6d2b79f5) >>> 0); t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }
  range(a, b) { return a + (b - a) * this.next(); } chance(p) { return this.next() < p; } pick(a) { return a[Math.floor(this.next() * a.length)]; } sign() { return this.next() < .5 ? -1 : 1; }
}
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
const bayer = (x, y) => (BAYER[((y & 3) << 2) | (x & 3)] + .5) / 16;
const dith = (a, x, y) => a >= 1 || a > bayer(x, y);

// ---- the frame ----------------------------------------------------------------------------------------------------
// A sprite here is { w, h, data, k }: k fine pixels per sprite pixel (1 for the 36×36 pet). It stands on the bottom
// of the pet's box, as the pet's art does.
class Buf {
  constructor() { this.c = new Uint32Array(GW * GH); this.l = new Float32Array(GW * GH); this.lc = new Uint32Array(GW * GH); this.m = new Uint8Array(GW * GH); this.f = new Uint8Array(GW * GH); this.pets = []; this.shake = [0, 0]; this.was = null; this.fx = this.fy = false; this.room = null; }
  clear() { this.c.fill(0); this.l.fill(0); this.m.fill(0); this.f.fill(0); this.pets.length = 0; this.shake = [0, 0]; }
  // room: [x0, y0, x1, y1] cells, the part of the grid this frame may draw in (the pet's own room on screen, see
  // fxRoom); nothing is drawn outside it. edge: how far a cell is from the room's edge; fits: a shape lies inside it.
  at(x, y) { x = Math.floor(x); y = Math.floor(y); const r = this.room; return x < 0 || y < 0 || x >= GW || y >= GH || (r && (x < r[0] || y < r[1] || x >= r[2] || y >= r[3])) ? -1 : y * GW + x; }
  fits(x0, y0, x1, y1) { const r = this.room; return !r || (x0 >= r[0] && y0 >= r[1] && x1 <= r[2] && y1 <= r[3]); }
  set(x, y, c) { const i = this.at(x, y); if (i >= 0) { this.c[i] = c; if (this.m[i]) this.f[i] = 1; } } // f: an effect pixel drawn over the pet
  glow(cx, cy, r, c, k) {
    const rm = this.room; if (rm) r = Math.min(r, cx - rm[0], rm[2] - cx, cy - rm[1], rm[3] - cy); // light shrinks to the room, never cut
    if (r < 1 || k <= .01) return;
    const l = this.l, lc = this.lc, x0 = Math.max(0, Math.floor(cx - r)), x1 = Math.min(GW - 1, Math.ceil(cx + r));
    for (let y = Math.max(0, Math.floor(cy - r)), y1 = Math.min(GH - 1, Math.ceil(cy + r)); y <= y1; y++) {
      const dy = (y + .5 - cy) / r, dy2 = dy * dy; if (dy2 >= 1) continue;
      for (let x = x0; x <= x1; x++) {
        const dx = (x + .5 - cx) / r, d = dx * dx + dy2; if (d >= 1) continue;
        const f = (1 - Math.sqrt(d)) ** 2 * k, i = y * GW + x; if (f > l[i]) { l[i] = f; lc[i] = c; }
      }
    }
  }
  rect(x, y, w, h, c) { for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) this.set(x + i, y + j, c); }
  frame(x, y, w, h, fill, line = INK) { this.rect(x, y, w, h, line); this.rect(x + 1, y + 1, w - 2, h - 2, fill); }
  disc(cx, cy, r, c) { for (let y = Math.floor(cy - r); y <= Math.ceil(cy + r); y++) for (let x = Math.floor(cx - r); x <= Math.ceil(cx + r); x++) { const dx = x + .5 - cx, dy = y + .5 - cy; if (dx * dx + dy * dy <= r * r) this.set(x, y, c); } }
  ring(cx, cy, r, th, c, a = 1) {
    const ro = r + th / 2, ri = Math.max(0, r - th / 2);
    for (let y = Math.floor(cy - ro); y <= Math.ceil(cy + ro); y++) for (let x = Math.floor(cx - ro); x <= Math.ceil(cx + ro); x++) { const dx = x + .5 - cx, dy = y + .5 - cy, d = dx * dx + dy * dy; if (d <= ro * ro && d >= ri * ri && dith(a, x, y)) this.set(x, y, c); }
  }
  line(x0, y0, x1, y1, c) {
    x0 = Math.round(x0); y0 = Math.round(y0); x1 = Math.round(x1); y1 = Math.round(y1);
    const dx = Math.abs(x1 - x0), dy = -Math.abs(y1 - y0), sx = x0 < x1 ? 1 : -1, sy = y0 < y1 ? 1 : -1; let e = dx + dy;
    for (let n = 0; n < 512; n++) { this.set(x0, y0, c); if (x0 === x1 && y0 === y1) break; const e2 = 2 * e; if (e2 >= dy) { e += dy; x0 += sx; } if (e2 <= dx) { e += dx; y0 += sy; } }
  }
  // Light is drawn as see-through pixels in a few steps, so it shows on any wallpaper over the transparent overlay.
  // Composed at twice the grid: effects keep their size, while the pets drawn this frame are painted again from their
  // fine pixels on top (effect pixels drawn over a pet stay in front of it). Only the part of the image that holds
  // something now or did last frame is written; returns that part [x, y, w, h] in image pixels, for putImageData.
  // fx / fy mirror the image left-right / top-down (a prop on the other side; the top edge of the screen): pets and
  // text are drawn into the grid mirrored already (blit, text, bmp), so they come out upright.
  toImage(img) {
    const out = this.out?.buffer === img.data.buffer ? this.out : (this.out = new Uint32Array(img.data.buffer)), W2 = GW * 2, H2 = GH * 2, c = this.c, l = this.l, fx = this.fx, fy = this.fy;
    let x0 = GW, y0 = GH, x1 = -1, y1 = -1;
    for (let y = 0, i = 0; y < GH; y++) for (let x = 0; x < GW; x++, i++) if (c[i] || l[i] > .08) { if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y; }
    const grow = (a, r) => (a ? [Math.min(a[0], r[0]), Math.min(a[1], r[1]), Math.max(a[2], r[2]), Math.max(a[3], r[3])] : r);
    let cur = x1 < 0 ? null : this.mirror([x0 * 2, y0 * 2, x1 * 2 + 2, y1 * 2 + 2]);
    for (const p of this.pets) { const r = this.petRect(p); if (r) cur = grow(cur, r); }
    const box = this.was ? (cur ? grow(this.was, cur) : this.was) : cur; // last frame's pixels are cleared too
    this.was = cur;
    if (!box) return [0, 0, 0, 0];
    for (let oy = box[1] >> 1; oy < box[3] >> 1; oy++) for (let ox = box[0] >> 1; ox < box[2] >> 1; ox++) {
      const i = (fy ? GH - 1 - oy : oy) * GW + (fx ? GW - 1 - ox : ox), ci = c[i], li = l[i];
      const v = this.m[i] === 2 && !this.f[i] ? 0 : ci >>> 24 && !(this.m[i] && !this.f[i]) ? ci : li > .08 ? ((li > .55 ? 200 : li > .32 ? 150 : li > .18 ? 100 : 55) << 24 | (this.lc[i] & 0xffffff)) >>> 0 : 0;
      const o = oy * 2 * W2 + ox * 2; out[o] = out[o + 1] = out[o + W2] = out[o + W2 + 1] = v;
    }
    for (const p of this.pets) {
      const spr = p.spr, SX = p.sx * spr.k, SY = p.sy * spr.k, hw = spr.w * SX / 2, h = spr.h * SY, keep = spr.h * p.clip;
      const [L, T, R, B] = this.mirror([p.cx * 2 - hw, p.feet * 2 - h, p.cx * 2 + hw, p.feet * 2]);
      for (let y = Math.max(0, Math.floor(T)); y < Math.min(H2, Math.ceil(B)); y++) for (let x = Math.max(0, Math.floor(L)); x < Math.min(W2, Math.ceil(R)); x++) {
        if (this.f[((fy ? H2 - 1 - y : y) >> 1) * GW + ((fx ? W2 - 1 - x : x) >> 1)]) continue;
        const u = Math.floor((x + .5 - L) / SX), v = Math.floor((y + .5 - T) / SY);
        if (u < 0 || v < 0 || u >= spr.w || v >= spr.h || v >= keep) continue;
        let c = spr.data[v * spr.w + u];
        if (!(c >>> 24) || !dith(p.a, x, y)) continue;
        if (p.flash && p.fa > 0 && dith(p.fa, x, y)) { const l = lum(c); c = l > .5 ? (p.fa > .75 ? p.flash[0] : p.flash[1]) : l > .22 ? p.flash[2] : p.flash[4]; }
        out[y * W2 + x] = c;
      }
    }
    return [box[0], box[1], box[2] - box[0], box[3] - box[1]];
  }
  // A rect [l, t, r, b] of image pixels as it lands in the (mirrored) image.
  mirror([l, t, r, b]) { const W2 = GW * 2, H2 = GH * 2; return [this.fx ? W2 - r : l, this.fy ? H2 - b : t, this.fx ? W2 - l : r, this.fy ? H2 - t : b]; }
  // The image pixels a pet drawn this frame can cover, cell-aligned and inside the image, or null.
  petRect(p) {
    const spr = p.spr, hw = spr.w * p.sx * spr.k / 2, h = spr.h * p.sy * spr.k, [l, t, rr, b] = this.mirror([p.cx * 2 - hw, p.feet * 2 - h, p.cx * 2 + hw, p.feet * 2]);
    const r = [Math.max(0, Math.floor(l / 2) * 2), Math.max(0, Math.floor(t / 2) * 2), Math.min(GW * 2, Math.ceil(rr / 2) * 2), Math.min(GH * 2, Math.ceil(b / 2) * 2)];
    return r[2] > r[0] && r[3] > r[1] ? r : null;
  }
}
const dot = (b, x, y, c, a = 1) => { const px = Math.floor(x), py = Math.floor(y); if (dith(a, px, py)) b.set(px, py, c); };
function star(b, x, y, arm, c, glow = .5) {
  const px = Math.floor(x), py = Math.floor(y);
  for (let d = 1; d <= arm; d++) { const cc = d === arm && arm > 1 ? scale(c, .8) : c; b.set(px + d, py, cc); b.set(px - d, py, cc); b.set(px, py + d, cc); b.set(px, py - d, cc); }
  b.set(px, py, arm > 0 ? WHITE : c); b.glow(x, y, 3 + arm * 2.2, c, glow);
}
function flare(b, x, y, L, ang, c1, c2) {
  if (L < .8) return;
  for (let i = 0; i < 8; i++) {
    const diag = i % 2 === 1, an = ang + i * TAU / 8, len = diag ? L * .42 : L, ca = Math.cos(an), sa = Math.sin(an);
    for (let d = 0; d <= len; d += .5) {
      const u = d / len, c = diag ? (u < .5 ? c1 : c2) : u < .3 ? WHITE : u < .65 ? c1 : c2; dot(b, x + ca * d, y + sa * d, c, u < .7 ? 1 : 1 - (u - .7) / .35);
      if (!diag && u < .22 && L > 4) { dot(b, x + ca * d - sa, y + sa * d + ca, c1); dot(b, x + ca * d + sa, y + sa * d - ca, c1); }
    }
  }
  dot(b, x, y, WHITE); b.glow(x, y, 4 + L * 1.3, c1, .55);
}
// A 3×5 pixel font with a dark outline, so numbers read on light and dark wallpapers.
const FONT = {
  '+': ['...', '.X.', 'XXX', '.X.', '...'], 0: ['XXX', 'X.X', 'X.X', 'X.X', 'XXX'], 1: ['.X.', 'XX.', '.X.', '.X.', 'XXX'], 2: ['XXX', '..X', 'XXX', 'X..', 'XXX'],
  3: ['XXX', '..X', 'XXX', '..X', 'XXX'], 4: ['X.X', 'X.X', 'XXX', '..X', '..X'], 5: ['XXX', 'X..', 'XXX', '..X', 'XXX'], 6: ['XXX', 'X..', 'XXX', 'X.X', 'XXX'],
  7: ['XXX', '..X', '..X', '.X.', '.X.'], 8: ['XXX', 'X.X', 'XXX', 'X.X', 'XXX'], 9: ['XXX', 'X.X', 'XXX', '..X', 'XXX'],
  L: ['X..', 'X..', 'X..', 'X..', 'XXX'], V: ['X...X', 'X...X', '.X.X.', '.X.X.', '..X..'], X: ['X.X', 'X.X', '.X.', 'X.X', 'X.X'], P: ['XX.', 'X.X', 'XX.', 'X..', 'X..'], ' ': ['.', '.', '.', '.', '.'], '!': ['X', 'X', 'X', '.', 'X'],
};
const textW = (s, k = 1) => ([...s].reduce((w, ch) => w + (FONT[ch] || FONT[' '])[0].length + 1, 0) - 1) * k;
// Whether a text fits the room whole (with its outline).
const textFits = (b, s, cx, top, k = 1) => { const w = textW(s, k), x0 = Math.round(cx - w / 2); return b.fits(x0 - 1, top - 1, x0 + w + 1, top + 5 * k + 1); };
function text(b, s, cx, top, fill, k = 1, a = 1) {
  const cells = []; let x = 0;
  for (const ch of s) { const g = FONT[ch] || FONT[' ']; g.forEach((row, j) => [...row].forEach((v, i) => v === 'X' && cells.push([x + i, j]))); x += g[0].length + 1; }
  const w = (x - 1) * k, x0 = Math.round(cx - w / 2), on = new Set(), lit = new Set(); // lit: the glyphs' top row, a shade lighter
  for (const [i, j] of cells) for (let u = 0; u < k; u++) for (let v = 0; v < k; v++) { const key = `${x0 + i * k + u},${top + (b.fy ? 4 - j : j) * k + (b.fy ? k - 1 - v : v)}`; on.add(key); if (j === 0) lit.add(key); }
  for (const key of on) { const [px, py] = key.split(',').map(Number); for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, -1], [1, -1], [-1, 1]]) if (!on.has(`${px + dx},${py + dy}`)) dot(b, px + dx, py + dy, INK, a); }
  for (const key of on) { const [px, py] = key.split(',').map(Number); dot(b, px, py, lit.has(key) ? scale(fill, 1.15) : fill, a); }
}
// Small bitmaps: o = outline, digits index a ramp, W = white.
function bmp(b, rows, x0, y0, r, a = 1) { rows.forEach((row, j) => [...row].forEach((ch, i) => { if (ch === '.') return; dot(b, x0 + i, y0 + (b.fy ? rows.length - 1 - j : j), ch === 'o' ? INK : ch === 'W' ? WHITE : r[+ch], a); })); }

// Draw a pet standing on (cx, feet) cells, stretched by sx/sy, optionally flashed toward a ramp (by brightness, like
// power-up's silhouette flash). The cells it covers are marked so the aura can grow from them; its fine pixels are
// painted by toImage. o.a: see-through (dithered), o.clip: the share of its height still drawn, from the top.
function blit(b, spr, cx, feet, sx = 1, sy = 1, flash = null, fa = 0, o = {}) {
  if (b.flat) return blitFlat(b, spr, cx, feet); // stays where its SVG stands: no hop, stretch or flash
  const SX = sx * spr.k / 2, SY = sy * spr.k / 2, hw = spr.w * SX / 2, h = spr.h * SY, top = feet - h, a = o.a ?? 1, clip = o.clip ?? 1;
  for (let y = Math.floor(top); y < Math.ceil(feet); y++) for (let x = Math.floor(cx - hw); x < Math.ceil(cx + hw); x++) {
    let u = Math.floor((x + .5 - cx) / SX + spr.w / 2), v = Math.floor((y + .5 - top) / SY);
    if (u < 0 || v < 0 || u >= spr.w || v >= spr.h) continue;
    if (b.fx) u = spr.w - 1 - u; if (b.fy) v = spr.h - 1 - v; // mirrored here, upright in the image
    if (v >= spr.h * clip) continue;
    const c = spr.data[v * spr.w + u]; if (!(c >>> 24) || !dith(a, x, y)) continue;
    b.set(x, y, c); const i = b.at(x, y); if (i >= 0) b.m[i] = 1;
  }
  b.pets.push({ spr, cx, feet, sx, sy, flash, fa, a, clip });
}
// A flat pet: only the cells under it are marked (2: left empty unless an effect pixel is drawn over it).
function blitFlat(b, spr, cx, feet) {
  const S = spr.k / 2, hw = spr.w * S / 2, top = feet - spr.h * S;
  for (let y = Math.floor(top); y < Math.ceil(feet); y++) for (let x = Math.floor(cx - hw); x < Math.ceil(cx + hw); x++) {
    let u = Math.floor((x + .5 - cx) / S + spr.w / 2), v = Math.floor((y + .5 - top) / S);
    if (u < 0 || v < 0 || u >= spr.w || v >= spr.h) continue;
    if (b.fx) u = spr.w - 1 - u; if (b.fy) v = spr.h - 1 - v;
    const i = b.at(x, y); if (i >= 0 && spr.data[v * spr.w + u] >>> 24) b.m[i] = 2;
  }
}
const shadow = (b, cx, feet, w = 9) => { if (b.fy || b.flat) return; /* no ground under a pet hanging from the top edge; a flat pet draws its own */ for (const [row, ww, k] of [[feet, w, .3], [feet + 1, w * .7, .2], [feet + 2, w * .4, .12]]) for (let x = Math.ceil(-ww); x <= ww; x++) { const i = b.at(cx + x, row); if (i >= 0 && !b.c[i] && b.l[i] < k) { b.l[i] = k; b.lc[i] = SHADOW; } } };
function hopPose(t, t0, h = 4, dur = .3) {
  if (t >= t0 - .09 && t < t0) return { dy: 0, sx: 1.18, sy: .82 };
  if (t >= t0 && t < t0 + dur) { const p = (t - t0) / dur, mid = p > .3 && p < .7; return { dy: -Math.round(h * Math.sin(Math.PI * p)), sx: mid ? 1 : .88, sy: mid ? 1 : 1.14 }; }
  if (t >= t0 + dur && t < t0 + dur + .1) return { dy: 0, sx: 1.22, sy: .8 };
  return { dy: 0, sx: 1, sy: 1 };
}
function shock(b, cx, fy, t, z, L) {
  const R = 70 * z, q = t / .55; if (q < 0 || q >= 1.8) return;
  if (q < .5) for (let dx = -Math.round(14 * z); dx <= 14 * z; dx++) { const u = Math.abs(dx) / (14 * z), a = (1 - q / .5) * (1 - u) * 2.2; dot(b, cx + dx, fy, u < .3 ? WHITE : u < .6 ? L[1] : L[2], a); dot(b, cx + dx, fy - 1, u < .4 ? L[1] : L[2], a * .7); }
  const reach = 4 + R * outExpo(Math.min(1, q)), after = 1 - clamp((q - .25) / 1.4);
  for (let dx = -Math.round(reach); dx <= reach; dx++) { const u = Math.abs(dx) / reach; dot(b, cx + dx, fy, u < .5 ? L[2] : L[3], after * .8 * (1 - u * .5)); }
  if (q < 1) {
    const head = 4 + R * outExpo(q), len = (6 + 22 * (1 - q)) * z, crest = 6 * z * (1 - q) ** .7;
    for (const sd of [-1, 1]) {
      for (let j = 0; j < len; j++) {
        const x = cx + sd * (head - j), a = (1 - j / len) * (1 - q) * 2.2, lift = Math.round(crest * Math.max(0, 1 - j / (len * .5)));
        dot(b, x, fy, j < 2 ? WHITE : j < len * .35 ? L[1] : j < len * .7 ? L[2] : L[3], a);
        for (let r = 1; r <= lift; r++) dot(b, x, fy - r, j < 2 && r > lift - 2 ? WHITE : r === lift ? L[1] : L[2], a * (1 - r / (lift + 2)) * 1.4);
      }
      for (let r = 0; r < crest + 3; r++) dot(b, cx + sd * head, fy - r, WHITE, (1 - q) * 1.6 * (1 - r / (crest + 3))); b.glow(cx + sd * head, fy - 3, 20 * z, L[2], .8 * (1 - q));
    }
  }
}
// An outline round the pet: 1 cell always, a 2nd and 3rd as it pulses (dithered), all inside its own slot.
function halo(b, k, L) {
  let X0 = GW, Y0 = GH, X1 = -1, Y1 = -1;
  for (let y = 0, i = 0; y < GH; y++) for (let x = 0; x < GW; x++, i++) if (b.m[i]) { if (x < X0) X0 = x; if (x > X1) X1 = x; if (y < Y0) Y0 = y; if (y > Y1) Y1 = y; }
  if (X1 < 0) return;
  for (let y = Math.max(0, Y0 - 3); y <= Math.min(GH - 1, Y1 + 1); y++) for (let x = Math.max(0, X0 - 3); x <= Math.min(GW - 1, X1 + 3); x++) {
    const i = y * GW + x; if (b.m[i] || b.c[i]) continue;
    let d = 9; for (let dy = -3; dy <= 3; dy++) for (let dx = -3; dx <= 3; dx++) { const j = b.at(x + dx, y + dy); if (j >= 0 && b.m[j]) d = Math.min(d, Math.max(Math.abs(dx), Math.abs(dy))); }
    if (d === 1) b.set(x, y, k > .6 ? L[1] : L[2]); else if (d === 2 && dith(k * 1.2, x, y)) b.set(x, y, L[3]); else if (d === 3 && dith(k * .7 - .1, x, y)) b.set(x, y, L[4]);
  }
}
function stepList(list, dt, drag = 0, grav = 0) { for (let i = list.length - 1; i >= 0; i--) { const p = list[i]; if ((p.life -= dt) <= 0) { list.splice(i, 1); continue; } p.vx -= p.vx * drag * dt; p.vy -= p.vy * drag * dt; p.vy += grav * dt; p.x += p.vx * dt; p.y += p.vy * dt; if (p.grow) p.s += p.grow * dt; } }
const shake = (b, t, t0, dur, amp = 1) => { if (t >= t0 && t < t0 + dur) { const k = Math.floor(t * 40); b.shake = [(hash(k, 1) < .5 ? -1 : 1) * amp, (hash(k, 2) < .5 ? -1 : 1) * amp]; } };
// A tool as a small badge (11 x 11 cells): a paper disc with the tool drawn on it; the terminal's cursor blinks.
const BADGE = ['..ooooooo..', '.o2222222o.', ...Array(7).fill('o222222222o'), '.o2222222o.', '..ooooooo..'];
const BADGE_ICON = [
  ['......o', '....o4o', '...o4o.', '..o4o..', '.o4o...', 'o5o....', '7......'],
  ['0000000', '0100000', '0010000', '0100110', '0000000', '...0...', '..000..'],
  ['o22o22o', 'o33o33o', 'o22o22o', 'o33o33o', 'o22o22o', 'o33o33o', 'ooooooo'],
  ['.ooo...', 'o666o..', 'o666o..', 'o666o..', '.ooo5..', '....55.', '.....55'],
];
function toolBadge(b, seg, x0, y0, t) {
  const pal = [SCREEN, GREEN[3], PAPER, LINE, WARM[3], hex('#7a5a3a'), hex('#bfe6ff'), hex('#3a3f6b')];
  bmp(b, BADGE, x0, y0, pal);
  const icon = BADGE_ICON[seg].map((r, j) => (seg === 1 && j === 3 && Math.floor(t * 4 + 1e-6) % 2 ? r.slice(0, 4) + '000' : r));
  bmp(b, icon, x0 + 2, y0 + 2, pal);
}
// The tool as a small card held in the pet's hands (9 x 9 cells, inside its own box); the terminal's cursor blinks.
function toolMini(b, seg, x0, y0, t) {
  const pal = [SCREEN, GREEN[3], PAPER, LINE, WARM[3], hex('#7a5a3a'), hex('#bfe6ff'), hex('#3a3f6b')];
  b.frame(x0, y0, 9, 9, seg === 1 ? SCREEN : PAPER);
  const icon = BADGE_ICON[seg].map((r, j) => (seg === 1 && j === 3 && Math.floor(t * 4 + 1e-6) % 2 ? r.slice(0, 4) + '000' : r));
  bmp(b, icon, x0 + 1, y0 + 1, pal);
}
function pencilTip(lt, PX0, PY0) { const u = Math.min(1, lt / 1.1), row = Math.floor(u * 3); return [PX0 + 3 + (u * 3 % 1) * 14, PY0 + 7 + row * 3 + Math.sin(u * 60) * .9]; }
function pencil(b, x, y) {
  x = Math.round(x); y = Math.round(y);
  for (let i = 0; i < 11; i++) { const px = x + 1 + i, py = y - 1 - i, c = i < 2 ? hex('#e8c9a0') : i < 8 ? WARM[3] : i < 9 ? hex('#c0c4cc') : hex('#ff8fa3'); b.set(px - 1, py, INK); b.set(px + 2, py, INK); b.set(px, py, c); b.set(px + 1, py, i < 2 ? hex('#d1ae80') : i < 8 ? WARM[4] : i < 9 ? hex('#9aa0aa') : hex('#e06c84')); }
  b.set(x, y, INK); b.set(x + 1, y, INK); b.set(x + 12, y - 12, INK); b.set(x + 13, y - 12, INK);
}

// ---- the effects --------------------------------------------------------------------------------------------------
// Each: D seconds long, ox the grid column the pet stands on (the canvas is placed so that column is the pet's middle),
// make(o) a fresh run. o.pet(st) gives sprites. Timing, colours and look follow the approved mockup.
const FX = {
  // Waiting for your approval: a tight orange outline that hugs the pet and pulses, never past its own slot.
  aura: {
    D: 2.4, ox: CX, loop: true, cycle: 2.4,
    make(o) {
      return {
        update() {},
        draw(b, t) {
          const pulse = .5 + .5 * Math.sin(t / 1.2 * TAU - Math.PI / 2);
          blit(b, o.pet('waiting'), CX, FEET, 1, 1, CORAL, .35 * pulse);
          halo(b, pulse, CORAL);
          b.glow(CX, MID, 13, CORAL[3], .12 + .22 * pulse);
        },
      };
    },
  },
  // A task finished: the pet hops; three XP orbs fly into the XP bar, each moving it on. A full bar: a beat's pause, a
  // gold light column comes down, a ground wave, a little shake and "LV n". (Holy Light / Pickup.)
  // o.f0 / o.f1: the bar before and after (1 = a new level), o.lv: the level now.
  xp: {
    D: 3.8, ox: CX,
    make(o) {
      const R = new Rng(31), orbs = [], sp = [], pops = [], up = o.f1 >= 1, step = (o.f1 - o.f0) / 3;
      let fill = o.f0, shown = o.f0, full = -1, lvl = -1, burst = false;
      const LIFT = o.flipY ? 9 : 0, BX = CX - 8, BY = TOP - 10 - LIFT, BW = 16; // a bar as wide as the pet; LIFT: mirrored, clear of the name tag under the pet
      for (let i = 0; i < 3; i++) orbs.push({ t0: .35 + i * .2, x: CX, y: MID, done: false, tr: [] });
      return {
        D: up ? 3.8 : 2.1,
        update(dt, t) {
          if (lvl > 0 && t > lvl && t < lvl + .08) return;
          for (const ob of orbs) {
            if (t < ob.t0 || ob.done) continue;
            const k = clamp((t - ob.t0) / .38), u = Math.pow(k, 1.8), sx = CX + (ob.t0 * 37 % 9 - 4), sy = MID - 2, tx = BX + 1 + (BW - 2) * Math.min(1, fill + .02), ty = BY + 2, mx = (sx + tx) / 2 + 12 * (ob.t0 > .5 ? 1 : -1), my = Math.min(sy, ty) - 8;
            ob.x = (1 - u) ** 2 * sx + 2 * (1 - u) * u * mx + u * u * tx; ob.y = (1 - u) ** 2 * sy + 2 * (1 - u) * u * my + u * u * ty; ob.tr.unshift([ob.x, ob.y]); if (ob.tr.length > 6) ob.tr.pop();
            if (k >= 1) {
              ob.done = true; fill = Math.min(1, fill + step); pops.push({ x: tx + 4, y: BY - 9, t });
              for (let i = 0; i < 5; i++) sp.push({ x: tx, y: ty, vx: R.range(-18, 18), vy: -R.range(8, 26), life: R.range(.3, .5), max: .5, s: 0, c: WARM[R.chance(.5) ? 1 : 2] });
              if (up && orbs.every(x => x.done) && full < 0) { fill = 1; full = t; lvl = t + .22; }
            }
          }
          shown += (fill - shown) * Math.min(1, dt * 14);
          if (lvl > 0 && t >= lvl + .08 && !burst) { burst = true; for (let i = 0; i < 22; i++) { const a = R.range(-Math.PI, 0), v = R.range(20, 55); sp.push({ x: CX + R.range(-6, 6), y: FEET - R.range(0, 6), vx: Math.cos(a) * v, vy: Math.sin(a) * v, life: R.range(.6, 1.1), max: 1.1, s: R.chance(.4) ? 1 : 0, c: R.pick(WARM.slice(0, 4)) }); } }
          stepList(sp, dt, 1.6, 22);
        },
        draw(b, t) {
          const T = lvl > 0 ? t - lvl - .08 : -1, p = hopPose(t, .12, 6, .3), q = T >= 0 ? hopPose(T, .05, 10, .38) : p;
          shadow(b, CX, FEET);
          if (T >= 0 && T < .9) {
            const w = 9 * (1 - smooth(.2, .9, T)), bot = T < .06 ? FEET * (T / .06) : FEET;
            for (let y = 0; y < bot; y++) for (let x = -Math.ceil(w); x <= w; x++) { const u = Math.abs(x) / Math.max(1, w); dot(b, CX + x, y, u < .25 ? WHITE : u < .55 ? WARM[2] : WARM[3], (1 - u * .6) * (1 - smooth(.3, .9, T)) * 1.3); }
            b.glow(CX, FEET - 10, 30, WARM[3], .7 * (1 - T / .9));
          }
          const flash = T >= 0 ? gauss(T, .03, .08) * 1.6 : gauss(t, .45, .06) * .8;
          blit(b, o.pet('done'), CX, FEET + q.dy, q.sx, q.sy, WARM, flash);
          if (T >= 0) { shock(b, CX, FEET, T, .32, WARM); shake(b, T, 0, .25, 1); }
          const barA = T >= 0 ? 1 - smooth(.1, .5, T) : up ? 1 : 1 - smooth(1.6, 2.05, t);
          // The bar, its label, the "+1"s and "LV n" are drawn whole or not at all: where the room is too tight (a packed
          // row, the top edge over the name tag) the pet just hops and flashes and the orbs fly off.
          if (barA > .05 && b.fits(BX, BY, BX + BW, BY + 5)) {
            const fw = Math.round((BW - 2) * shown);
            for (let j = 0; j < 5; j++) for (let i = 0; i < BW; i++) { const edge = j === 0 || j === 4 || i === 0 || i === BW - 1, inFill = i - 1 < fw; dot(b, BX + i, BY + j, edge ? INK : inFill ? (j === 1 ? WARM[1] : j === 3 ? WARM[4] : WARM[3]) : hex('#3a3550'), barA); }
            if (textFits(b, 'XP', BX - 6, BY)) text(b, 'XP', BX - 6, BY, WARM[3], 1, barA); if (full > 0 && t - full < .2) b.glow(BX + BW / 2, BY + 2, 16, WARM[2], .8);
          }
          for (const ob of orbs) { if (t < ob.t0 || ob.done) continue; ob.tr.forEach(([x, y], i) => i > 0 && b.line(ob.tr[i - 1][0], ob.tr[i - 1][1], x, y, i < 3 ? WARM[2] : WARM[4])); star(b, ob.x, ob.y, 1, WARM[2], .6); }
          for (const pp of pops) { const k = (t - pp.t) / .7, y = Math.round(pp.y - 6 * outCubic(k)); if (k < 1 && textFits(b, '+1', pp.x, y)) text(b, '+1', pp.x, y, WARM[3], 1, 1 - smooth(.6, 1, k)); }
          for (const s of sp) { const k = s.life / s.max; star(b, s.x, s.y, Math.round(s.s * Math.min(1, k * 3)), s.c); }
          if (T >= .1) {
            const k = outBack(clamp((T - .1) / .25), 2.5), a = 1 - smooth(2.2, 2.9, T), s = `LV ${o.lv}`, top = Math.round(TOP - 18 - LIFT + (1 - k) * 6);
            const z = [k > .6 ? 2 : 1, 1].find(z => textFits(b, s, CX, top, z)); if (z) text(b, s, CX, top, WARM[3], z, a);
          }
        },
      };
    },
  },
  // Working: the pet holds the prop of the tool Claude is running (o.seg, see TOOL_PROP): writes with a pen on paper,
  // types on a small terminal, turns a book's page, sweeps a magnifier. One run is one 1.2 s loop; o.first pops it in.
// o.badge: no room beside the pet (a packed row): a small badge of the tool over its head instead, inside its own slot.
  tool: {
    D: 1.2, ox: o => (o.badge || o.plain || o.mini ? CX : CX - 8), loop: true, cycle: 6, // the pet's bob (1/3 s) and the cursor (1/2 s) repeat with the loop every 6 s
    make(o) {
      const R = new Rng(41), sp = [], seg = o.seg, PCX = o.badge || o.plain || o.mini ? CX : CX - 8, PX0 = CX + 4, PY0 = FEET - 20;
      return {
        update(dt, lt) { if (seg === 0 && !o.badge && R.chance(.4)) { const tip = pencilTip(lt, PX0, PY0); sp.push({ x: tip[0], y: tip[1], vx: R.range(-10, 10), vy: -R.range(5, 15), life: .3, max: .3, c: WARM[2] }); } stepList(sp, dt, 2, 20); },
        draw(b, lt, t) {
          const bob = Math.floor(t * 6 + 1e-6) % 2; // + 1e-6: whole steps land on the same side every cycle
          shadow(b, PCX, FEET); blit(b, o.pet('working'), PCX, FEET - bob);
          if (o.plain) return; // no room for a prop at all
          if (o.mini) { toolMini(b, seg, CX, FEET - 9 - bob, t); return; } // held in its hands, inside its box
          if (o.badge) { if (b.fits(CX - 5, FEET - 31 - bob, CX + 6, FEET - 20 - bob)) toolBadge(b, seg, CX - 5, FEET - 31 - bob, t); return; }
          if (o.first && lt < .1) for (let i = 0; i < 8; i++) { const a = i / 8 * TAU, r = 4 + lt / .1 * 8; dot(b, PX0 + 10 + Math.cos(a) * r, PY0 + 10 + Math.sin(a) * r * .7, WHITE, 1 - lt / .1); }
          if (seg === 0) {
            b.frame(PX0, PY0 + 4, 20, 15, PAPER); for (let r = 0; r < 3; r++) b.rect(PX0 + 3, PY0 + 7 + r * 3, 14, 1, hex('#e3e0d8'));
            for (let s = 0; s < 1; s += .01) { const u = s * Math.min(1, lt / 1.1), row = Math.floor(u * 3); b.set(PX0 + 3 + (u * 3 % 1) * 14, PY0 + 7 + row * 3 + Math.sin(u * 60) * .9, hex('#3a3f6b')); }
            const tip = pencilTip(lt, PX0, PY0); pencil(b, tip[0], tip[1]); for (const s of sp) star(b, s.x, s.y, 0, s.c, .4);
          }
          if (seg === 1) {
            b.frame(PX0 - 1, PY0 + 1, 24, 18, SCREEN); b.rect(PX0 + 7, PY0 + 19, 8, 2, INK); b.rect(PX0 + 4, PY0 + 21, 14, 1, INK);
            const G = GREEN[3], lines = [10, 14, 7], typed = lt / 1.05 * 31; let left = typed;
            lines.forEach((n, r) => { const k = Math.max(0, Math.min(n, left)); left -= n; for (let i = 0; i < k; i++) if ((i * 7 + r) % 5 !== 3) b.set(PX0 + 2 + i, PY0 + 4 + r * 4, r === 0 && i < 2 ? WARM[3] : G); });
            const row = typed < 10 ? 0 : typed < 24 ? 1 : 2, col = Math.min(lines[row], Math.max(0, typed - [0, 10, 24][row])); if (lt < 1.05 && Math.floor(t * 4 + 1e-6) % 2) b.rect(PX0 + 2 + col, PY0 + 3 + row * 4, 2, 3, G);
            if (lt > 1.05) { bmp(b, ['.......3', '......3.', '3....3..', '.3..3...', '..33....'], PX0 + 7, PY0 + 7, GREEN); b.glow(PX0 + 11, PY0 + 9, 14, GREEN[3], .6); } b.glow(PX0 + 11, PY0 + 9, 16, GREEN[3], .25);
          }
          if (seg === 2) {
            const BXk = PX0 - 2, BYk = PY0 + 6; b.frame(BXk, BYk, 12, 13, PAPER); b.frame(BXk + 11, BYk, 12, 13, PAPER); b.rect(BXk + 11, BYk + 1, 1, 11, hex('#c9c2b4'));
            for (let r = 0; r < 4; r++) { b.rect(BXk + 2, BYk + 3 + r * 2, 8, 1, LINE); b.rect(BXk + 13, BYk + 3 + r * 2, 8, 1, LINE); }
            const f = clamp((lt - .35) / .45);
            if (f > 0 && f < 1) {
              const w = Math.round(11 * Math.cos(f * Math.PI)), aw = Math.abs(w), lift = Math.round(Math.sin(f * Math.PI) * 3), x0 = w >= 0 ? BXk + 11 : BXk + 11 + w;
              for (let j = 0; j < 13; j++) for (let i = 0; i < aw; i++) { const edge = (w >= 0 ? i === aw - 1 : i === 0) || j === 0 || j === 12; b.set(x0 + i, BYk - lift + j, edge ? INK : w >= 0 ? PAPER : hex('#efe9dc')); }
            }
            b.rect(BXk + 1, BYk + 13, 21, 1, hex('#7a5a3a'));
          }
          if (seg === 3) {
            const sx = PX0 + 10 + Math.sin(lt * 4.5) * 9, sy = PY0 + 8, hit = lt > .55 && lt < .9;
            for (let r = 0; r < 4; r++) b.rect(PX0 - 1, PY0 + 4 + r * 4, 24, 1, LINE); b.rect(PX0 + 9, PY0 + 8, 6, 1, hit ? WARM[4] : LINE);
            for (let i = 0; i < 5; i++) { b.set(sx + 3 + i, sy + 3 + i, INK); b.set(sx + 4 + i, sy + 3 + i, hex('#7a5a3a')); b.set(sx + 5 + i, sy + 3 + i, INK); }
            b.ring(sx, sy, 4.6, 1.6, INK); b.disc(sx, sy, 3.6, hex('#bfe6ff')); dot(b, sx - 2, sy - 2, WHITE); dot(b, sx - 1, sy - 2, WHITE); dot(b, sx - 2, sy - 1, WHITE);
            if (hit) flare(b, PX0 + 12, PY0 + 6, 4 + 2 * Math.sin((lt - .55) / .35 * Math.PI), 0, WARM[2], WARM[3]);
          }
        },
      };
    },
  },
  // Context under 25%: a battery over its head is down to one blinking red bar; the pet shivers, sags a little, and
  // sweat drops run off its head and splash on the ground.
  ctx: {
    D: 2.4, ox: CX,
    make(o) {
      const R = new Rng(51), drops = [], splash = [];
      for (const [side, t0] of [[1, .25], [-1, 1.3]]) drops.push({ t0, side, x: CX + side * 7, y: TOP + 3, vy: 0, done: false });
      return {
        update(dt, t) {
          for (const d of drops) {
            if (t < d.t0 || d.done) continue; const k = t - d.t0;
            if (k < .4) { d.y = TOP + 3 + k * 14; d.x = CX + d.side * (7 + k * 6); } else { d.vy += 120 * dt; d.y += d.vy * dt; d.x += d.side * 8 * dt; }
            if (d.y >= FEET - 2) { d.done = true; for (let i = 0; i < 5; i++) splash.push({ x: d.x, y: FEET - 1, vx: R.range(-16, 16), vy: -R.range(10, 26), life: R.range(.25, .4), max: .4 }); }
          }
          stepList(splash, dt, 1, 120);
        },
        draw(b, t) {
          const jit = Math.floor(t * 18) % 2 ? 1 : 0; shadow(b, CX, FEET); blit(b, o.pet('working'), CX + jit, FEET, 1.04, .94);
          const X0 = CX - 8, Y0 = TOP - 13 - (o.flipY ? 9 : 0), on = (t % .5) < .28; // the battery whole or not at all (no room: the pet shivers and sweats)
          if (b.fits(X0, Y0, X0 + 17, Y0 + 9)) {
            b.frame(X0, Y0, 15, 9, hex('#2a2f3e')); b.rect(X0 + 15, Y0 + 3, 2, 3, INK);
            b.rect(X0 + 2, Y0 + 2, 3, 5, on ? RED[3] : RED[5]); if (on) { b.rect(X0 + 2, Y0 + 2, 3, 1, RED[2]); b.glow(X0 + 3, Y0 + 4, 9, RED[3], .6); }
            if (textFits(b, '!', X0 + 21, Y0 + 2)) text(b, '!', X0 + 21, Y0 + 2, RED[3], 1, on ? 1 : .4);
          }
          for (const d of drops) if (t >= d.t0 && !d.done) bmp(b, ['..o..', '.o2o.', 'o122o', 'o1W3o', 'o233o', '.ooo.'], Math.round(d.x) - 2, Math.round(d.y) - 3, COOL);
          for (const s of splash) dot(b, s.x, s.y, s.life / s.max > .5 ? COOL[2] : COOL[3]);
        },
      };
    },
  },
  // The session ended: the pet waves, lights up, then melts from the feet up into rising sparks; a thin light column
  // flares and fades, leaving one twinkle. (Blink.)
  end: {
    D: 2.6, ox: CX,
    make(o) {
      const R = new Rng(71), parts = [], S = .7, E = 1.4, spr = o.pet('done'), rows = Math.ceil(spr.h * spr.k / 2); let made = 0;
      // The cells of one row of the pet (from the top), with the colour of the fine pixel at each cell's middle.
      const row = r => { const out = [], w = Math.ceil(spr.w * spr.k / 2), top = FEET - spr.h * spr.k / 2;
        for (let i = 0; i < w; i++) { const u = Math.floor((i + .5) * 2 / spr.k), v = Math.floor((r + .5) * 2 / spr.k), c = u < spr.w && v < spr.h ? spr.data[v * spr.w + u] : 0; if (c >>> 24) out.push([CX - w / 2 + i, top + r, c]); }
        return out; };
      return {
        update(dt, t) {
          if (t >= S && t < E) { const cut = Math.floor(rows * (1 - (t - S) / (E - S))); while (made < rows - cut) { for (const [x, y, c] of row(rows - 1 - made)) parts.push({ x, y, vx: R.range(-6, 6), vy: -R.range(18, 40), life: R.range(.6, 1.1), max: 1.1, c }); made++; } }
          stepList(parts, dt, .8, -14);
        },
        draw(b, t) {
          const wave = t < .5 && Math.floor(t * 8) % 2 ? 1.06 : 1;
          if (t < E) { const cut = t < S ? rows : Math.floor(rows * (1 - (t - S) / (E - S))); shadow(b, CX, FEET, 9 * cut / rows); blit(b, spr, CX, FEET, wave, 1, COOL, t > .4 ? smooth(.4, .7, t) * 1.2 : 0, { clip: cut / rows }); }
          if (t > S - .05 && t < E + .5) { const a = t < S + .1 ? (t - S + .05) / .15 : 1 - smooth(E, E + .5, t), w = 2.5; for (let y = 0; y < FEET; y++) for (let x = -Math.ceil(w); x <= w; x++) dot(b, CX + x, y, Math.abs(x) < 1 ? WHITE : COOL[2], a * (1 - Math.abs(x) / (w + 1)) * 1.2); b.glow(CX, MID - 10, 20, COOL[2], .5 * a); }
          for (const q of parts) { const k = q.life / q.max; dot(b, q.x, q.y, k > .6 ? mix(q.c, WHITE, .5) : k > .3 ? COOL[2] : COOL[3], Math.min(1, k * 1.6)); }
          if (t > E + .1 && t < 2.4) flare(b, CX, TOP - 6, 5 * Math.sin(clamp((t - E - .1) / 1) * Math.PI), t, COOL[2], COOL[3]);
        },
      };
    },
  },
};

// How far the tool's prop reaches from the pet's column (its ox) and feet, in cells, over its whole loop: { l, r, u, d }
// (left, right, up, down). Only the prop (not the pet, not its shadow): the overlay places it where all of it fits.
const NOPET = { w: 1, h: 1, data: new Uint32Array(1), k: 1 }, EXTENT = new Map();
function fxExtent(o) {
  const key = `${o.seg}|${!!o.badge}`;
  if (!EXTENT.has(key)) {
    const run = fxRun('tool', { seg: o.seg, badge: !!o.badge, pet: () => NOPET, first: false }), b = new Buf();
    let x0 = GW, y0 = GH, x1 = -1, y1 = -1;
    for (let f = 0; f < FX.tool.cycle * 30; f++) {
      run.step(1 / 30); run.draw(b);
      for (let y = 0, i = 0; y < GH; y++) for (let x = 0; x < GW; x++, i++) if (b.c[i]) { if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y; }
    }
    EXTENT.set(key, { l: run.ox - x0, r: x1 + 1 - run.ox, u: FEET - y0, d: y1 + 1 - FEET });
  }
  return EXTENT.get(key);
}

// A pet's status balloon ("!" waiting, tick done): only without effects; with them the aura, the XP and the battery tell
// the state. The "+N" mark keeps its own balloon (a folded side pet shows no effect).
const statusBalloon = (fxOn, kind) => (fxOn ? '' : kind);

// ---- staying off other things ---------------------------------------------------------------------------------
// Where the effect canvas sits in its pet's sprite box (size px square, before the box's own scale): the pet's middle on
// column ox, its feet on the box's bottom (mirrored left-right / top-down as the effect is).
function fxPlace(size, ox, flipX, flipY) { // ox: the run's (fxRun(...).ox)
  const c = size / 18;
  return { left: size / 2 - (flipX ? GW - ox : ox) * c, top: flipY ? -(GH - FEET) * c : size - FEET * c, width: GW * c, height: GH * c };
}
// Rects are [left, top, right, bottom]. a minus b: up to 4 rects that cover a and not b.
function rectMinus(a, b) {
  if (b[0] >= a[2] || b[2] <= a[0] || b[1] >= a[3] || b[3] <= a[1]) return [a];
  const out = [], t = Math.max(a[1], b[1]), bo = Math.min(a[3], b[3]);
  if (a[1] < b[1]) out.push([a[0], a[1], a[2], b[1]]);
  if (b[3] < a[3]) out.push([a[0], b[3], a[2], a[3]]);
  if (a[0] < b[0]) out.push([a[0], t, b[0], bo]);
  if (b[2] < a[2]) out.push([b[2], t, a[2], bo]);
  return out;
}
// The parts of an effect's box (on screen) it must leave empty: everything in avoid (other pets, name tags, balloons,
// bubbles, cards), except where its own pet is, which it always draws; front: pets standing in front of its own pet,
// left empty even over it. Disjoint, clipped to the box.
function fxHoles(box, own, avoid, front = []) {
  const out = [];
  for (const a of [...front, ...avoid]) {
    if (!(a[2] > box[0] && a[0] < box[2] && a[3] > box[1] && a[1] < box[3])) continue;
    const cut = [Math.max(a[0], box[0]), Math.max(a[1], box[1]), Math.min(a[2], box[2]), Math.min(a[3], box[3])];
    let pieces = front.includes(a) ? [cut] : rectMinus(cut, own);
    for (const o of out) pieces = pieces.flatMap(p => rectMinus(p, o));
    out.push(...pieces.filter(p => p[2] - p[0] > .01 && p[3] - p[1] > .01));
  }
  return out;
}
// Those holes as a CSS clip-path for the effect's element (w x h CSS px, drawn in box on screen).
function fxClipPath(box, holes, w, h) {
  if (!holes.length) return '';
  const sx = w / (box[2] - box[0]), sy = h / (box[3] - box[1]), n = v => +v.toFixed(2);
  return `path(evenodd, "M0 0H${n(w)}V${n(h)}H0Z${holes.map(([l, t, r, b]) => `M${n((l - box[0]) * sx)} ${n((t - box[1]) * sy)}H${n((r - box[0]) * sx)}V${n((b - box[1]) * sy)}H${n((l - box[0]) * sx)}Z`).join('')}")`;
}

// ---- running ------------------------------------------------------------------------------------------------------
// One effect playing on one pet: a fixed 60 Hz simulation, drawn when the overlay draws (at most ~30 times a second).
// A loop effect starts over at its end (the tool prop keeps its clock, so only its first loop pops in); a one-shot
// reports done.
function fxRun(kind, o) {
  const def = FX[kind];
  let fx = def.make(o), k = 0, steps = 0, acc = 0; // steps of this loop and in all, counted whole so loops repeat exactly
  const N = () => Math.round((fx.D ?? def.D) / SIM);
  return {
    kind, ox: typeof def.ox === 'function' ? def.ox(o) : def.ox, loop: !!def.loop, done: false, get total() { return steps * SIM; },
    step(dt) {
      acc += Math.min(.1, dt);
      while (acc >= SIM && !this.done) {
        acc -= SIM; k++; steps++;
        if (k > N()) { if (!this.loop) { this.done = true; break; } k -= N(); fx = def.make({ ...o, first: false }); }
        fx.update(SIM, k * SIM, steps * SIM);
      }
    },
    draw(b, room = o.room) { b.fx = !!o.flipX; b.fy = !!o.flipY; b.flat = !!o.flat; b.room = room || null; b.clear(); fx.draw(b, k * SIM, steps * SIM); },
  };
}

// A loop effect's steady part drawn once as a sheet of frames (30 a second, cycle seconds long, from its second loop:
// the first one starts from nothing and may pop the prop in), so the overlay plays it as a CSS animation instead of drawing every frame: the same
// pixels for no work per frame. Built a few frames at a time (step) while the effect plays live meanwhile.
// ready: { url, fw, fh, cols, n, ms, lead, x, y }: the sheet's image, one frame's size with a 1 px gutter all round,
// frames per row, frame count, period, the live time before it starts, where a frame sits in the effect's image, and
// each frame's shake (cells). The gutter is 3 px so a 1-cell shake never shows the next frame.
function fxSheet(kind, o) {
  const def = FX[kind], n = Math.round(def.cycle * 30), lead = def.D, run = fxRun(kind, o), b = new Buf(); // from the 2nd loop on
  const img = { data: new Uint8ClampedArray(GW * GH * 16) }, frames = [], shakes = [], W2 = GW * 2, G = 3;
  let sim = 0, box = null;
  const sheet = {
    ready: null,
    step(budget) {
      while (budget-- > 0 && frames.length < n) {
        for (const until = Math.round((lead + frames.length / 30) / SIM); sim < until; sim++) run.step(SIM);
        run.draw(b); b.toImage(img); shakes.push(b.shake.slice());
        const r = b.was; if (!r) { frames.push(null); continue; }
        const w = r[2] - r[0], h = r[3] - r[1], px = new Uint32Array(w * h), all = new Uint32Array(img.data.buffer);
        for (let y = 0; y < h; y++) px.set(all.subarray((r[1] + y) * W2 + r[0], (r[1] + y) * W2 + r[2]), y * w);
        frames.push({ r, px });
        box = box ? [Math.min(box[0], r[0]), Math.min(box[1], r[1]), Math.max(box[2], r[2]), Math.max(box[3], r[3])] : r.slice();
      }
      if (frames.length === n && !this.ready && !this.packing) this.pack();
    },
    pack() {
      this.packing = true;
      const fw = (box ? box[2] - box[0] : 1) + 2 * G, fh = (box ? box[3] - box[1] : 1) + 2 * G, cols = Math.ceil(Math.sqrt(n * fh / fw)), rows = Math.ceil(n / cols); // near square: one layer, no tiles
      const cv = Object.assign(document.createElement('canvas'), { width: cols * fw, height: rows * fh }), g = cv.getContext('2d');
      frames.forEach((f, i) => { if (f) g.putImageData(new ImageData(new Uint8ClampedArray(f.px.buffer), f.r[2] - f.r[0], f.r[3] - f.r[1]), (i % cols) * fw + G + f.r[0] - box[0], Math.floor(i / cols) * fh + G + f.r[1] - box[1]); });
      frames.length = 0;
      cv.toBlob(bl => { this.ready = { url: URL.createObjectURL(bl), fw, fh, cols, n, ms: def.cycle * 1000, lead, x: (box ? box[0] : 0) - G, y: (box ? box[1] : 0) - G, shakes }; });
    },
  };
  return sheet;
}

// Sprites for the effects: the pet as petArt draws it (creature.js), painted into a canvas once its picture has loaded
// (onReady is called then); until then there is no sprite and no effect. A flat pet's sprite is its first frame without
// badge or ring, at the pet's 36×36 box: the cells it covers (blitFlat), the colours of its goodbye sparks, and the pet
// a "show when needed" balloon carries (pet-ride.js).
const FX_SPR = new Map();
function fxSprite(s, st, color, onReady) {
  const ci = color ?? hashStr((s.project || s.id) + 'c') % BODY.length, id = s.project || s.id, whole = s.pixel && s.whole;
  const key = s.image ? `${s.image}|${whole ? 'w' : ci}|${id}` : `flat|${id}|${st}|${ci}`;
  if (FX_SPR.has(key)) return FX_SPR.get(key);
  FX_SPR.set(key, null);
  const token = new URLSearchParams(location.search).get('t') || '', img = new Image();
  img.onload = () => { FX_SPR.set(key, !s.image ? flatSprite(img) : whole ? wholeSprite(img) : facedSprite(img, bigHeadLook(id, ci), s.pixel)); onReady?.(); };
  img.src = s.image ? `/pet-image/${encodeURIComponent(s.image)}?t=${encodeURIComponent(token)}` : 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(flatArt(id, st, ci));
  return null;
}
const canvas2d = (w, h) => { const c = document.createElement('canvas'); c.width = w; c.height = h; return c.getContext('2d', { willReadFrequently: true }); };
const spriteOf = (g, w, h, k) => ({ w, h, data: new Uint32Array(g.getImageData(0, 0, w, h).data.buffer.slice(0)), k });
// A flat pet at its box: solid where it is mostly covered, empty elsewhere (its soft shadow, its smoothed edge).
function flatSprite(img) {
  const g = canvas2d(36, 36); g.drawImage(img, 0, 0, 36, 36);
  const spr = spriteOf(g, 36, 36, 1);
  spr.data.forEach((c, i) => { spr.data[i] = c >>> 24 < 128 ? 0 : (c | 0xff000000) >>> 0; });
  return spr;
}
// The picture as the whole pet: the pet's box (36 fine pixels), standing on its bottom.
function wholeSprite(img) {
  const g = canvas2d(36, 36), f = Math.min(36 / img.naturalWidth, 36 / img.naturalHeight), w = img.naturalWidth * f, h = img.naturalHeight * f;
  g.imageSmoothingEnabled = false; g.drawImage(img, (36 - w) / 2, 36 - h, w, h);
  return spriteOf(g, 36, 36, 1);
}
// The picture as the face of a big-head creature (22 units, 110% of the pet's box): 2 pixels per unit, scaled to fit.
function facedSprite(img, look, pixel) {
  const g = canvas2d(44, 44), css = c => `rgb(${c & 255},${(c >>> 8) & 255},${(c >>> 16) & 255})`;
  const cells = (set, c) => { g.fillStyle = css(c); for (const k of set) { const [x, y] = k.split(',').map(Number); g.fillRect(x * 2, y * 2, 2, 2); } };
  cells(look.limbLine, look.line32); cells(look.limbs, look.body32); cells(look.body, look.body32);
  const side = Math.min(img.naturalWidth, img.naturalHeight); // preserveAspectRatio slice: the middle square
  g.imageSmoothingEnabled = !pixel; g.drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 9, 9, 26, 26);
  cells(look.ring, look.line32);
  return spriteOf(g, 44, 44, 36 * 1.1 / 44);
}
