// "Show when needed" (settings: petMode "ondemand"), the other way pets.html shows pets. Nothing walks along the edge:
// only a session that needs you (waiting for your approval, a task just done, context running low) shows, as its pet
// hanging under a balloon (pet-ride.js) that rises from below the bottom of the screen to past the top, again and again
// until the need is over. It never stops in the middle of the screen, except while you point at it: then it waits and
// shows its name and its card (with Allow / Deny while it waits). A pill in the corner counts the sessions and lists
// them, with Allow / Deny; "do not disturb" hides every pet and the pill says so, with a button to end it.
// Runs on pets.html's globals (cfg, T, esc, bridge, look, statusText, answerButtons, mouse, lastList, ...).
const OD = { pets: new Map(), seen: new Set(), answered: new Set(), lastStart: 0, ui: null, popKey: '', pillKey: '' };
const OD_CROSS = 10;   // seconds to rise from below the bottom edge to past the top
const OD_SWAY = 6;     // sideways sway, px each way (a 4 s swing)
const OD_STAGGER = 1.6; // seconds between two pets that start at once
const OD_GAP = 12;     // space kept between two pets
const OD_CTX = 25;     // context below this (%) is a need
const OD_SHOW = { ctx: 12e3, done: 6e3 }; // low context shows this long; with reduced motion, a done task too
const odOn = () => cfg.petMode === 'ondemand';
const dndOn = () => (cfg.dndUntil || 0) > Date.now();

// What a session needs from you now, if anything. A finished task floats once; low context once until it is back up.
function odNeed(s, now) {
  const st = look(s);
  if (st === 'waiting') return s.answer && OD.answered.has(s.answer.id) ? null : 'waiting';
  if (st === 'done' && now - s.since < DONE_BUBBLE && !OD.seen.has(`${s.id}|done|${s.since}`)) return 'done';
  if (s.ctx != null && s.ctx < OD_CTX && !OD.seen.has(`${s.id}|ctx`)) return 'ctx';
  return null;
}

function odSync(list) {
  odUI();
  const now = Date.now(), here = new Set();
  if (dndOn()) { for (const p of [...OD.pets.values()]) odRemove(p, false); odPill(); return; } // back after, if still needed
  for (const s of list) {
    if (s.ctx == null || s.ctx >= OD_CTX) OD.seen.delete(`${s.id}|ctx`);
    if (s.pet === 'hidden') continue;
    const need = odNeed(s, now), p = OD.pets.get(s.id);
    if (p) { p.s = s; here.add(s.id); }
    if (p?.leaving) continue;
    if (need && !p) { odMake(s, need); here.add(s.id); }
    else if (p && need !== p.need) {
      if (need) odRetarget(p, need);                      // a pet that was done asks for approval, ...
      else if (p.need === 'waiting') odLeave(p, 'waiting'); // answered: here, on the Board or in the terminal
      else p.last = true;                                   // this crossing is its last
    } else if (p) odPaint(p);
  }
  for (const p of OD.pets.values()) if (!here.has(p.s.id) && !p.leaving) p.last = true; // ended, or hidden
  odPill();
}

// ---------- a pet under its balloon ----------
function odRide(p) {
  const s = p.s, st = look(s), color = petColor(s, lastList || []), art = (s.parent && byId(s.parent)) || s;
  p.R?.el.remove();
  p.R = new PetRide.Ride(PetRide.look(pose => petArt(art, st, color, pose), st, p.need, `${art.project || art.id}|${art.image || ''}|${st}|${color}`), cfg.size,
    { origin: () => [p.x, p.y], layer: document.body }, hashStr(s.id + s.since));
  p.R.frozen = !FX_ON;
  p.el.querySelector('.spr')?.append(p.R.el);
}
function odMake(s, need) {
  const el = document.createElement('div');
  el.className = 'pet od';
  document.body.appendChild(el);
  const p = { el, s, need, x: 0, y: 0, swing: 0, hover: false, t0: Date.now() };
  OD.pets.set(s.id, p);
  odPaint(p);
  odRide(p);
  el.onpointerdown = e => odPress(p, e);
  if (FX_ON) { p.cross = true; odLap(p, true); }
  else { odRest(p); el.classList.add('gone'); requestAnimationFrame(() => el.classList.remove('gone')); } // reduced motion: it fades in near the bottom edge
  odUntil(p);
  wake();
}
function odRetarget(p, need) { p.need = need; p.last = false; p.t0 = Date.now(); p.key = null; odUntil(p); odPaint(p); odRide(p); }
function odUntil(p) { p.until = p.need === 'ctx' || (!FX_ON && p.need === 'done') ? Date.now() + OD_SHOW[p.need] : Infinity; }
// It goes: answered, its balloon pops; dismissed, it lets go (done) or sinks (low context); with reduced motion it fades.
function odLeave(p, need) {
  if (p.leaving) return;
  p.leaving = true; p.card = p.open = null; p.el.classList.add('going'); p.el.style.pointerEvents = 'none';
  odSeen(p);
  if (FX_ON) p.R.leave(need); else { p.el.classList.add('gone'); p.goneAt = performance.now() + 320; } // removed by odFrame once faded
  wake();
}
function odSeen(p) {
  if (p.need === 'done') OD.seen.add(`${p.s.id}|done|${p.s.since}`);
  if (p.need === 'ctx') OD.seen.add(`${p.s.id}|ctx`);
}
function odRemove(p, seen = true) { if (seen) odSeen(p); p.el.remove(); if (OD.pets.get(p.s.id) === p) OD.pets.delete(p.s.id); }
function odClear() { for (const p of [...OD.pets.values()]) odRemove(p, false); OD.ui?.corner.remove(); OD.ui = null; }

// Its name, and on hover its card: what pets.html shows at the edge, plus what the session needs and its buttons.
function odPaint(p) {
  const s = p.s, st = look(s), k = p.need, hp = s.ctx, now = Date.now();
  const key = JSON.stringify([k, st, s.title, s.name, s.cwd, s.quest, s.summary, s.ask, s.askTool, hp, s.lv, s.answer?.id, s.answer?.until == null, cfg.label, cfg.clickAction, lang]);
  if (key === p.key) return;
  p.key = key;
  const need = k === 'waiting' ? `<div class="need waiting"><b>${esc(s.askTool ? T('pet.waiting', { tool: s.askTool }) : T('pet.waitingPlain'))}</b>${s.ask ? `<code>${esc(s.ask)}</code>` : ''}</div>${answerButtons(s, now)}`
    : k === 'done' ? `<div class="need done"><b>${T('od.done')}</b>${esc(s.summary)}</div>`
    : `<div class="need ctx"><b>${T('od.ctx')}</b>${T('od.ctxBody', { n: hp })}</div>`;
  p.el.className = `pet od st-${st}${p.hover ? ' hover' : ''}${p.leaving ? ' going' : ''}`;
  p.el.innerHTML = `<div class="spr"></div><div class="tag"><span class="nm">${esc(labelOf(s))}</span>${s.issue ? `<span class="iss">#${s.issue.iid}</span>` : ''}</div>
    <div class="card"><h2>${esc(s.title || s.name)}</h2><p class="sub">${esc(s.cwd)}</p>${k === 'done' ? '' : `<p class="q">${esc(s.quest || T('line.noQuest'))}</p>`}${need}
      <div class="row"><span class="st"><i class="dot ${st}"></i>${esc(statusText(s, st))}</span><span class="badge">${T('pet.lv', { n: s.lv })}</span></div>
      <div class="row ctx"><span>${T('ctx')}</span><span>${hp == null ? '?' : hp + '%'}</span></div><div class="meter ${hp != null && hp < OD_CTX ? 'low' : ''}"><i style="--v:${(hp ?? 0) / 100}"></i></div>
      <div class="hint"><span>${T('od.click')}</span>${k === 'waiting' ? '' : `<button class="btn sm ghost" type="button" data-hide>${T('pet.hide')}</button>`}</div></div>`;
  if (p.R) p.el.querySelector('.spr').append(p.R.el);
  if (p.hover) odCardPlace(p);
}
function odPress(p, e) {
  if (e.button !== 0 || p.leaving) return;
  const a = e.target.closest('[data-answer]');
  if (a) { if (p.s.answer) { OD.answered.add(p.s.answer.id); bridge?.answer(p.s.answer.id, a.dataset.answer); odLeave(p, 'waiting'); } return; }
  if (e.target.closest('[data-hide]')) return odLeave(p, p.need);
  if (e.target.closest('.card') && !e.target.closest('h2')) return; // reading its card
  bridge ? bridge.petClick(p.s.id) : open('/?s=' + encodeURIComponent(p.s.id));
  if (p.need !== 'waiting') odLeave(p, p.need); // seen: it goes
}

// ---------- crossing the screen ----------
const odBox = (x, y) => ({ l: x, r: x + cfg.size, t: y - PetRide.above(cfg.size), b: y + cfg.size });
const odHit = (a, b, g) => a.l < b.r + g && a.r + g > b.l && a.t < b.b + g && a.b + g > b.t;
// A lane: the pet's column, away from the pointer and from the other pets' columns and the pill, near the pointer if it can be.
function odLaneCost(p, b) {
  let c = 0;
  for (const o of OD.pets.values()) if (o !== p && !o.leaving && o.box) { const ov = Math.min(b.r, o.box.r) - Math.max(b.l, o.box.l); if (ov > -OD_GAP) c += 2000 + ov * 10; }
  if (mouse.x >= 0 && mouse.x > b.l - 40 && mouse.x < b.r + 40) c += 5000; // never right under the pointer
  const k = OD.ui?.pill.getBoundingClientRect(); if (k?.width && b.r > k.left - 8 && b.l < k.right + 8) c += 800;
  return c + Math.abs((b.l + b.r) / 2 - (mouse.x >= 0 ? mouse.x + 140 : innerWidth / 2)) * .5;
}
function odPickLane(p) {
  let best = 6, bc = Infinity;
  for (let x = 6; x <= innerWidth - cfg.size - 6; x += 10) { const c = odLaneCost(p, odBox(x, 0)); if (c < bc) { bc = c; best = x; } }
  return best;
}
function odLap(p, first) {
  p.x = odPickLane(p); p.y = innerHeight + PetRide.above(cfg.size) + 6; p.box = odBox(p.x, p.y);
  const now = performance.now();
  p.startAt = first ? Math.max(now, OD.lastStart + OD_STAGGER * 1000) : now; // pets starting together go one after another
  OD.lastStart = p.startAt;
  odPlace(p);
}
function odPlace(p) { p.el.style.transform = `translate(${Math.round(p.x + (p.cross ? OD_SWAY * Math.sin(p.swing * Math.PI / 2) : 0))}px,${Math.round(p.y)}px)`; }
// One step up. It waits while pointed at, or behind a pet just ahead in its column, so two pets never overlap.
function odCross(p, t, dt) {
  if (t >= p.startAt && !p.hover) {
    const speed = (innerHeight + PetRide.above(cfg.size) + cfg.size + 20) / OD_CROSS, nb = odBox(p.x, p.y - speed * dt);
    const ahead = [...OD.pets.values()].some(o => o !== p && !o.leaving && o.box && o.box.t < p.box.t && odHit(nb, o.box, OD_GAP));
    const card = [...OD.pets.values()].some(o => o !== p && o.open?.some(r => odHit(nb, r, OD_GAP) && !odHit(p.box, r, OD_GAP))); // an open card or name: wait below it
    if (!ahead && !card) { p.y = nb.b - cfg.size; p.box = nb; p.swing += dt; }
  }
  odPlace(p);
  if (p.box.b < 0) { if (p.last || p.need === 'done') odRemove(p); else odLap(p, false); } // out past the top: again, unless its need is over (a finished task crosses once)
}
// Reduced motion: no flight; it stands near the bottom edge, not under the pointer, apart from the other pets.
function odRest(p) { p.x = odPickLane(p); p.y = innerHeight - cfg.size - 20; p.box = odBox(p.x, p.y); odPlace(p); }
// Its card: placed once as it opens (it never jumps while you reach for its buttons): beside the pet or above it, on screen,
// clear of the pet itself, its balloon and its name tag, and of every other pet and the pill; the least covered spot if
// none is clear. While it is open, pets rising towards it, or towards the pet's name tag, wait below them (odCross).
const odArea = (a, b) => Math.max(0, Math.min(a.r, b.r) - Math.max(a.l, b.l)) * Math.max(0, Math.min(a.b, b.b) - Math.max(a.t, b.t));
const odGrow = (r, g) => ({ l: r.l - g, t: r.t - g, r: r.r + g, b: r.b + g });
const odRect = n => { const b = n.getBoundingClientRect(); return { l: b.left, t: b.top, r: b.right, b: b.bottom }; };
function odCardPlace(p) {
  const c = p.el.querySelector('.card'); if (!c) return;
  const o = p.el.getBoundingClientRect(), tag = p.el.querySelector('.tag'), cw = c.offsetWidth, ch = c.offsetHeight, W = innerWidth, H = innerHeight, M = 8;
  const own = [p.box, ...(tag ? [odRect(tag)] : [])].map(r => odGrow(r, 6)), L = Math.min(...own.map(r => r.l)), R = Math.max(...own.map(r => r.r));
  const others = [...OD.pets.values()].filter(q => q !== p && !q.leaving && q.box).map(q => odGrow(q.box, OD_GAP));
  const pill = OD.ui && !OD.ui.corner.hidden ? [odGrow(odRect(OD.ui.corner), M)] : [];
  const fits = r => r.l >= M && r.t >= M && r.r <= W - M && r.b <= H - M;
  const tries = [];
  for (let k = 0; k <= 24; k++) for (const s of k ? [1, -1] : [1]) { // beside it, level with the balloon first, then higher or lower (up to a tall card's height)
    const t = p.box.t + s * k * 24;
    tries.push({ l: R + 4, t, pref: k * 2 }, { l: L - 4 - cw, t, pref: k * 2 + 1 });
  }
  const cx = Math.min(Math.max(M, (p.box.l + p.box.r - cw) / 2), W - M - cw);
  tries.push({ l: cx, t: p.box.t - 6 - ch, pref: 30 }, { l: cx, t: Math.max(...own.map(r => r.b)), pref: 31 }); // above it, or below its name
  let best = null, bc = Infinity;
  for (const x of tries) {
    const r = { l: x.l, t: Math.min(Math.max(M, x.t), H - M - ch), r: 0, b: 0 }; r.r = r.l + cw; r.b = r.t + ch;
    if (!fits(r) || own.some(q => odArea(q, r))) continue;
    const cost = others.reduce((a, q) => a + odArea(q, r), 0) * 100 + pill.reduce((a, q) => a + odArea(q, r), 0) * 100 + x.pref;
    if (cost < bc) { bc = cost; best = r; }
  }
  if (!best) best = { l: R + 4, t: Math.min(Math.max(M, p.box.t), H - M - ch) }, best.r = best.l + cw, best.b = best.t + ch; // a tiny screen: beside it anyway
  p.card = best; p.open = [best, ...(tag ? [odRect(tag)] : [])]; // what pets rising towards it wait below
  p.el.style.setProperty('--cx', `${Math.round(best.l - o.left)}px`); p.el.style.setProperty('--cy', `${Math.round(best.t - o.top)}px`);
}

// Every overlay frame: returns the rects that take the mouse and whether anything moves.
function odFrame(t, dt) {
  const now = Date.now(), rects = [];
  let busy = false;
  for (const p of [...OD.pets.values()]) {
    if (p.R?.tick(t, dt) === 'end') { odRemove(p); continue; }
    if (p.leaving) { if (t >= (p.goneAt ?? Infinity)) odRemove(p); busy = true; continue; }
    const over = mouse.x >= p.box.l && mouse.x <= p.box.r && mouse.y >= p.box.t && mouse.y <= p.box.b
      || [...p.el.querySelectorAll('.card,.tag')].some(n => inRect(n.getBoundingClientRect(), mouse.x, mouse.y));
    if (over) p.hoverAt = t;
    const hover = over || t - (p.hoverAt ?? -1e9) < 300; // a short grace while moving between the pet and its card
    if (hover !== p.hover) { p.hover = hover; p.el.classList.toggle('hover', hover); if (hover) odCardPlace(p); else p.card = p.open = null; }
    if (p.hover && p.until !== Infinity) p.until = Math.max(p.until, now + 1500); // reading it: it stays
    if (p.need === 'ctx') p.R.setLevel(Math.min(4, Math.floor((now - p.t0) / OD_SHOW.ctx * 5))); // the balloon leaks
    if (now > p.until) p.last = true;
    if (p.cross) { odCross(p, t, dt); if (!OD.pets.has(p.s.id)) continue; busy = true; }
    else if (p.last && !p.hover) odLeave(p, p.need);
    if (FX_ON) busy = true; // the balloon's loop
    rects.push([p.box.l, p.box.t, p.box.r - p.box.l, p.box.b - p.box.t]);
    if (p.hover) for (const n of p.el.querySelectorAll('.card,.tag')) { const b = n.getBoundingClientRect(); rects.push([b.left, b.top, b.width, b.height]); }
  }
  if (PetRide.freesTick(t)) busy = true;
  odPill();
  if (OD.ui) for (const n of OD.ui.corner.querySelectorAll('#odPill,#odDndOff,#odPop')) if (!n.hidden) { const b = n.getBoundingClientRect(); rects.push([b.left, b.top, b.width, b.height]); }
  return { rects: rects.map(r => r.map(Math.round)), busy };
}

// ---------- the corner pill: how many sessions, which need you; the list with Allow / Deny; "do not disturb" ----------
function odUI() {
  if (OD.ui) return;
  const corner = document.createElement('div');
  corner.id = 'odCorner';
  corner.innerHTML = `<div id="odPop" hidden role="dialog"></div><button id="odDndOff" type="button" hidden></button><button id="odPill" type="button" aria-expanded="false" aria-controls="odPop"><i class="dot"></i><span></span></button>`;
  document.body.appendChild(corner);
  OD.ui = { corner, pill: corner.querySelector('#odPill'), pop: corner.querySelector('#odPop'), off: corner.querySelector('#odDndOff') };
  OD.ui.pill.onclick = () => { const open = OD.ui.pop.hidden; OD.ui.pop.hidden = !open; OD.ui.pill.setAttribute('aria-expanded', open); OD.popKey = ''; odPill(); };
  OD.ui.off.onclick = () => bridge?.setSettings({ dnd: '0', dndUntil: 0 });
  OD.ui.pop.onpointerdown = e => {
    const row = e.target.closest('[data-sid]'), s = row && byId(row.dataset.sid);
    if (!s) return;
    const a = e.target.closest('[data-answer]');
    if (a) { if (s.answer) { OD.answered.add(s.answer.id); bridge?.answer(s.answer.id, a.dataset.answer); const p = OD.pets.get(s.id); if (p) odLeave(p, 'waiting'); } return; }
    bridge ? bridge.petClick(s.id) : open('/?s=' + encodeURIComponent(s.id));
  };
}
function odPill() {
  if (!OD.ui) return;
  const list = (lastList || []).filter(s => s.pet !== 'hidden'), now = Date.now(), dnd = dndOn();
  const w = list.filter(s => look(s) === 'waiting').length, news = [...OD.pets.values()].filter(p => p.need !== 'waiting').length;
  const parts = [T('od.sessions', { n: list.length })];
  if (w) parts.push(T('od.need', { n: w })); else if (news) parts.push(T('od.news', { n: news }));
  if (dnd) parts.unshift(T('od.dnd', { n: Math.ceil((cfg.dndUntil - now) / 60e3) }));
  const text = parts.join(' · '), key = JSON.stringify([text, w, news, dnd, list.length, lang]);
  if (key !== OD.pillKey) {
    OD.pillKey = key;
    OD.ui.corner.hidden = !list.length && !dnd;
    OD.ui.pill.querySelector('span').textContent = text;
    OD.ui.pill.setAttribute('aria-label', T('od.pill', { x: text }));
    OD.ui.pill.querySelector('.dot').className = 'dot ' + (w ? 'waiting' : news ? 'done' : '');
    OD.ui.off.hidden = !dnd; OD.ui.off.textContent = T('od.dndOff');
  }
  if (!OD.ui.pop.hidden) {
    const pk = JSON.stringify([list.map(s => [s.id, look(s), s.tool, s.askTool, s.answer?.id, s.title, s.name]), lang]);
    if (pk !== OD.popKey) {
      OD.popKey = pk;
      OD.ui.pop.innerHTML = `<h2>${T('od.list')}</h2>` + list.map(s => {
        const st = look(s);
        return `<div class="row ${st}" data-sid="${esc(s.id)}"><span class="mini">${petArt(s, st, petColor(s, lastList || []))}</span><span class="nm">${esc(labelOf(s))}</span>`
          // Allow / Deny when Bobix can answer; otherwise the row keeps its status (where to answer is its tooltip).
          + `<span class="stx"${st === 'waiting' && !(bridge && left(s, now)) ? ` title="${esc(T(approveWhere(s)))}"` : ''}>${esc(statusText(s, st))}</span>${st === 'waiting' && bridge && left(s, now) ? answerButtons(s, now) : ''}</div>`;
      }).join('');
    }
  }
}
