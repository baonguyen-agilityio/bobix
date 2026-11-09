function hashStr(str) { let h = 2166136261; for (const c of str) h = Math.imul(h ^ c.charCodeAt(0), 16777619); return h >>> 0; }

// Sinh "sinh vật" pixel 7x7 đối xứng từ session_id, có viền tối 1px. Mắt/miệng đổi theo trạng thái.
const BODY = ['--sky', '--mint', '--lilac', '--peach', '--butter', '--rose'];
// color: an index into BODY (see petColor); by default the colour that goes with id.
// pixel: the face is pixel art, so it is scaled without smoothing.
// Without a picture the pet is the fine 36×36 version of the 7×7 creature (fineSprite), drawn as one path per colour.
function creature(id, st, face, color, pixel) {
  let r = hashStr(id) || 1;
  const bit = () => (r = Math.imul(r ^ (r >>> 15), 2246822507) >>> 0, r & 1);
  const ci = color ?? hashStr(id + 'c') % BODY.length;
  if (face) return bigHead(`var(${BODY[ci]})`, bit, face, pixel);
  return spriteSVG(fineSprite(id, st, ci));
}

// ---- the fine pet (36×36) ------------------------------------------------------------------------------------------
// Built from the 7×7 creature: Scale2x twice rounds the body's diagonal steps (ears, arms and legs stay apart), then
// a fresh 1-pixel outline, a light top-left and a dark bottom-right band, and a face redrawn per state. Colours are
// packed like ImageData (0xAABBGGRR) so the pets overlay's effects can draw the same pixels into a canvas.
// The design tokens the pets are made of (oklch L, C, h), the same values as ui/theme.css (a test keeps them equal).
const TOKENS = { '--n-900': [.24, .035, 266], '--sky': [.84, .075, 235], '--mint': [.86, .085, 162], '--lilac': [.83, .075, 300], '--peach': [.86, .075, 55], '--butter': [.90, .095, 98], '--rose': [.84, .075, 5] };
const packRGB = (r, g, b) => ((255 << 24) | (b << 16) | (g << 8) | r) >>> 0;
function oklch([L, C, h]) {
  const a = C * Math.cos(h * Math.PI / 180), b = C * Math.sin(h * Math.PI / 180);
  const l = (L + .3963377774 * a + .2158037573 * b) ** 3, m = (L - .1055613458 * a - .0638541728 * b) ** 3, s = (L - .0894841775 * a - 1.291485548 * b) ** 3;
  const g = x => Math.round(255 * Math.min(1, Math.max(0, x <= .0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - .055)));
  return packRGB(g(4.0767416621 * l - 3.3077115913 * m + .2309699292 * s), g(-1.2684380046 * l + 2.6097574011 * m - .3413193965 * s), g(-.0041960863 * l - .7034186147 * m + 1.707614701 * s));
}
// color-mix(in oklch, a p, b): lightness and chroma blend, the hue takes the shorter way round.
function mixOklch(a, b, p) { let dh = b[2] - a[2]; if (dh > 180) dh -= 360; if (dh < -180) dh += 360; return [a[0] * p + b[0] * (1 - p), a[1] * p + b[1] * (1 - p), a[2] + dh * (1 - p)]; }
const WHITE32 = packRGB(255, 255, 255);
const mix32 = (a, b, t) => { const m = s => Math.round(((a >>> s) & 255) * (1 - t) + ((b >>> s) & 255) * t); return packRGB(m(0), m(8), m(16)); };
const hex32 = c => '#' + [0, 8, 16].map(s => ((c >>> s) & 255).toString(16).padStart(2, '0')).join('');
// Scale2x (EPX): doubles a picture and rounds its diagonal steps without blurring.
function scale2x(src, w, h) {
  const W = w * 2, out = new Uint32Array(W * h * 2), at = (x, y) => (x < 0 || y < 0 || x >= w || y >= h ? 0 : src[y * w + x]);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const P = at(x, y), A = at(x, y - 1), B = at(x + 1, y), C = at(x - 1, y), D = at(x, y + 1);
    out[2 * y * W + 2 * x] = C === A && C !== D && A !== B ? A : P; out[2 * y * W + 2 * x + 1] = A === B && A !== C && B !== D ? B : P;
    out[(2 * y + 1) * W + 2 * x] = D === C && D !== B && C !== A ? C : P; out[(2 * y + 1) * W + 2 * x + 1] = B === D && B !== A && D !== C ? D : P;
  }
  return out;
}
// The 7×7 creature's cells (ears, arms, legs, rounded head) for a project: the same bits in the same order as always.
function creatureCells(id) {
  let r = hashStr(id) || 1;
  const bit = () => (r = Math.imul(r ^ (r >>> 15), 2246822507) >>> 0, r & 1);
  const on = new Set(), put = (x, y) => { on.add(`${x},${y}`); on.add(`${6 - x},${y}`); };
  for (let y = 1; y <= 5; y++) for (let x = 1; x <= 3; x++) put(x, y);
  if (bit()) put(1, 0); if (bit()) put(2, 0);          // tai / sừng
  for (let y = 2; y <= 4; y++) if (bit()) put(0, y);    // tay
  if (bit()) put(1, 6); else put(2, 6);                 // chân
  if (bit()) { on.delete('1,1'); on.delete('5,1'); }    // đầu tròn
  return on;
}
const FINE = new Map();
function fineSprite(id, st, ci) {
  const key = `${id}|${st}|${ci}`;
  if (FINE.has(key)) return FINE.get(key);
  const tone = TOKENS[BODY[ci]], body = oklch(tone), line = oklch(mixOklch(tone, TOKENS['--n-900'], .45)), W = 36;
  const d9 = new Uint32Array(81); // the body on the 9×9 grid (the old outline ring stays empty)
  for (const k of creatureCells(id)) { const [x, y] = k.split(',').map(Number); d9[(y + 1) * 9 + x + 1] = body; }
  const d = scale2x(scale2x(d9, 9, 9), 18, 18), o = d.slice();
  for (let y = 0; y < W; y++) for (let x = 0; x < W; x++) { // the new outline: every empty pixel touching the body (8 ways)
    if (d[y * W + x]) continue;
    outer: for (let j = -1; j <= 1; j++) for (let i = -1; i <= 1; i++) { const xx = x + i, yy = y + j; if (xx >= 0 && yy >= 0 && xx < W && yy < W && d[yy * W + xx]) { o[y * W + x] = line; break outer; } }
  }
  // Shading: body pixels within 2 of the rim get light on the top and left, dark on the bottom and right.
  const get = (x, y) => (x < 0 || y < 0 || x >= W || y >= W ? 0 : o[y * W + x]), rim = c => !c || c === line;
  const near = (x, y, dx, dy) => rim(get(x + dx, y + dy)) || rim(get(x + 2 * dx, y + 2 * dy));
  const light = mix32(body, WHITE32, .38), dark = mix32(body, line, .32), out = o.slice();
  for (let y = 0; y < W; y++) for (let x = 0; x < W; x++) {
    if (o[y * W + x] !== body) continue;
    if (near(x, y, 0, -1) || near(x, y, -1, 0)) out[y * W + x] = light; else if (near(x, y, 0, 1) || near(x, y, 1, 0)) out[y * W + x] = dark;
  }
  face36(out, st, oklch(TOKENS['--n-900']), oklch(TOKENS['--rose']));
  const spr = { w: W, h: W, data: out };
  FINE.set(key, spr);
  return spr;
}
// The face where the 7×7 creature has it (eyes on its pixels 2 and 4 of row 3): round eyes with a glint while working,
// wide eyes and an "o" while waiting, happy arcs, a U smile and pink cheeks when done, closed lines when idle.
function face36(o, st, ink, rose) {
  const P = (x, y, c) => { o[y * 36 + x] = c; }, R = (x, y, w, h, c) => { for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) P(x + i, y + j, c); };
  const eye = (x, y, w, h, glint) => { // a rectangle with its corners taken off, and a white glint top-left
    R(x, y, w, h, ink);
    for (const [cx, cy] of [[x, y], [x + w - 1, y], [x, y + h - 1], [x + w - 1, y + h - 1]]) P(cx, cy, o[cy * 36 + (cx === x ? cx - 1 : cx + 1)] || 0);
    R(x + 1, y + 1, glint, glint, WHITE32);
  };
  if (st === 'idle') { R(12, 18, 4, 1, ink); R(20, 18, 4, 1, ink); return; }
  if (st === 'done') {
    for (const x of [12, 20]) { P(x, 19, ink); P(x + 1, 18, ink); P(x + 2, 18, ink); P(x + 3, 19, ink); P(x, 20, ink); P(x + 3, 20, ink); }
    P(15, 23, ink); P(16, 24, ink); R(17, 25, 2, 1, ink); P(19, 24, ink); P(20, 23, ink); R(16, 23, 4, 1, ink); R(17, 24, 2, 1, packRGB(0xff, 0x8f, 0xa3));
    R(8, 21, 3, 2, rose); R(25, 21, 3, 2, rose); return;
  }
  if (st === 'waiting') { eye(12, 15, 4, 5, 2); eye(20, 15, 4, 5, 2); R(17, 23, 2, 3, ink); return; }
  eye(12, 16, 4, 4, 1); eye(20, 16, 4, 4, 1);
}
// A sprite as SVG: one path per colour, each run of pixels in a row one rectangle. Same box as the 9×9 creature had.
function spriteSVG(spr) {
  const runs = new Map();
  for (let y = 0; y < spr.h; y++) for (let x = 0; x < spr.w;) {
    const c = spr.data[y * spr.w + x];
    let n = 1; while (x + n < spr.w && spr.data[y * spr.w + x + n] === c) n++;
    if (c >>> 24) runs.set(c, (runs.get(c) || '') + `M${x} ${y}h${n}v1h-${n}z`);
    x += n;
  }
  return `<svg viewBox="0 0 ${spr.w} ${spr.h}" width="80%" height="80%" shape-rendering="crispEdges" aria-hidden="true">${[...runs].map(([c, d]) => `<path fill="${hex32(c)}" d="${d}"/>`).join('')}</svg>`;
}

// A creature wearing an uploaded picture: the picture (13 of 22 units) sits in the creature's body, and the outline around the
// body is drawn last, so it trims only the picture's corners into pixel steps. Ears, arms and legs come from the same project's creature (same bits,
// same order).
function bigHead(color, bit, face, pixel) {
  const px = (x, y, fill) => `<rect x="${x}" y="${y}" width="1" height="1" fill="${fill}"/>`;
  const line = `color-mix(in oklch, ${color} 45%, var(--n-900))`, { limbLine, limbs, body, ring } = bigHeadCells(bit);
  const cells = (set, fill) => [...set].map(k => { const [x, y] = k.split(',').map(Number); return px(x, y, fill); }).join('');
  return `<svg class="faced" viewBox="0 0 22 22" width="100%" height="100%" shape-rendering="crispEdges" aria-hidden="true">`
    + `${cells(limbLine, line)}${cells(limbs, color)}${cells(body, color)}`
    + `<image href="${face}" x="4.5" y="4.5" width="13" height="13"${pixel ? ' style="image-rendering:pixelated"' : ''} preserveAspectRatio="xMidYMid slice"/>${cells(ring, line)}</svg>`;
}
// The cells of a big-head creature on its 22×22 grid: limbs and their outline, the body (under the picture) and the
// outline ring drawn over the picture's corners. The pets overlay paints the same cells into its effects.
function bigHeadCells(bit) {
  const limbs = new Set(), put = (x, y) => { limbs.add(`${x},${y}`); limbs.add(`${21 - x},${y}`); };
  const block = (x, y, w, h) => { for (let i = 0; i < w; i++) for (let j = 0; j < h; j++) put(x + i, y + j); };
  if (bit()) block(6, 1, 2, 2); if (bit()) block(8, 1, 2, 2); // ears / horns
  for (const y of [7, 10, 13]) if (bit()) block(1, y, 2, 2);  // arms
  block(bit() ? 6 : 8, 19, 2, 2);                             // legs
  const body = new Set();
  for (let x = 4; x <= 17; x++) for (let y = 4; y <= 17; y++) body.add(`${x},${y}`);
  const cut = bit() ? [[4, 4], [5, 4], [4, 5]] : [[4, 4]];    // the corners of the body: rounder or squarer
  for (const [x, y] of cut) for (const [a, b] of [[x, y], [21 - x, y], [x, 21 - y], [21 - x, 21 - y]]) body.delete(`${a},${b}`);
  const around = (set, skip) => {
    const out = new Set();
    for (const k of set) { const [x, y] = k.split(',').map(Number); for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) { const n = `${x + dx},${y + dy}`; if (!skip.has(n)) out.add(n); } }
    return out;
  };
  return { limbLine: around(limbs, new Set([...limbs, ...body])), limbs, body, ring: around(body, body) };
}
// The picture pet's look for drawing in a canvas: big-head cells, and its colours as packed pixels (see fineSprite).
function bigHeadLook(id, color) {
  let r = hashStr(id) || 1;
  const bit = () => (r = Math.imul(r ^ (r >>> 15), 2246822507) >>> 0, r & 1);
  const tone = TOKENS[BODY[color ?? hashStr(id + 'c') % BODY.length]];
  return { ...bigHeadCells(bit), body32: oklch(tone), line32: oklch(mixOklch(tone, TOKENS['--n-900'], .45)) };
}

// A pixel-art picture that is the whole pet: it stands on the bottom of the pet's box, scaled without smoothing.
function picturePet(href) {
  return `<svg class="pic" viewBox="0 0 1 1" width="100%" height="100%" aria-hidden="true">`
    + `<image href="${href}" width="1" height="1" preserveAspectRatio="xMidYMax meet" style="image-rendering:pixelated"/></svg>`;
}

// Where to answer a permission request Bobix can't answer: the Claude app asks in its own card for its sessions.
const approveWhere = s => (s.hostSession ? 'ask.whereApp' : 'pane.askHint');

// The pet of a session: its project's creature, wearing the picture chosen for that project if there is one, or the
// picture itself when the person chose it to be the whole pet.
function petArt(s, st, color) {
  const token = new URLSearchParams(location.search).get('t') || '';
  const pic = s.image && `/pet-image/${encodeURIComponent(s.image)}?t=${encodeURIComponent(token)}`;
  return pic && s.pixel && s.whole ? picturePet(pic) : creature(s.project || s.id, st, pic, color, s.pixel);
}

// A session's colour. Its project gives the shape; the colour tells sessions apart: sessions of one project take the
// project's colour and the next ones in turn, and a side session never has its parent's colour or its neighbour's.
function petColor(s, list) {
  // In the order sessions came (the list keeps it), so one that arrives later never shifts the colour of one already out.
  const n = BODY.length, proj = x => x.project || x.id, ids = xs => xs.map(x => x.id);
  const top = x => !(x.parent && list.some(y => y.id === x.parent)); // stands on its own (no parent on screen)
  const par = s.parent && list.find(x => x.id === s.parent);
  if (par) { // side sessions avoid the colours of their project's own sessions, so a kid never looks like one of them
    const taken = new Set(list.filter(x => top(x) && proj(x) === proj(par)).map(x => petColor(x, list)));
    const free = [...Array(n).keys()].map(i => (petColor(par, list) + 1 + i) % n).filter(i => !taken.has(i));
    const k = ids(list.filter(x => x.parent === s.parent)).indexOf(s.id);
    return free.length ? free[k % free.length] : (petColor(par, list) + 1 + (k % (n - 1))) % n;
  }
  const mates = ids(list.filter(x => top(x) && proj(x) === proj(s)));
  return (hashStr(proj(s) + 'c') % n + Math.max(0, mates.indexOf(s.id))) % n;
}

// A party balloon in the pets' pixel style: orange with "!" waits for you, green with a tick is done. Its string hangs to the pet.
// On the top edge it floats below the pet: balloon and string are flipped, the "!" / tick and the shine are not.
function balloonSVG(kind, down = false) {
  const W = 11, rows = ['...xxxxx...', '..xxxxxxx..', '.xxxxxxxxx.', 'xxxxxxxxxxx', 'xxxxxxxxxxx', 'xxxxxxxxxxx', 'xxxxxxxxxxx',
    '.xxxxxxxxx.', '.xxxxxxxxx.', '..xxxxxxx..', '...xxxxx...', '....xxx....', '.....x.....'];
  const on = new Set(); rows.forEach((r, y) => [...r].forEach((c, x) => c === 'x' && on.add(`${x},${y}`)));
  const fill = kind === 'waiting' ? 'var(--coral)' : 'var(--mint)', line = `color-mix(in oklch, ${fill} 45%, var(--n-900))`;
  const icon = kind === 'waiting' ? [[5, 3], [5, 4], [5, 5], [5, 6], [5, 8]] : [[3, 5], [4, 6], [5, 7], [6, 6], [7, 5], [8, 4]];
  const shine = [[3, 2], [2, 3], [2, 4]], px = (x, y, f) => `<rect x="${x}" y="${y}" width="1" height="1" fill="${f}"/>`;
  let out = '';
  for (const k of on) { const [x, y] = k.split(',').map(Number); const edge = [[1, 0], [-1, 0], [0, 1], [0, -1]].some(([a, b]) => !on.has(`${x + a},${y + b}`)); out += px(x, y, edge ? line : fill); }
  out += [5, 5, 6, 6, 5, 5, 4, 4, 5].map((x, i) => px(x, 13 + i, 'var(--ink-3)')).join(''); // the string
  // Flipped, body row y shows at 21 - y; the upright marks shift by 11 so they land on the same part of the body.
  const marks = shine.map(([x, y]) => px(x, y + (down ? 11 : 0), 'oklch(98% 0.01 90 / .85)')).join('') + icon.map(([x, y]) => px(x, y + (down ? 11 : 0), 'var(--n-900)')).join('');
  return `<svg viewBox="0 0 ${W} 22" shape-rendering="crispEdges" aria-hidden="true">${down ? `<g transform="matrix(1 0 0 -1 0 22)">${out}</g>` : out}${marks}</svg>`;
}
