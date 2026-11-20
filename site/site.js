// Bobix site: pets on the screen edge, demos and the install tabs. Strings come from data-* attributes.
// Every string the page shows comes from data-* attributes (this script tag, the buttons), so the
// Vietnamese page needs no change here.
const T = document.currentScript.dataset;
const reduce = matchMedia('(prefers-reduced-motion: reduce)');

// Interruptible spring: always integrates from the live value and velocity.
// damping = ratio (1 = no overshoot), response = seconds (Apple's terms).
function spring(get, set, { damping = 1, response = 0.35 } = {}) {
  let x = get(), v = 0, target = x, raf = 0, last = 0;
  const k = (2 * Math.PI / response) ** 2, c = 4 * Math.PI * damping / response;
  function tick(t) {
    const dt = Math.min(0.032, (t - last) / 1000 || 0.016); last = t;
    for (let i = 0; i < 4; i++) { const h = dt / 4; v += (-k * (x - target) - c * v) * h; x += v * h; }
    set(x, v);
    if (Math.abs(v) < 0.5 && Math.abs(x - target) < 0.1) { x = target; v = 0; set(x, 0); raf = 0; return; }
    raf = requestAnimationFrame(tick);
  }
  return {
    to(t, vel) { target = t; if (vel !== undefined) v = vel;
      if (reduce.matches) { cancelAnimationFrame(raf); raf = 0; x = t; v = 0; set(x); return; }
      if (!raf) { last = performance.now(); raf = requestAnimationFrame(tick); } },
    jump(t) { cancelAnimationFrame(raf); raf = 0; x = target = t; v = 0; set(x, 0); },
    get value() { return x; }, get velocity() { return v; },
    grab() { cancelAnimationFrame(raf); raf = 0; return x; },
  };
}

// Segmented control: thumb springs from wherever it is, so rapid taps never jump.
function segmented(seg, onChange) {
  const thumb = seg.querySelector('.thumb'), btns = [...seg.querySelectorAll('button')];
  let w = 0;
  const pos = spring(() => 0, x => thumb.style.transform = `translateX(${x}px)`, { response: 0.32 });
  const wid = spring(() => 0, x => thumb.style.width = x + 'px', { response: 0.32 });
  function select(i, focus) {
    btns.forEach((b, j) => { b.setAttribute('aria-selected', i === j); b.tabIndex = i === j ? 0 : -1; });
    if (focus) btns[i].focus();
    pos.to(btns[i].offsetLeft); wid.to(btns[i].offsetWidth); onChange(i);
  }
  btns.forEach((b, i) => {
    b.addEventListener('pointerdown', () => select(i));          // respond on press
    b.addEventListener('click', () => select(i));                 // keyboard / AT
    b.addEventListener('keydown', e => {
      const d = { ArrowRight: 1, ArrowLeft: -1 }[e.key]; if (!d) return;
      e.preventDefault(); select((i + d + btns.length) % btns.length, true);
    });
  });
  const init = () => { const b = btns.find(b => b.getAttribute('aria-selected') === 'true'); pos.jump(b.offsetLeft); wid.jump(b.offsetWidth); };
  init(); addEventListener('resize', init); document.fonts?.ready.then(init);
}

const pets = document.querySelectorAll('.stage .pet'), cap = document.getElementById('cap');
const stateTabs = [...document.querySelectorAll('#pets .seg button')];
segmented(document.querySelector('#pets .seg'), i => { pets.forEach((p, j) => p.classList.toggle('on', i === j)); cap.textContent = stateTabs[i].dataset.cap; });

// Install: open the tab for the visitor's computer when it is easy to tell (Windows, Linux).
const panels = [...document.querySelectorAll('#install .panel')], osTabs = [...document.querySelectorAll('#osseg button')];
const showOs = i => panels.forEach((p, j) => p.hidden = i !== j);
const plat = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '';
const guess = /win/i.test(plat) ? 1 : /linux/i.test(plat) && !/android/i.test(navigator.userAgent) ? 2 : 0;
osTabs.forEach((b, j) => { b.setAttribute('aria-selected', j === guess); b.tabIndex = j === guess ? 0 : -1; });
showOs(guess);
segmented(document.getElementById('osseg'), showOs);

document.querySelectorAll('.copy').forEach(b => b.addEventListener('click', async () => {
  try { await navigator.clipboard.writeText(b.previousElementSibling.textContent); b.textContent = T.copied; }
  catch { b.textContent = T.fail; }
  setTimeout(() => { b.textContent = T.copy; }, 1500);
}));

// Pets on the screen edge: they wander as you scroll, follow a drag 1:1, rubber-band at the
// window edges, and a flick projects where they land; the spring inherits the throw velocity.
const rubber = (over, dim) => (1 - 1 / (over * 0.55 / dim + 1)) * dim;
let walkers = [];
walkers = [...document.querySelectorAll('.walker')].map((el, i) => {
  const p = { el, home: [0.14, 0.52, 0.84][i], speed: [0.9, -0.6, 0.45][i], dragging: false };
  p.s = spring(() => 0, (x, v) => {
    el.style.transform = `translateX(${soft(x)}px)`;
    if (Math.abs(v) > 20) el.classList.toggle('left', v < 0);
    el.classList.toggle('idle', Math.abs(v) < 20 && !p.dragging);
  }, { damping: 1, response: 0.9 });
  return p;
});
const span = () => innerWidth - walkers[0].el.offsetWidth;
const WALL = 24;   // how far a pet may stretch past the window edge, so it never leaves the screen
function soft(x) { const max = span(); return x < 0 ? -rubber(-x, WALL) : x > max ? max + rubber(x - max, WALL) : x; }
function wanderTarget(p) {
  const prog = scrollY / Math.max(1, document.documentElement.scrollHeight - innerHeight);
  const t = p.home + Math.sin(prog * Math.PI * 2 * Math.abs(p.speed) + p.home * 6) * 0.12 * Math.sign(p.speed);
  return Math.max(0, Math.min(span(), t * span()));
}
const follow = () => walkers.forEach(p => { if (!p.dragging) p.s.to(wanderTarget(p)); });
walkers.forEach(p => p.s.jump(wanderTarget(p)));
addEventListener('scroll', follow, { passive: true });
addEventListener('resize', follow);

function say(p, text, ms = 1800) {
  const b = p.el.querySelector('.bubble'); b.textContent = text; b.style.marginLeft = '0px';
  const r = b.getBoundingClientRect(), shift = Math.max(8 - r.left, Math.min(0, innerWidth - 8 - r.right));
  b.style.marginLeft = shift + 'px'; p.el.classList.add('talk');
  clearTimeout(p.talkT); p.talkT = setTimeout(() => p.el.classList.remove('talk'), ms);
}
walkers.forEach(p => {
  const el = p.el; let ox = 0, hist = [], moved = 0;
  el.addEventListener('pointerdown', e => {
    el.setPointerCapture(e.pointerId); p.dragging = true; moved = 0; el.classList.add('held');
    ox = e.clientX - Math.max(0, Math.min(span(), p.s.grab())); hist = [{ x: e.clientX, t: e.timeStamp }];
  });
  el.addEventListener('pointermove', e => {
    if (!el.hasPointerCapture(e.pointerId)) return;
    const x = e.clientX - ox;
    moved = Math.max(moved, Math.abs(e.clientX - hist[0].x));
    el.classList.toggle('left', e.clientX < hist[hist.length - 1].x);
    p.s.jump(x); hist.push({ x: e.clientX, t: e.timeStamp }); if (hist.length > 5) hist.shift();
  });
  const up = e => {
    if (!el.hasPointerCapture(e.pointerId)) return; el.releasePointerCapture(e.pointerId); el.classList.remove('held');
    const a = hist[0], b = hist[hist.length - 1], dt = (b.t - a.t) / 1000;
    const raw = dt > 0 && e.timeStamp - b.t < 80 ? (b.x - a.x) / dt : 0;
    const v = Math.max(-3000, Math.min(3000, raw));   // a synthetic or wild flick must not fling it off screen
    const max = span(), land = Math.max(0, Math.min(max, p.s.value + (v / 1000) * 0.998 / (1 - 0.998)));
    p.home = land / max - (wanderTarget(p) / max - p.home);   // it stays where it lands
    p.s.to(land, v); p.dragging = false;
    if (moved < 4) say(p, p.state === 'needs' ? T.sayAsk.replace('{name}', el.dataset.name) : el.dataset.name);
  };
  el.addEventListener('pointerup', up); el.addEventListener('pointercancel', up);
});

// Pet A follows the approval demo; the header's live chip mirrors it.
const A = walkers[0], live = document.getElementById('live');
function setState(s) {
  A.state = s; A.el.querySelectorAll('[data-s]').forEach(n => n.classList.toggle('on', n.dataset.s === s));
  live.classList.toggle('needs', s === 'needs');
  live.lastElementChild.textContent = s === 'needs' ? T.liveNeeds : T.live;
}
setState('working');
let answered = false;
new IntersectionObserver(es => es.forEach(e => {
  if (e.isIntersecting && !answered && A.state !== 'needs') { setState('needs'); say(A, T.sayNeeds, 2200); }
}), { threshold: 0.4 }).observe(document.getElementById('approvals'));
(() => {
  const done = document.getElementById('done'); let t;
  document.querySelectorAll('#choices .choice').forEach(b => b.addEventListener('click', () => {
    answered = true; done.textContent = b.dataset.r; clearTimeout(t); t = setTimeout(() => done.textContent = '', 2600);
    setState('done'); say(A, b.classList.contains('n') ? T.sayDeny : T.sayThanks);
    clearTimeout(A.doneT); A.doneT = setTimeout(() => setState('working'), 2600);
  }));
})();
const nav = document.getElementById('nav');
addEventListener('scroll', () => {
  nav.classList.toggle('scrolled', scrollY > 40);
  const nearEnd = scrollY + innerHeight > document.documentElement.scrollHeight - 200;
  if (nearEnd && A.state === 'working') setState('resting');
  else if (!nearEnd && A.state === 'resting') setState('working');
}, { passive: true });

// Launcher mock: types a query once, then the highlight springs to the hovered or arrowed row.
(() => {
  const box = document.getElementById('launcher'), rows = [...box.querySelectorAll('.lrow')], hi = box.querySelector('.lhi');
  const y = spring(() => 0, v => hi.style.transform = `translateY(${v}px)`, { response: 0.28 });
  let i = 0;
  const pick = n => { i = (n + rows.length) % rows.length; rows.forEach((r, j) => r.classList.toggle('on', j === i)); y.to(rows[i].offsetTop); };
  rows.forEach((r, j) => r.addEventListener('pointerenter', () => pick(j)));
  box.tabIndex = 0;
  box.addEventListener('keydown', e => { const d = { ArrowDown: 1, ArrowUp: -1 }[e.key]; if (d) { e.preventDefault(); pick(i + d); } });
  pick(0); y.jump(0);
  const typed = document.getElementById('ltyped'), q = T.query;
  new IntersectionObserver((es, o) => es.forEach(e => {
    if (!e.isIntersecting) return; o.disconnect();
    if (reduce.matches) { typed.textContent = q; return; }
    let k = 0; const tick = () => { typed.textContent = q.slice(0, ++k); if (k < q.length) setTimeout(tick, 110); };
    setTimeout(tick, 400);
  }), { threshold: 0.6 }).observe(box);
})();

// Reveal on scroll + gentle hero screenshot settle tied to scroll position.
(() => {
  const io = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); } }), { rootMargin: '0px 0px -8% 0px' });
  document.querySelectorAll('.rv').forEach(n => io.observe(n));
  const dev = document.getElementById('device');
  const f = () => { if (reduce.matches) return; const p = Math.min(1, scrollY / 500); dev.style.transform = `perspective(1600px) rotateX(${(1 - p) * 8}deg) scale(${0.94 + p * 0.06})`; };
  addEventListener('scroll', () => requestAnimationFrame(f), { passive: true }); f();
})();
