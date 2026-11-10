// The guided tour of what is new: its steps, and where its bubble goes next to the element a step points at.
// Works in the browser (window.TOUR) and in Node (require). The panel draws it (see "tour" in panel.html); it opens
// by itself with the steps not seen yet (settings.tourSeen: a new install counts today's as seen), and whole from Setup.
(function (root) {
  // Each step: the screen it shows (and the item it picks there), the elements it points at (the first one on screen wins; none: the bubble stands in
  // the middle), and its strings tour.<key>.title / tour.<key>.body. open: what the panel opens first: a ticket (one with a ⚠ if there is one), or the Start box of the issue Next up suggests.
  // alt: where to point instead when none of `at` is there (another screen).
  const STEPS = [
    { key: 'pets', tab: 'settings', item: 'set:place', at: ['#preview'], art: true },
    { key: 'ondemand', tab: 'settings', item: 'set:place', at: ['[data-row="petMode"]'] },
    { key: 'dnd', tab: 'settings', item: 'set:notify', at: ['[data-row="dnd"]'] },
    { key: 'board', tab: 'sessions', at: ['#b-list'] },
    { key: 'stages', tab: 'sessions', at: ['#b-dyn .stg', '[data-grp="doing"]'], open: 'ticket' },
    { key: 'stuck', tab: 'overview', at: ['[data-grp="stuck"]'], alt: { tab: 'settings', item: 'set:stuck', at: ['[data-row="stuckOn"]'] } },
    { key: 'next', tab: 'sessions', item: 'next', at: ['[data-bsel="next"]', '[data-grp="todo"]'] },
    { key: 'skill', tab: 'sessions', at: ['#st-skills'], open: 'start', alt: { tab: 'skills', item: 'role', at: ['#sk-role'] } },
    { key: 'skills', tab: 'skills', at: ['#b-list'] },
    { key: 'standup', tab: 'overview', item: 'today', at: ['#b-dyn [data-standup]'] },
    { key: 'wait', tab: 'overview', at: ['[data-grp="aq"] .wt'], alt: { tab: 'settings', item: 'set:notify', at: ['[data-row="longWait"]'] } },
    { key: 'allow', tab: 'overview', at: ['[data-grp="aq"]', '#b-list'] },
  ];

  const GAP = 12, EDGE = 12, NARROW = 560;

  // Where the bubble goes: { left, top, width } in the viewport. A narrow window puts it across the bottom (or the top,
  // when the element is in the lower half); a wide one below the element when it fits, else above, else beside it at
  // the bottom; never off screen. r: the element's rect, or null (then centred).
  function place(r, vw, vh, bw, bh) {
    if (vw < NARROW) {
      const width = vw - 2 * EDGE, low = r && r.top + r.height / 2 > vh / 2;
      return { left: EDGE, top: low ? EDGE : Math.max(EDGE, vh - bh - EDGE), width };
    }
    const width = Math.min(bw, vw - 2 * EDGE);
    if (!r) return { left: Math.round((vw - width) / 2), top: Math.round(Math.max(EDGE, (vh - bh) / 2)), width };
    const left = Math.round(Math.min(Math.max(EDGE, r.left), vw - width - EDGE));
    const below = r.top + r.height + GAP, above = r.top - GAP - bh;
    const top = below + bh <= vh - EDGE ? below : above >= EDGE ? above : vh - bh - EDGE;
    return { left, top: Math.round(Math.max(EDGE, top)), width };
  }

  // The steps not seen yet, in order.
  const fresh = seen => STEPS.filter(s => !(seen || []).includes(s.key));

  const api = { STEPS, place, NARROW, fresh };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.TOUR = api;
})(this);
