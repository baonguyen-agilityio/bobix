// Evidence-only helpers for the pets window, queued through cmd-server.mjs (they use the page's own globals).
// pet(name): the pet whose session name starts with name. hold(name, ms): point at it (the cursor the app would feed)
// for ms. press(sel, name): a person's press on that part of the pet's card (Allow / Deny) or on the pet itself.
window.EV = {
  pet: n => [...pets.values()].find(p => (p.s.name || p.s.title || '').startsWith(n) || (p.s.issue && '#' + p.s.issue.iid === n)),
  at: p => { const r = (p.el.querySelector('.tag') || p.el).getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top - 30 }; },
  hold(n, ms = 4000) { const p = EV.pet(n); if (!p) return 'no pet ' + n; clearInterval(EV.t); const end = Date.now() + ms;
    EV.t = setInterval(() => { if (Date.now() > end) { clearInterval(EV.t); moveTo({ x: -1, y: -1 }); } else moveTo(EV.at(p)); }, 30); return 'holding ' + n; },
  press(sel, n) { const p = EV.pet(n); if (!p) return 'no pet ' + n; const el = sel ? p.el.querySelector(sel) : p.el.querySelector('canvas') || p.el; if (!el) return 'no ' + sel;
    const r = el.getBoundingClientRect(), o = { bubbles: true, button: 0, pointerId: 1, isPrimary: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 };
    el.dispatchEvent(new PointerEvent('pointerdown', o)); el.dispatchEvent(new PointerEvent('pointerup', o)); return 'pressed ' + (sel || 'pet') + ' ' + n; },
  list: () => [...pets.values()].map(p => `${p.s.name}|${p.s.state}|x=${Math.round(p.x)}|ctx=${p.s.ctx}`).join('\n'),
};
'helpers ready';
