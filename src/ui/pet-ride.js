// "Show when needed" (pets.html): a pet that needs you hangs under a flat party balloon (creature.js balloonParts) and
// crosses the screen. Drawn as SVG in the pet's own units (its box is 36 × 36, its feet on the box's bottom), the pet
// itself being its own SVG, acting its state. The balloon drifts on a small loop and the pet hangs under it a beat
// behind; the exits (popped when answered, let go when done, sinking when context ran out) last about 1.4 s.
// Needs creature.js (balloonParts, n1, poseAt). Exposes window.PetRide.
(() => {
  const CYCLE = 2.4, FPS = 30, TAU = Math.PI * 2;
  const ABOVE = 60 / 56; // room the balloon takes above the pet's box, in pet boxes
  const BS = 2.4;        // the balloon's scale: its 11-unit box drawn 26 units wide
  const CONFETTI = ['#ff7a4d', '#34c38f', '#ffd862', '#62c8f4', '#b98af0'];
  const STATE = { waiting: '#ff7a4d', done: '#34c38f', ctx: '#f6c445' };
  const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
  const smooth = (a, b, x) => { const t = clamp((x - a) / (b - a)); return t * t * (3 - 2 * t); };
  const outCubic = t => 1 - (1 - clamp(t)) ** 3;
  const op = a => (a < 1 ? ` opacity="${n1(Math.max(0, a))}"` : '');
  class Rng {
    constructor(s = 1) { this.s = s >>> 0 || 1; }
    next() { let t = (this.s = (this.s + 0x6d2b79f5) >>> 0); t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }
    range(a, b) { return a + (b - a) * this.next(); } pick(a) { return a[Math.floor(this.next() * a.length)]; }
  }

  // ---- the parts ----
  // The balloon, its knot at (kx, ky). sc: size (shrinks as context runs out), sq: squash (+ wide), flash: turns white.
  function balloon(B, kx, ky, o = {}) {
    const sc = BS * (o.sc ?? 1), sq = o.sq ?? 0, sx = sc * (1 + sq), sy = sc * (1 - sq);
    const body = o.flash ? B.body.replace(/fill="#(?!fff)[0-9a-f]{6}"/g, 'fill="#ffffff"') : B.body;
    return `<g transform="translate(${n1(kx)} ${n1(ky)}) scale(${n1(sx)} ${n1(sy)}) translate(-5.5 -13.6)"${op(o.a ?? 1)}>${body}${B.icon(5.5, 6.4)}</g>`;
  }
  // Its string, from the knot (kx, ky) to (x1, y1), with a soft bend.
  const string = (kx, ky, x1, y1, bend = 0, a = 1) => `<path d="M${n1(kx)} ${n1(ky)}Q${n1((kx + x1) / 2 + 2 + bend)} ${n1((ky + y1) / 2)} ${n1(x1)} ${n1(y1)}" fill="none" stroke="#868b99" stroke-width=".8" stroke-linecap="round"${op(a)}/>`;
  // The pet, its box's bottom middle at (x, feet).
  const pet = (L, x, feet, pose, a = 1) => `<g transform="translate(${n1(x - 18)} ${n1(feet - 36)})"${op(a)}>${L.pet(pose).replace('width="100%" height="100%"', 'width="36" height="36"')}</g>`;
  // A little parachute whose canopy bottom is at y, its lines to the pet's shoulders.
  function chute(x, y, open, c, a) {
    if (open <= .05) return '';
    const w = 11 * open, h = 7 * open;
    return `<g${op(a)}><path d="M${n1(x - w)} ${n1(y)}L${n1(x - 8)} ${n1(y + 17)}M${n1(x + w)} ${n1(y)}L${n1(x + 8)} ${n1(y + 17)}" stroke="#868b99" stroke-width=".7" fill="none"/>`
      + `<path d="M${n1(x - w)} ${n1(y)}Q${n1(x - w)} ${n1(y - h * 1.6)} ${n1(x)} ${n1(y - h * 1.6)}Q${n1(x + w)} ${n1(y - h * 1.6)} ${n1(x + w)} ${n1(y)}Q${n1(x)} ${n1(y - h * .5)} ${n1(x - w)} ${n1(y)}Z" fill="${c}"/>`
      + `<path d="M${n1(x - w * .35)} ${n1(y - h * .3)}Q${n1(x)} ${n1(y - h * 1.9)} ${n1(x + w * .35)} ${n1(y - h * .3)}" fill="#fff" opacity=".5"/></g>`;
  }
  // One frame of a pet under its balloon, ph = phase of the floating loop: the balloon drifts on a small ellipse, the
  // pet hangs under it a beat behind; k = how much air is gone (0–4). The pet's box bottom is at (18, 36).
  function pose(L, o = {}) {
    const ph = o.ph ?? 0, k = o.k ?? 0, dy = k * 1.5 + (o.dy ?? 0), sc = (1 - .1 * k) * (o.sc ?? 1);
    const kx = 18 + 2.5 * Math.cos(ph), ky = -5 + 1.8 * Math.sin(ph) + dy;
    const px = 18 + 1.3 * Math.cos(ph - .7), feet = 36 + Math.sin(ph - .5) + dy;
    return { kx, ky, px, feet, svg: string(kx, ky, px, feet - 31, 0, o.a ?? 1) + balloon(L.B, kx, ky, { sc, sq: o.sq, flash: o.flash, a: o.a }) };
  }

  // ---- particles ----
  function burst(list, rng, x, y, n, o) {
    for (let i = 0; i < n; i++) {
      const ang = o.ang != null ? o.ang + rng.range(-o.spread, o.spread) : rng.range(0, TAU), v = rng.range(o.v * .35, o.v), r0 = o.r0 ?? 0;
      list.push({ x: x + Math.cos(ang) * r0, y: y + Math.sin(ang) * r0, vx: Math.cos(ang) * v, vy: Math.sin(ang) * v, c: rng.pick(o.cols), life: rng.range(o.life * .6, o.life), age: 0,
        g: o.g ?? 160, drag: o.drag ?? 2.5, puff: o.puff, r: o.r ?? 1, rot: rng.range(0, 180) });
    }
  }
  function parts(list, dt) {
    let s = '';
    for (let i = list.length - 1; i >= 0; i--) {
      const p = list[i]; p.age += dt; if (p.age >= p.life) { list.splice(i, 1); continue; }
      const k = Math.exp(-p.drag * dt); p.vx *= k; p.vy = p.vy * k + p.g * dt; p.x += p.vx * dt; p.y += p.vy * dt;
      const u = p.age / p.life, a = 1 - smooth(.55, 1, u);
      s += p.puff ? `<circle cx="${n1(p.x)}" cy="${n1(p.y)}" r="${n1(p.r * (.6 + u * .9))}" fill="#fff" stroke="#d5d9e2" stroke-width=".5"${op(a)}/>` // a soft cloud
        : `<rect x="${n1(p.x - .9)}" y="${n1(p.y - .5)}" width="1.8" height="1" rx=".4" fill="${p.c}" transform="rotate(${n1(p.rot + p.age * 400)} ${n1(p.x)} ${n1(p.y)})"${op(a)}/>`;
    }
    return s;
  }
  const ring = (x, y, r, c, a) => `<circle cx="${n1(x)}" cy="${n1(y)}" r="${n1(r)}" fill="none" stroke="${c}" stroke-width="1.2"${op(a)}/>`;

  // ---- exits: each returns { D, draw(t, dt) → svg } ----
  const EXIT = {
    // Answered: the balloon stretches, flashes and pops into confetti; the pet floats down on a tiny parachute.
    pop: c => { const D = 1.3, rng = new Rng(c.seed), ps = []; let b = null;
      return { D, draw(t, dt) {
        const ph = c.ph0 + TAU * Math.min(t, .1) / CYCLE;
        if (t < .1 || !b) { b = pose(c.L, { ph, k: c.k, sq: .16 * t / .1, sc: 1 + .08 * t / .1, flash: t > .055 }); return b.svg + pet(c.L, b.px, b.feet, c.pose(t)); }
        const by = b.ky - 6.4 * BS;
        if (!ps.done) { ps.done = true; burst(ps, rng, b.kx, by, 30, { v: 90, cols: CONFETTI, life: 1.1, g: 150, drag: 2.2, r0: 4 }); }
        const u = t - .1, open = smooth(.04, .24, u), fall = u < .18 ? 120 * u * u : 4 + 16 * (u - .18), a = 1 - smooth(.85, 1.3, t);
        const px = b.px + 2 * Math.sin(u * 6) * open, feet = b.feet + fall;
        return chute(px, feet - 42, open, c.L.c, a) + pet(c.L, px, feet, c.pose(t), a)
          + (u < .28 ? ring(b.kx, by, 3 + 18 * outCubic(u / .28), u < .08 ? '#fff' : c.L.c, 1 - u / .28) : '') + parts(ps, dt);
      } }; },
    // Done and dismissed: the pet waves, lets go, the balloon floats off the top of the screen, the pet goes in a puff.
    release: c => { const D = 1.4, rng = new Rng(c.seed), ps = []; let b = null;
      return { D, draw(t, dt) {
        const wave = ['wave', Math.floor(t / .15) % 4];
        if (t < .55 || !b) { b = pose(c.L, { ph: c.ph0 + TAU * t / CYCLE, k: c.k }); return b.svg + pet(c.L, b.px, b.feet, wave); }
        if (!ps.done) { ps.done = true; c.free(b.kx, b.ky); burst(ps, rng, b.px, b.feet - 14, 8, { v: 34, cols: ['#34c38f', '#ffd862'], life: .6, g: -40, drag: 3, r0: 8 }); }
        const u = t - .55, a = 1 - smooth(.8, 1.3, t);
        if (t > .8 && !ps.puffed) { ps.puffed = true; burst(ps, rng, b.px, b.feet - 10, 7, { v: 22, cols: ['#fff'], life: .5, g: -26, drag: 4, puff: true, r: 2.4, r0: 7 }); }
        return pet(c.L, b.px, b.feet + 4 * outCubic(u / .25), wave, a) + parts(ps, dt);
      } }; },
    // Low context, dismissed: the balloon keeps leaking, pet and balloon sink and fade.
    sink: c => { const D = 1.4, rng = new Rng(c.seed), ps = [];
      return { D, draw(t, dt) {
        const a = 1 - smooth(.6, 1.4, t), b = pose(c.L, { ph: c.ph0 + TAU * t / CYCLE, k: 4, sc: 1 - .45 * smooth(0, D, t), dy: 26 * (t / D) ** 2, a });
        if (Math.floor(t * 30) % 3 === 0 && t < 1) burst(ps, rng, b.kx, b.ky, 1, { ang: Math.PI / 2, spread: .8, v: 20, cols: ['#fff'], life: .3, g: 0, drag: 3, puff: true, r: .8 });
        return b.svg + pet(c.L, b.px, b.feet, c.pose(t), a) + parts(ps, dt);
      } }; },
  };
  const EXIT_OF = { waiting: 'pop', done: 'release', ctx: 'sink' };

  // A pet's look under its balloon: pet(pose) its SVG acting that pose ([pose, frame], or null for its state's own),
  // st its state, the need's colour and balloon.
  function look(petOf, st, need, key) {
    return { pet: petOf, st, c: STATE[need] || STATE.waiting, B: balloonParts(need), key: `${key}|${need}` };
  }

  // ---- balloons let go of: drawn once, then only moved up and off the screen ----
  const FREES = [];
  function freeBalloon(layer, x, y, B, U) { // x, y: where the knot is on screen; U: CSS px a unit
    const el = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    el.setAttribute('class', 'free'); el.setAttribute('viewBox', '-20 -40 40 64');
    Object.assign(el.style, { width: `${40 * U}px`, height: `${64 * U}px` });
    el.innerHTML = string(0, 0, -2, 20, 2) + balloon(B, 0, 0);
    layer.prepend(el); // behind every pet
    FREES.push({ el, x: x - 20 * U, y: y - 40 * U, t0: performance.now() });
  }
  function freesTick(now) {
    for (let i = FREES.length - 1; i >= 0; i--) {
      const f = FREES[i], t = (now - f.t0) / 1000, y = f.y - (50 * t + 300 * t * t), x = f.x + 6 * Math.sin(t * 3);
      f.el.style.transform = `translate(${Math.round(x)}px,${Math.round(y)}px)`;
      if (y < -140 || t > 4) { f.el.remove(); FREES.splice(i, 1); }
    }
    return FREES.length;
  }

  // ---- one pet's ride ----
  // size: the pet's box in CSS px; host: { origin() → [x, y] of the pet's box on screen, layer }.
  class Ride {
    constructor(L, size, host, seed) {
      Object.assign(this, { L, U: size / 36, host, seed, k: 0, run: null, frozen: false, t0: performance.now(), shown: '' });
      this.el = Object.assign(document.createElement('div'), { className: 'fly' });
      this.svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      this.svg.setAttribute('viewBox', '0 0 36 36'); this.svg.setAttribute('aria-hidden', 'true');
      Object.assign(this.svg.style, { width: `${size}px`, height: `${size}px` });
      this.el.append(this.svg);
      this.tick(this.t0, 0);
    }
    // Its pose t seconds into its ride (frozen: the first frame).
    pose(t) { return this.frozen ? null : poseAt(this.L.st, t * 1000); }
    // Starts the exit for this need (answered, or done / low context dismissed).
    leave(need) {
      const U = this.U, c = { L: this.L, k: this.k, seed: this.seed, ph0: this.phase(performance.now()), pose: t => this.pose(t),
        free: (kx, ky) => { const [x, y] = this.host.origin(); freeBalloon(this.host.layer, x + kx * U, y + ky * U, this.L.B, U); } };
      this.run = EXIT[EXIT_OF[need] || 'pop'](c); this.rt = performance.now();
    }
    phase(now) { return this.frozen ? 0 : TAU * (Math.floor(((now - this.t0) / 1000 % CYCLE) * FPS) / FPS) / CYCLE; }
    setLevel(k) { if (k > this.k) { this.k = k; this.shown = ''; } }
    // Every overlay frame while the pet is shown; 'end' once its exit has played.
    tick(now, dt) {
      let svg, end = false;
      if (this.run) { const r = this.run, t = Math.min((now - this.rt) / 1000, r.D); svg = r.draw(t, dt); end = t >= r.D; }
      else { const b = pose(this.L, { ph: this.phase(now), k: this.k }); svg = b.svg + pet(this.L, b.px, b.feet, this.pose((now - this.t0) / 1000)); }
      if (svg !== this.shown) { this.shown = svg; this.svg.innerHTML = svg; }
      return end ? 'end' : null;
    }
  }
  window.PetRide = { Ride, look, freesTick, above: size => ABOVE * size };
})();
