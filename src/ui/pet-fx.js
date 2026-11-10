// Effects for the pets overlay (pets.html), drawn as flat SVG (soft shapes, the pets' palette, no outline) in one layer
// per pet while an effect plays. The pet's own SVG stays on screen and keeps acting its state; the effect draws round it.
// Colour ramps and particle motion follow Sprite Fusion's MIT VFX by Hugo Duprez (MIT, see MIT-sprite-fusion-vfx.txt).
// Grid: the pet's box is 18 × 18 cells, its feet on row FEET of column ox; the layer is GW × GH cells.
// Needs creature.js (n1).
const GW = 88, GH = 60, FEET = 54, CX = 44, SIM = 1 / 60, TOP = FEET - 18, MID = FEET - 9;
const XP_PER_LV = 3; // XP_PER_LV in src-tauri/src/state.rs: finished tasks per level

// ---- when effects play (pure: tested in test/core.test.js) ------------------------------------------------------
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

// ---- colours (ramps: 0 white … 6 darkest) and maths ---------------------------------------------------------------
const WARM = ['#ffffff', '#fffbe2', '#fff0a6', '#ffd862', '#f6aa3c', '#cf722b', '#7e3c1f'];
const COOL = ['#ffffff', '#dcfbff', '#a6ecff', '#62c8f4', '#3a8bd8', '#2c58aa', '#212f6b'];
const RED = ['#ffffff', '#ffd9d9', '#ff8a8a', '#ff4d4d', '#e02828', '#a3161b', '#5e0d12'];
const INK = '#1f2230';
const TAU = Math.PI * 2, clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const smooth = (a, b, x) => { const t = clamp((x - a) / (b - a)); return t * t * (3 - 2 * t); };
const outBack = (t, s = 1.7) => { const u = clamp(t) - 1; return 1 + (s + 1) * u * u * u + s * u * u; };
const outCubic = t => 1 - Math.pow(1 - clamp(t), 3), outExpo = t => (t >= 1 ? 1 : 1 - Math.pow(2, -10 * clamp(t)));
class Rng {
  constructor(s = 1) { this.s = s >>> 0 || 1; }
  next() { let t = (this.s = (this.s + 0x6d2b79f5) >>> 0); t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }
  range(a, b) { return a + (b - a) * this.next(); } chance(p) { return this.next() < p; } pick(a) { return a[Math.floor(this.next() * a.length)]; }
}
function stepList(list, dt, drag = 0, grav = 0) { for (let i = list.length - 1; i >= 0; i--) { const p = list[i]; if ((p.life -= dt) <= 0) { list.splice(i, 1); continue; } p.vx -= p.vx * drag * dt; p.vy -= p.vy * drag * dt; p.vy += grav * dt; p.x += p.vx * dt; p.y += p.vy * dt; } }

// ---- one frame ----------------------------------------------------------------------------------------------------
// The shapes of one frame, in the effect's own cells. room: [x0, y0, x1, y1], the part of the grid it may draw in (the
// pet's room on screen, see pets.html fxRoom): a shape is drawn only when it lies whole inside it (a light column is cut
// to it instead). fy: mirrored top-down (the top edge of the screen), text and icons kept upright. boxes: each shape's
// box, for the tests.
class Scene {
  constructor(room = null, fy = false) { this.room = room; this.fy = fy; this.out = []; this.boxes = []; this.shake = [0, 0]; }
  fits(x0, y0, x1, y1) { const r = this.room; return !r || (x0 >= r[0] && y0 >= r[1] && x1 <= r[2] && y1 <= r[3]); }
  add(svg, x0, y0, x1, y1) { if (!this.fits(x0, y0, x1, y1)) return false; this.out.push(svg); this.boxes.push([x0, y0, x1, y1]); return true; }
  // A box cut to the room: [x0, y0, x1, y1], or null when nothing is left.
  cut(x0, y0, x1, y1) { const r = this.room || [0, 0, GW, GH], b = [Math.max(x0, r[0]), Math.max(y0, r[1]), Math.min(x1, r[2]), Math.min(y1, r[3])]; return b[2] > b[0] && b[3] > b[1] ? b : null; }
  // Something drawn upright round (cx, cy) even when the frame is mirrored.
  upright(cx, cy, inner) { return this.fy ? `<g transform="translate(${n1(cx)} ${n1(cy)}) scale(1 -1) translate(${n1(-cx)} ${n1(-cy)})">${inner}</g>` : inner; }
  circle(x, y, r, fill, a = 1) { return a > .02 && this.add(`<circle cx="${n1(x)}" cy="${n1(y)}" r="${n1(r)}" fill="${fill}"${op(a)}/>`, x - r, y - r, x + r, y + r); }
  rect(x, y, w, h, fill, a = 1, rx = 0) { return a > .02 && this.add(`<rect x="${n1(x)}" y="${n1(y)}" width="${n1(w)}" height="${n1(h)}" rx="${n1(rx)}" fill="${fill}"${op(a)}/>`, x, y, x + w, y + h); }
  line(pts, stroke, w, a = 1) {
    if (a <= .02 || pts.length < 2) return false;
    const xs = pts.map(p => p[0]), ys = pts.map(p => p[1]), h = w / 2;
    return this.add(`<path d="M${pts.map(p => `${n1(p[0])} ${n1(p[1])}`).join('L')}" fill="none" stroke="${stroke}" stroke-width="${n1(w)}" stroke-linecap="round" stroke-linejoin="round"${op(a)}/>`,
      Math.min(...xs) - h, Math.min(...ys) - h, Math.max(...xs) + h, Math.max(...ys) + h);
  }
  ring(cx, cy, rx, ry, stroke, w, a = 1) { return a > .02 && this.add(`<ellipse cx="${n1(cx)}" cy="${n1(cy)}" rx="${n1(rx)}" ry="${n1(ry)}" fill="none" stroke="${stroke}" stroke-width="${n1(w)}"${op(a)}/>`, cx - rx - w / 2, cy - ry - w / 2, cx + rx + w / 2, cy + ry + w / 2); }
  // A four-pointed sparkle of radius s.
  star(x, y, s, fill, a = 1) {
    const k = s * .28;
    return a > .02 && s > .3 && this.add(`<path d="M${n1(x)} ${n1(y - s)}Q${n1(x + k)} ${n1(y - k)} ${n1(x + s)} ${n1(y)}Q${n1(x + k)} ${n1(y + k)} ${n1(x)} ${n1(y + s)}Q${n1(x - k)} ${n1(y + k)} ${n1(x - s)} ${n1(y)}Q${n1(x - k)} ${n1(y - k)} ${n1(x)} ${n1(y - s)}Z" fill="${fill}"${op(a)}/>`, x - s, y - s, x + s, y + s);
  }
  // A drop of sweat, its round end at (x, y), its tip up (as drawn on the screen).
  drop(x, y, s, a = 1) {
    const d = this.fy ? -1 : 1; // its tip points away from the ground, mirrored or not
    return a > .02 && this.add(`<path d="M${n1(x)} ${n1(y - 2.4 * s * d)}C${n1(x + 1.7 * s)} ${n1(y - .4 * s * d)} ${n1(x + 1.6 * s)} ${n1(y + 1.5 * s * d)} ${n1(x)} ${n1(y + 1.5 * s * d)}C${n1(x - 1.6 * s)} ${n1(y + 1.5 * s * d)} ${n1(x - 1.7 * s)} ${n1(y - .4 * s * d)} ${n1(x)} ${n1(y - 2.4 * s * d)}Z" fill="${COOL[3]}"${op(a)}/>`
      + `<circle cx="${n1(x - .5 * s)}" cy="${n1(y + .3 * s * d)}" r="${n1(.45 * s)}" fill="#fff"${op(a * .9)}/>`, x - 1.7 * s, y - 2.4 * s, x + 1.7 * s, y + 2.4 * s);
  }
  // Text in the app's UI font (heavy), centred on (cx, cy), size cells tall, with a dark edge so it reads on any wallpaper.
  text(s, cx, cy, size, fill, a = 1) {
    const w = textW(s, size);
    return a > .02 && this.add(this.upright(cx, cy, `<text x="${n1(cx)}" y="${n1(cy)}" font-family="-apple-system,BlinkMacSystemFont,'SF Pro Text','Segoe UI',system-ui,sans-serif" font-weight="800" font-size="${n1(size)}" text-anchor="middle" dominant-baseline="central" fill="${fill}" stroke="${INK}" stroke-width="${n1(size * .16)}" stroke-linejoin="round" paint-order="stroke"${op(a)}>${s}</text>`),
      cx - w / 2, cy - size * .55, cx + w / 2, cy + size * .55);
  }
  svg() { const g = this.out.join(''); return this.fy ? `<g transform="matrix(1 0 0 -1 0 ${GH})">${g}</g>` : g; }
}
const op = a => (a < 1 ? ` opacity="${n1(a)}"` : '');
const textW = (s, size) => [...s].length * size * .58 + size * .3;
const textFits = (S, s, cx, cy, size) => { const w = textW(s, size); return S.fits(cx - w / 2, cy - size * .55, cx + w / 2, cy + size * .55); };
const shake = (S, t, t0, dur, amp = 1) => { if (t >= t0 && t < t0 + dur) { const k = Math.floor(t * 40); S.shake = [k % 2 ? amp : -amp, (k >> 1) % 2 ? amp : -amp]; } };
// A light column from the top of the room down to bot, w cells either side of x: soft colour, a white core.
function column(S, x, bot, w, c, a) {
  const o = S.cut(x - w, 0, x + w, bot), i = S.cut(x - w * .3, 0, x + w * .3, bot);
  if (o) S.rect(o[0], o[1], o[2] - o[0], o[3] - o[1], c, a * .45, Math.min(w, 3));
  if (i) S.rect(i[0], i[1], i[2] - i[0], i[3] - i[1], '#fff', a * .7, Math.min(w * .3, 1));
}

// ---- the effects --------------------------------------------------------------------------------------------------
// Each: D seconds long, ox the grid column the pet stands on (the layer is placed so that column is the pet's middle),
// make(o) a fresh run: update(dt, t) moves it, draw(S, t) draws it.
const FX = {
  // A task finished: three XP orbs fly from the pet into an XP bar over its head, each moving it on, with a "+1". A full
  // bar: a beat's pause, a gold light column comes down, a ground wave, a little shake and "LV n".
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
              ob.done = true; fill = Math.min(1, fill + step); pops.push({ x: tx + 4, y: BY - 7, t });
              for (let i = 0; i < 5; i++) sp.push({ x: tx, y: ty, vx: R.range(-18, 18), vy: -R.range(8, 26), life: R.range(.3, .5), max: .5, s: R.range(.8, 1.4), c: WARM[R.chance(.5) ? 2 : 3] });
              if (up && orbs.every(x => x.done) && full < 0) { fill = 1; full = t; lvl = t + .22; }
            }
          }
          shown += (fill - shown) * Math.min(1, dt * 14);
          if (lvl > 0 && t >= lvl + .08 && !burst) { burst = true; for (let i = 0; i < 22; i++) { const a = R.range(-Math.PI, 0), v = R.range(20, 55); sp.push({ x: CX + R.range(-6, 6), y: FEET - R.range(0, 6), vx: Math.cos(a) * v, vy: Math.sin(a) * v, life: R.range(.6, 1.1), max: 1.1, s: R.range(.8, 1.8), c: R.pick(WARM.slice(1, 5)) }); } }
          stepList(sp, dt, 1.6, 22);
        },
        draw(S, t) {
          const T = lvl > 0 ? t - lvl - .08 : -1;
          if (T >= 0 && T < .9) column(S, CX, T < .06 ? FEET * (T / .06) : FEET, 9 * (1 - smooth(.2, .9, T)), WARM[3], 1 - smooth(.3, .9, T));
          if (T >= 0 && T < 1) { // the ground wave, as wide as the room lets it
            const r = S.room, reach = Math.min(4 + 22 * outExpo(T / .55), r ? Math.min(CX - r[0], r[2] - CX) - 1 : GW);
            S.ring(CX, FEET, reach, Math.min(1.6, reach * .18), WARM[3], .8, 1 - T);
            shake(S, T, 0, .25, 1);
          }
          const barA = T >= 0 ? 1 - smooth(.1, .5, T) : up ? 1 : 1 - smooth(1.6, 2.05, t);
          // The bar, its label, the "+1"s and "LV n" are drawn whole or not at all: where the room is too tight (a packed
          // row, the top edge over the name tag) the orbs just fly off.
          if (barA > .05 && S.fits(BX - 7, BY - .5, BX + BW + .5, BY + 5)) {
            S.rect(BX, BY + .5, BW, 4, INK, barA * .35, 2); S.rect(BX, BY, BW, 4, '#fff', barA * .95, 2); // a white track with a soft drop: it reads on any wallpaper
            S.rect(BX + .7, BY + .7, Math.max(0, (BW - 1.4) * shown), 2.6, WARM[4], barA, 1.3);
            S.rect(BX + 1.3, BY + .9, Math.max(0, (BW - 2.6) * shown), .8, WARM[2], barA, .4);
            S.text('XP', BX - 3.6, BY + 2, 4, WARM[3], barA);
            if (full > 0 && t - full < .25) S.rect(BX - .6, BY - .6, BW + 1.2, 5.2, WARM[2], .5 * (1 - (t - full) / .25), 2.6);
          }
          for (const ob of orbs) {
            if (t < ob.t0 || ob.done) continue;
            S.line(ob.tr, WARM[3], 1.3, .45);
            S.circle(ob.x, ob.y, 1.7, WARM[4]); S.circle(ob.x - .4, ob.y - .4, .7, WARM[1]);
          }
          for (const pp of pops) { const k = (t - pp.t) / .7, y = pp.y - 6 * outCubic(k); if (k < 1 && textFits(S, '+1', pp.x, y, 4.5)) S.text('+1', pp.x, y, 4.5, WARM[3], 1 - smooth(.6, 1, k)); }
          for (const s of sp) S.star(s.x, s.y, s.s * Math.min(1, s.life / s.max * 3), s.c);
          if (T >= .1) {
            const k = outBack(clamp((T - .1) / .25), 2.5), a = 1 - smooth(2.2, 2.9, T), s = `LV ${o.lv}`, y = TOP - 16 - LIFT + (1 - k) * 6;
            const z = [k > .6 ? 7 : 5, 5].find(z => textFits(S, s, CX, y, z)); if (z) S.text(s, CX, y, z, WARM[3], a);
          }
        },
      };
    },
  },
  // Context under 25%: a battery over its head is down to one blinking red bar, with a "!", and sweat drops run off its
  // head and splash on the ground.
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
        draw(S, t) {
          const X0 = CX - 8, Y0 = TOP - 13 - (o.flipY ? 9 : 0), on = (t % .5) < .28; // the battery whole or not at all
          if (S.fits(X0 - .6, Y0 - .6, X0 + 17.6, Y0 + 9.6)) {
            S.rect(X0 - .6, Y0 - .6, 16.2, 10.2, '#fff', .85, 3); S.rect(X0 + 14.4, Y0 + 2.4, 3.2, 4.2, '#fff', .85, 1.2); // a light rim: it reads on a dark wallpaper too
            S.rect(X0, Y0, 15, 9, INK, 1, 2.4); S.rect(X0 + 1, Y0 + 1, 13, 7, '#fff', 1, 1.6); S.rect(X0 + 15, Y0 + 3, 2, 3, INK, 1, .8);
            S.rect(X0 + 2.2, Y0 + 2.2, 3, 4.6, on ? RED[3] : RED[2], 1, 1);
            if (textFits(S, '!', X0 + 21, Y0 + 4.5, 7)) S.text('!', X0 + 21, Y0 + 4.5, 7, RED[3], on ? 1 : .45);
          }
          for (const d of drops) if (t >= d.t0 && !d.done) S.drop(d.x, d.y, 1);
          for (const s of splash) S.circle(s.x, s.y, .6, COOL[s.life / s.max > .5 ? 2 : 3]);
        },
      };
    },
  },
  // The session ended: the pet melts from the feet up (pets.html, a CSS clip on its SVG) into rising sparks of its colour;
  // a thin light column flares and fades, leaving one twinkle. o.tint: the pet's colour.
  end: {
    D: 2.6, ox: CX,
    make(o) {
      const R = new Rng(71), parts = [], S0 = .7, E = 1.4, rows = 16, tint = o.tint || COOL[3]; let made = 0;
      return {
        update(dt, t) {
          if (t >= S0 && t < E) { const cut = Math.floor(rows * (1 - (t - S0) / (E - S0))); while (made < rows - cut) { const y = FEET - 1 - made; for (let i = 0; i < 3; i++) parts.push({ x: CX + R.range(-7, 7), y, vx: R.range(-6, 6), vy: -R.range(18, 40), life: R.range(.6, 1.1), max: 1.1, c: R.chance(.6) ? tint : '#fff' }); made++; } }
          stepList(parts, dt, .8, -14);
        },
        draw(S, t) {
          if (t > S0 - .05 && t < E + .5) { const a = t < S0 + .1 ? (t - S0 + .05) / .15 : 1 - smooth(E, E + .5, t); column(S, CX, FEET, 2.5, COOL[2], a); }
          for (const q of parts) { const k = q.life / q.max; S.circle(q.x, q.y, .5 + .5 * k, k > .3 ? q.c : COOL[2], Math.min(1, k * 1.6)); }
          if (t > E + .1 && t < 2.4) { const L = 5 * Math.sin(clamp((t - E - .1) / 1) * Math.PI); S.star(CX, TOP - 6, L, COOL[2]); S.star(CX, TOP - 6, L * .45, '#fff'); }
        },
      };
    },
  },
};

// A pet's status balloon ("!" waiting, tick done): only without effects; with them the ring, the badge and the XP tell
// the state.
const statusBalloon = (fxOn, kind) => (fxOn ? '' : kind);

// ---- staying off other things ---------------------------------------------------------------------------------
// Where the effect layer sits in its pet's sprite box (size px square, before the box's own scale): the pet's middle on
// column ox, its feet on the box's bottom (mirrored top-down as the effect is).
function fxPlace(size, ox, flipY) { // ox: the run's (fxRun(...).ox)
  const c = size / 18;
  return { left: size / 2 - ox * c, top: flipY ? -(GH - FEET) * c : size - FEET * c, width: GW * c, height: GH * c };
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
// One effect playing on one pet: a fixed 60 Hz simulation, drawn when the overlay draws (at most ~30 times a second)
// into a Scene; done once it has played.
function fxRun(kind, o) {
  const def = FX[kind], fx = def.make(o);
  let k = 0, acc = 0;
  const N = Math.round((fx.D ?? def.D) / SIM);
  return {
    kind, ox: def.ox, done: false, get total() { return k * SIM; },
    step(dt) {
      acc += Math.min(.1, dt);
      while (acc >= SIM && !this.done) { acc -= SIM; if (++k > N) { this.done = true; break; } fx.update(SIM, k * SIM); }
    },
    draw(room = o.room) { const S = new Scene(room || null, !!o.flipY); fx.draw(S, k * SIM); return S; },
  };
}
