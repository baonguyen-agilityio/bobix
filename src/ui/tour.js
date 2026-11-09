// The guided tour of what is new: its steps, and where its bubble goes next to the element a step points at.
// Works in the browser (window.TOUR) and in Node (require). The panel draws it (see "tour" in panel.html); it opens
// once for the version NEWS in i18n.js names (settings.seenVersion), and again from Settings.
(function (root) {
  // Each step: the tab it shows, the elements it points at (the first one on screen wins; none: the bubble stands in
  // the middle), and its strings tour.<key>.title / tour.<key>.body. open: what the panel opens first (a ticket, one with a ⚠ if there is one).
  // section: the board's section a narrow window must show (it shows one at a time).
  const STEPS = [
    { key: 'pets', tab: 'settings', at: ['#preview'], art: true },
    { key: 'ondemand', tab: 'settings', at: ['[data-row="petMode"]'] },
    { key: 'dnd', tab: 'settings', at: ['[data-row="dnd"]'] },
    { key: 'board', tab: 'sessions', at: ['.flow'] },
    { key: 'stages', tab: 'sessions', section: 'doing', at: ['#d-doing .tr:has(.stg)', '#d-doing'], open: 'ticket' },
    { key: 'next', tab: 'sessions', section: 'todo', at: ['.nx', '#d-todo'] },
    { key: 'standup', tab: 'sessions', at: ['[data-standup]'] },
    { key: 'allow', tab: 'sessions', at: ['#d-aq', '.flow'] },
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

  const api = { STEPS, place, NARROW };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.TOUR = api;
})(this);
