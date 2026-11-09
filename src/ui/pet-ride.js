// "Show when needed" (pets.html): a pet that needs you hangs under the app's pixel balloon (balloonSVG's 11×13 map) and
// crosses the screen. Same pixel technique as the pet and its effects (pet-fx.js): one canvas pixel = one fine pet
// pixel, balloon pixels and particles on a 2-pixel cell grid, dark outlines, Bayer dithering for transparency. The
// floating loop is drawn once into a near-square sheet of frames and played by moving it (nothing is drawn while a pet
// just floats); the exits (popped when answered, let go when done, sinking when context ran out) are drawn live for
// about 1.4 s. Needs creature.js (packRGB, oklch, mixOklch, mix32, TOKENS). Exposes window.PetRide.
(() => {
  const FPS = 30, CYCLE = 2.4; // loop sheets: 30 frames a second, one 2.4 s cycle
  const TAU = Math.PI * 2;
  // Live canvas for the exits: room for confetti and a pet floating down below its spot. Anchor = the pet's feet.
  const LW = 150, LH = 210, LAX = 75, LAY = 150;
  // One loop frame, and the anchor of the pet's feet in it.
  const F = { w: 56, h: 104, ax: 28, ay: 92 };
  // Room the balloon takes above the pet's box, at the default 56 px pet (scaled with the pet).
  const ABOVE = 60 / 56;

  // ---- colours (ramps: 0 white … 6 darkest, as in pet-fx.js) ----
  const hex = h => packRGB(parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)), ramp = a => a.map(hex);
  const WARM = ramp(['#ffffff', '#fffbe2', '#fff0a6', '#ffd862', '#f6aa3c', '#cf722b', '#7e3c1f']);
  const COOL = ramp(['#ffffff', '#dcfbff', '#a6ecff', '#62c8f4', '#3a8bd8', '#2c58aa', '#212f6b']);
  const CORAL = ramp(['#ffffff', '#ffe4d6', '#ffb08a', '#ff7a4d', '#f04a2a', '#b8301f', '#6e1c16']);
  const GREEN = ramp(['#ffffff', '#e6ffdc', '#acff92', '#62e26a', '#30b55b', '#20804c', '#175237']);
  const LILAC = ramp(['#ffffff', '#f3e6ff', '#dcb8ff', '#b98af0', '#9160d0', '#6a40a8', '#3f2566']);
  const AMBER = ramp(['#ffffff', '#fff3d6', '#ffd98a', '#ffbb3d', '#f0921c', '#b8650f', '#6b3a0a']);
  const WHITE = hex('#ffffff'), INK = hex('#1a1422'), DUST = hex('#9a90a8'), SILK = hex('#f1eef6');
  const STATE = { waiting: CORAL, done: GREEN, ctx: AMBER };
  const CONFETTI = [CORAL, GREEN, WARM, COOL, LILAC];

  // ---- maths ----
  const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
  const smooth = (a, b, x) => { const t = clamp((x - a) / (b - a)); return t * t * (3 - 2 * t); };
  const outCubic = t => 1 - (1 - clamp(t)) ** 3;
  class Rng {
    constructor(s = 1) { this.s = s >>> 0 || 1; }
    next() { let t = (this.s = (this.s + 0x6d2b79f5) >>> 0); t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }
    range(a, b) { return a + (b - a) * this.next(); } pick(a) { return a[Math.floor(this.next() * a.length)]; }
  }
  const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
  const dith = (a, x, y) => a >= 1 || a > (BAYER[((y & 3) << 2) | (x & 3)] + .5) / 16;

  // ---- a pixel buffer ----
  class Pix {
    constructor(w, h) { this.w = w; this.h = h; this.d = new Uint32Array(w * h); this.img = new ImageData(new Uint8ClampedArray(this.d.buffer), w, h); }
    clear() { this.d.fill(0); }
    set(x, y, c) { x = Math.floor(x); y = Math.floor(y); if (x >= 0 && y >= 0 && x < this.w && y < this.h) this.d[y * this.w + x] = c; }
    // One effect pixel: 2×2 fine pixels on the cell grid, dithered by a.
    cell(x, y, c, a = 1) { const X = Math.floor(x / 2), Y = Math.floor(y / 2); if (!dith(a, X, Y)) return; for (const [i, j] of [[0, 0], [1, 0], [0, 1], [1, 1]]) this.set(X * 2 + i, Y * 2 + j, c); }
    // An object on the cell grid with a dark outline: col(u, v) is the colour of the cell u, v cells from its centre, or 0.
    obj(cx, cy, R, col, a = 1, edge = INK) {
      const X = Math.round(cx / 2), Y = Math.round(cy / 2), n = Math.ceil(R) + 2, N = 2 * n + 1, m = new Uint32Array(N * N);
      for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) m[j * N + i] = col(i - n, j - n) || 0;
      const on = (i, j) => i >= 0 && j >= 0 && i < N && j < N && m[j * N + i];
      for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) {
        const c = m[j * N + i] || (on(i - 1, j) || on(i + 1, j) || on(i, j - 1) || on(i, j + 1) ? edge : 0);
        if (c) this.cell((X + i - n) * 2, (Y + j - n) * 2, c, a);
      }
    }
    // The pet (a pet-fx.js sprite: spr.k fine pixels a sprite pixel), feet on (cx, feet); dithered out by a.
    sprite(spr, cx, feet, a = 1) {
      const k = spr.k || 1, w = Math.round(spr.w * k), h = Math.round(spr.h * k), x0 = Math.round(cx - w / 2), y0 = Math.round(feet - h);
      for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
        const c = spr.data[Math.min(spr.h - 1, Math.floor(y / k)) * spr.w + Math.min(spr.w - 1, Math.floor(x / k))];
        if (c >>> 24 && dith(a, x0 + x, y0 + y)) this.set(x0 + x, y0 + y, c);
      }
    }
    line(x0, y0, x1, y1, c, a = 1) {
      const n = Math.ceil(Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0))) || 1;
      for (let i = 0; i <= n; i++) { const x = Math.round(x0 + (x1 - x0) * i / n), y = Math.round(y0 + (y1 - y0) * i / n); if (dith(a, x, y)) this.set(x, y, c); }
    }
  }
  function star(P, x, y, arm, c, a = 1) { for (let d = 1; d <= arm; d++) for (const [i, j] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) P.cell(x + i * d * 2, y + j * d * 2, d === arm ? c : WHITE, a); P.cell(x, y, WHITE, a); }
  function ring(P, x, y, r, c, a = 1) { const n = Math.max(8, Math.ceil(r * 1.6)); for (let i = 0; i < n; i++) P.cell(x + Math.cos(i / n * TAU) * r, y + Math.sin(i / n * TAU) * r, c, a); }

  // ---- the balloon: balloonSVG's map, outline (its colour mixed 45% with --n-900), shine and "!" / tick ----
  // One balloon pixel = one effect cell, so it is as big next to the pet as the edge balloon. (kx, ky): the bottom of its
  // knot. sc: size (shrinks as context runs out), sq: squash (+ wide, − tall), flash: the body turns white.
  const BMAP = ['...xxxxx...', '..xxxxxxx..', '.xxxxxxxxx.', 'xxxxxxxxxxx', 'xxxxxxxxxxx', 'xxxxxxxxxxx', 'xxxxxxxxxxx',
    '.xxxxxxxxx.', '.xxxxxxxxx.', '..xxxxxxx..', '...xxxxx...', '....xxx....', '.....x.....'];
  const B_ON = (u, v) => BMAP[v]?.[u] === 'x';
  const B_ICON = { bang: [[5, 3], [5, 4], [5, 5], [5, 6], [5, 8]], tick: [[3, 5], [4, 6], [5, 7], [6, 6], [7, 5], [8, 4]] };
  const B_SHINE = [[3, 2], [2, 3], [2, 4]];
  const B_WIGGLE = [0, 0, 1, 1, 0, 0, -1, -1, 0]; // the string's pixels, as balloonSVG draws them
  const N900 = TOKENS['--n-900'], STRING = oklch([.52, .03, 262]); // the app's --ink-3
  // Waiting and done as on the edge; low context: --butter with "!".
  const B_LOOK = { waiting: [[.66, .175, 33], 'bang'], done: [TOKENS['--mint'], 'tick'], ctx: [TOKENS['--butter'], 'bang'] };
  function balloonColours(need) {
    const [t, icon] = B_LOOK[need] || B_LOOK.waiting, fill = oklch(t);
    return { fill, line: oklch(mixOklch(t, N900, .45)), shine: mix32(fill, packRGB(250, 247, 238), .85), ink: oklch(N900), icon: new Set(B_ICON[icon].map(([x, y]) => `${x},${y}`)) };
  }
  function balloon(P, kx, ky, B, o = {}) {
    const sc = o.sc ?? 1, sq = o.sq ?? 0, qx = 2 * sc * (1 + sq), qy = 2 * sc * (1 - sq), a = o.a ?? 1;
    let x0 = Math.round(kx - 5.5 * qx), y0 = Math.round(ky - 13 * qy);
    if (sc === 1 && !sq) { x0 = Math.round(x0 / 2) * 2; y0 = Math.round(y0 / 2) * 2; } // on the cell grid
    const shine = new Set(B_SHINE.map(([x, y]) => `${x},${y}`));
    for (let y = 0; y < Math.round(13 * qy); y++) for (let x = 0; x < Math.round(11 * qx); x++) {
      const u = Math.floor(x / qx), v = Math.floor(y / qy);
      if (!B_ON(u, v) || !dith(a, x0 + x, y0 + y)) continue;
      const edge = !B_ON(u + 1, v) || !B_ON(u - 1, v) || !B_ON(u, v + 1) || !B_ON(u, v - 1), k = `${u},${v}`;
      P.set(x0 + x, y0 + y, edge ? B.line : o.flash ? WHITE : B.icon.has(k) ? B.ink : shine.has(k) ? B.shine : B.fill);
    }
    return ky - 6.5 * qy; // the middle of the balloon
  }
  // Its string: one balloon pixel wide, the app's wiggle, bent from the knot (kx, ky) to (x1, y1).
  function bstring(P, kx, ky, x1, y1, bend = 0, a = 1) {
    const n = Math.max(2, Math.round((y1 - ky) / 2));
    for (let i = 0; i < n; i++) { const t = i / (n - 1); P.cell(kx + (x1 - kx) * t + B_WIGGLE[i % 9] * 2 + bend * Math.sin(Math.PI * t), ky + i * 2, STRING, a); }
  }
  // A tiny parachute whose canopy bottom is at y, lines down to the pet's shoulders.
  function chute(P, x, y, open, r, a) {
    if (open <= .05) return;
    const w = Math.max(1, 4.6 * open), h = Math.max(1, 2.6 * open);
    P.line(x - w * 2 + 1, y, x - 7, y + 12, INK, a); P.line(x + w * 2 - 1, y, x + 7, y + 12, INK, a);
    P.obj(x, y - 2, w + 1, (u, v) => (v <= 0 && (u / w) ** 2 + (v / h) ** 2 <= 1 ? (Math.floor((u + 40) / 1.6) & 1 ? WHITE : r[3]) : 0), a);
  }
  const hand = (P, x, y, c, a = 1) => P.obj(x, y, 1, (u, v) => (u * u + v * v <= 1 ? c : 0), a);

  // One frame of a pet under its balloon, ph = phase of the floating loop: the balloon drifts on a small ellipse, the
  // pet hangs under it a beat behind; k = how much air is gone (0–4).
  function balloonPose(P, ax, ay, L, o = {}) {
    const ph = o.ph ?? 0, k = o.k ?? 0, dy = k * 2 + (o.dy ?? 0), sc = (1 - .12 * k) * (o.sc ?? 1);
    const kx = ax + 3 * Math.cos(ph), ky = ay - 52 + 2 * Math.sin(ph) + dy;
    const px = ax + 1.5 * Math.cos(ph - .7), feet = ay + 1.2 * Math.sin(ph - .5) + dy;
    bstring(P, kx, ky, px, feet - 32, 0, o.a ?? 1);
    const bcy = balloon(P, kx, ky, L.B, { sc, sq: o.sq, flash: o.flash, a: o.a });
    P.sprite(L.spr, px, feet, o.a ?? 1);
    return { kx, ky, bcy, px, feet };
  }

  // ---- particles ----
  function burst(list, rng, x, y, n, o) {
    for (let i = 0; i < n; i++) {
      const ang = o.ang != null ? o.ang + rng.range(-o.spread, o.spread) : rng.range(0, TAU), v = rng.range(o.v * .35, o.v), r0 = o.r0 ?? 0, pal = rng.pick(o.cols);
      list.push({ x: x + Math.cos(ang) * r0, y: y + Math.sin(ang) * r0, vx: Math.cos(ang) * v, vy: Math.sin(ang) * v - (o.lift ?? 0), c: pal[3] ?? pal, c2: pal[5] ?? pal,
        life: rng.range(o.life * .6, o.life), age: 0, g: o.g ?? 240, drag: o.drag ?? 2.5, puff: o.puff, ph: rng.range(0, TAU), r: o.r ?? 1 });
    }
  }
  function parts(P, list, dt) {
    for (let i = list.length - 1; i >= 0; i--) {
      const p = list[i]; p.age += dt; if (p.age >= p.life) { list.splice(i, 1); continue; }
      const k = Math.exp(-p.drag * dt); p.vx *= k; p.vy = p.vy * k + p.g * dt; p.x += p.vx * dt; p.y += p.vy * dt;
      const u = p.age / p.life, a = 1 - smooth(.55, 1, u);
      if (p.puff) { const r = p.r * (.6 + u * .9); P.obj(p.x, p.y, r, (x, y) => (x * x + y * y <= r * r ? (y > r * .4 ? SILK : WHITE) : 0), a, DUST); } // a soft cloud
      else P.cell(p.x, p.y, Math.sin(p.age * 18 + p.ph) > 0 ? p.c : p.c2, a);
    }
  }
  const puff = (list, rng, x, y) => burst(list, rng, x, y, 7, { v: 34, cols: [DUST], life: .5, g: -40, drag: 4, puff: true, r: 2, r0: 10 });

  // ---- exits (live): each returns { D, draw(P, t, dt) } ----
  const EXIT = {
    // Answered: the balloon stretches, flashes and pops into confetti; the pet floats down on a tiny parachute.
    pop: c => { const D = 1.3, rng = new Rng(c.seed), ps = []; let b = null;
      return { D, draw(P, t, dt) {
        const ph = c.ph0 + TAU * Math.min(t, .1) / CYCLE;
        if (t < .1 || !b) { b = balloonPose(P, LAX, LAY, c.L, { ph, k: c.k, sq: .16 * t / .1, sc: 1 + .08 * t / .1, flash: t > .055 }); return; }
        if (!ps.done) { ps.done = true; burst(ps, rng, b.kx, b.bcy, 30, { v: 150, cols: CONFETTI, life: 1.1, g: 260, drag: 2.2, r0: 6 }); }
        const u = t - .1, open = smooth(.04, .24, u), fall = u < .18 ? 200 * u * u : 6.5 + 26 * (u - .18), a = 1 - smooth(.85, 1.3, t);
        const px = b.px + 3 * Math.sin(u * 6) * open, feet = b.feet + fall;
        chute(P, px, feet - 46, open, c.L.r, a);
        P.sprite(c.L.spr, px, feet, a);
        if (u < .28) ring(P, b.kx, b.bcy, 4 + 26 * outCubic(u / .28), u < .08 ? WHITE : c.L.r[2], 1 - u / .28);
        if (u < .14) star(P, b.kx, b.bcy, u < .07 ? 3 : 2, c.L.r[2]);
        parts(P, ps, dt);
      } }; },
    // Done and dismissed: the pet waves, lets go, the balloon floats off the top of the screen, the pet goes in a puff.
    release: c => { const D = 1.4, rng = new Rng(c.seed), ps = []; let b = null;
      return { D, draw(P, t, dt) {
        if (t < .55 || !b) { b = balloonPose(P, LAX, LAY, c.L, { ph: c.ph0 + TAU * t / CYCLE, k: c.k }); hand(P, b.px + 18 + 2 * Math.sin(t * 22), b.feet - 30, c.L.body); return; }
        if (!ps.done) { ps.done = true; c.free(b.kx, b.ky); burst(ps, rng, b.px, b.feet - 20, 8, { v: 50, cols: [GREEN, WARM], life: .6, g: -60, drag: 3, r0: 12 }); }
        const u = t - .55, a = 1 - smooth(.8, 1.3, t);
        P.sprite(c.L.spr, b.px, b.feet + 6 * outCubic(u / .25), a);
        if (u < .2) hand(P, b.px + 18, b.feet - 28 + 10 * u, c.L.body, a);
        if (t > .8 && !ps.puffed) { ps.puffed = true; puff(ps, rng, b.px, b.feet - 14); }
        parts(P, ps, dt);
      } }; },
    // Low context, dismissed: the balloon keeps leaking, pet and balloon sink and fade.
    sink: c => { const D = 1.4, rng = new Rng(c.seed), ps = [];
      return { D, draw(P, t, dt) {
        const a = 1 - smooth(.6, 1.4, t), b = balloonPose(P, LAX, LAY, c.L, { ph: c.ph0 + TAU * t / CYCLE, k: 4, sc: 1 - .45 * smooth(0, D, t), dy: 40 * (t / D) ** 2, a });
        if (Math.floor(t * 30) % 3 === 0 && t < 1) burst(ps, rng, b.kx, b.ky, 1, { ang: Math.PI / 2, spread: .8, v: 30, cols: [WHITE, DUST], life: .3, g: 0, drag: 3 });
        parts(P, ps, dt);
      } }; },
  };
  const EXIT_OF = { waiting: 'pop', done: 'release', ctx: 'sink' };

  // ---- loop sheets: one cycle drawn once into a near-square sheet, shared by every pet with the same look ----
  const SHEETS = new Map();
  function sheetOf(L, k) {
    const key = `${L.key}|${k}`;
    if (SHEETS.has(key)) return SHEETS.get(key);
    const n = Math.round(CYCLE * FPS), cols = Math.ceil(Math.sqrt(n * F.h / F.w)), rows = Math.ceil(n / cols);
    const W = cols * F.w, d = new Uint32Array(W * rows * F.h), P = new Pix(F.w, F.h);
    for (let i = 0; i < n; i++) {
      P.clear(); balloonPose(P, F.ax, F.ay, L, { ph: TAU * i / n, k });
      const x0 = (i % cols) * F.w, y0 = Math.floor(i / cols) * F.h;
      for (let y = 0; y < F.h; y++) d.set(P.d.subarray(y * F.w, (y + 1) * F.w), (y0 + y) * W + x0);
    }
    const cv = Object.assign(document.createElement('canvas'), { width: W, height: rows * F.h });
    cv.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(d.buffer), W, rows * F.h), 0, 0);
    const sh = { cv, cols, n };
    SHEETS.set(key, sh);
    if (SHEETS.size > 16) SHEETS.delete(SHEETS.keys().next().value);
    return sh;
  }

  // A pet's look under its balloon: its sprite (pet-fx.js, a picture pet's too), its body colour (for the waving hand),
  // the need's colour ramp and balloon.
  function look(spr, need, key) {
    const n = new Map();
    for (const c of spr.data) if (c >>> 24) n.set(c, (n.get(c) || 0) + 1);
    return { spr, body: [...n].sort((a, b) => b[1] - a[1])[0]?.[0] ?? INK, r: STATE[need] || CORAL, B: balloonColours(need), key: `${key}|${need}` };
  }

  // ---- balloons let go of: drawn once, then only moved up and off the screen ----
  const FREES = [];
  function freeBalloon(layer, x, y, B, U) { // x, y: where the knot is on screen
    const P = new Pix(40, 78), kx = 20, ky = 46;
    bstring(P, kx, ky, kx - 2, ky + 18, 2);
    balloon(P, kx, ky, B);
    const cv = Object.assign(document.createElement('canvas'), { width: P.w, height: P.h, className: 'free' });
    cv.getContext('2d').putImageData(P.img, 0, 0);
    layer.prepend(cv); // behind every pet
    FREES.push({ cv, x: x - kx * U, y: y - ky * U, U, t0: performance.now() });
  }
  function freesTick(now) {
    for (let i = FREES.length - 1; i >= 0; i--) {
      const f = FREES[i], t = (now - f.t0) / 1000, y = f.y - (50 * t + 300 * t * t), x = f.x + 6 * Math.sin(t * 3);
      f.cv.style.transform = `translate(${Math.round(x)}px,${Math.round(y)}px) scale(${f.U})`;
      if (y < -140 || t > 4) { f.cv.remove(); FREES.splice(i, 1); }
    }
    return FREES.length;
  }

  // ---- one pet's ride ----
  // size: the pet's box in CSS px; host: { origin() → [x, y] of the pet's box on screen, layer }.
  class Ride {
    constructor(L, size, host, seed) {
      const U = size * .8 / 36; // CSS px a fine pet pixel (the creature is 80% of its box)
      Object.assign(this, { L, U, fx: size / 2, fy: size * .9, host, seed, k: 0, run: null, idx: -1, frozen: false, t0: performance.now() });
      const div = c => Object.assign(document.createElement('div'), { className: c });
      this.el = div('fly');
      this.P = new Pix(LW, LH);
      this.cv = Object.assign(document.createElement('canvas'), { width: LW, height: LH });
      Object.assign(this.cv.style, { left: `${this.fx - LAX * U}px`, top: `${this.fy - LAY * U}px`, transform: `scale(${U})` });
      this.win = div('sheetbox');
      this.sheetCv = document.createElement('canvas');
      this.win.append(this.sheetCv);
      this.el.append(this.win, this.cv);
      this.cv.hidden = true;
      this.sheet();
    }
    sheet() {
      const sh = sheetOf(this.L, this.k), U = this.U;
      this.sh = sh;
      Object.assign(this.sheetCv, { width: sh.cv.width, height: sh.cv.height });
      this.sheetCv.getContext('2d').drawImage(sh.cv, 0, 0);
      Object.assign(this.win.style, { width: `${F.w}px`, height: `${F.h}px`, left: `${this.fx - F.ax * U}px`, top: `${this.fy - F.ay * U}px`, transform: `scale(${U})` });
      this.idx = -1; this.frame(0);
    }
    // Starts the exit for this need (answered, or done / low context dismissed).
    leave(need) {
      const c = { L: this.L, k: this.k, seed: this.seed, ph0: this.phase(performance.now()),
        free: (kx, ky) => { const [x, y] = this.host.origin(); freeBalloon(this.host.layer, x + this.fx + (kx - LAX) * this.U, y + this.fy + (ky - LAY) * this.U, this.L.B, this.U); } };
      this.run = EXIT[EXIT_OF[need] || 'pop'](c); this.rt = performance.now(); this.cv.hidden = false; this.win.hidden = true;
    }
    phase(now) { return TAU * (((now - this.t0) / 1000) % CYCLE) / CYCLE; }
    setLevel(k) { if (k > this.k) { this.k = k; this.sheet(); } }
    frame(i) {
      if (i === this.idx) return;
      this.idx = i;
      const { cols } = this.sh;
      this.sheetCv.style.transform = `translate(${-(i % cols) * F.w}px,${-Math.floor(i / cols) * F.h}px)`;
    }
    // Every overlay frame while the pet is shown; 'end' once its exit has played.
    tick(now, dt) {
      if (this.run) {
        const r = this.run, t = Math.min((now - this.rt) / 1000, r.D);
        this.P.clear(); r.draw(this.P, t, dt); this.cv.getContext('2d').putImageData(this.P.img, 0, 0);
        return t < r.D ? null : 'end';
      }
      if (!this.frozen) this.frame(Math.floor(((now - this.t0) / 1000 % CYCLE) / CYCLE * this.sh.n) % this.sh.n);
      return null;
    }
  }
  window.PetRide = { Ride, look, freesTick, above: size => ABOVE * size };
})();
