// The guided tour of what is new: its steps, and where its bubble goes next to the element a step points at; and the
// "?" help next to the panel's groups and trickier settings: which ones have it, and where its tip goes.
// Works in the browser (window.TOUR) and in Node (require). The panel draws both (see "tour" and "help" in panel.html).
// After an update the tour opens by itself with only the news step(s) not seen yet, as one bubble (settings.tourSeen: a
// new install counts today's as seen); Setup shows it whole.
(function (root) {
  // Each step: the screen it shows (and the item it picks there), the elements it points at (the first one on screen wins; none: the bubble stands in
  // the middle), and its strings tour.<key>.title / tour.<key>.body. open: what the panel opens first: a ticket (one with a ⚠ if there is one), or the Quick Start box of the issue BOARD.next suggests.
  // alt: where to point instead when none of `at` is there (another screen). news: what this release adds, the only
  // step that opens by itself after an update.
  const STEPS = [
    { key: 'flat', tab: 'settings', item: 'set:place', at: ['#preview'], art: true },
    { key: 'launcher', tab: 'overview', at: ['.b-search'] },
    { key: 'ondemand', tab: 'settings', item: 'set:place', at: ['[data-row="petMode"]'] },
    { key: 'dnd', tab: 'settings', item: 'set:notify', at: ['[data-row="dnd"]'] },
    { key: 'board', tab: 'sessions', at: ['#b-list'] },
    { key: 'stuck', tab: 'overview', at: ['[data-grp="stuck"]'], alt: { tab: 'settings', item: 'set:stuck', at: ['[data-row="stuckOn"]'] } },
    { key: 'next', tab: 'work', at: ['[data-grp="fstart"]', '#b-list'] },
    { key: 'skill', tab: 'work', at: ['#st-skills'], open: 'start', alt: { tab: 'skills', item: 'role', at: ['#sk-role'] } },
    { key: 'skills', tab: 'skills', at: ['#b-list'] },
    { key: 'audit', tab: 'skills', item: 'aud', at: ['[data-grp="audit"]'] },
    { key: 'usage', tab: 'overview', item: 'today', at: ['#d-use'] },
    { key: 'standup', tab: 'overview', item: 'today', at: ['#b-dyn [data-standup]'] },
    { key: 'help', tab: 'overview', at: ['#b-list .help'], alt: { tab: 'settings', item: 'set:stuck', at: ['#settings .help'] }, news: true },
  ];

  // Where the panel has a "?": a list group's heading (grp: its key in drawList) or a settings row (row: its data-row),
  // each with its words help.<grp|row>.<key>. stuckOld shares stuck's.
  const HELP = {
    grp: ['aq', 'stuck', 'warn', 'hab', 'doing', 'verify', 'todo', 'done', 'loose', 'fstart', 'fapprove', 'fdone', 'you', 'projects', 'audit', 'dups', 'off', 'unused'],
    row: ['stuckNoPr', 'stuckNoReview', 'stuckBranch', 'stuckIdle'],
  };
  const helpKey = (kind, k) => (kind === 'grp' && k === 'stuckOld' ? 'stuck' : HELP[kind].includes(k) ? k : null);

  // Where a tip goes: { left, top } in the viewport, for a w × h tip by the "?" at r. Below it, centred on it; above
  // when there is no room below; never closer than TIP_EDGE to the window's sides (the panel goes down to 420 px).
  const TIP_GAP = 6, TIP_EDGE = 8;
  function tip(r, vw, vh, w, h) {
    const left = Math.round(Math.min(Math.max(TIP_EDGE, r.left + r.width / 2 - w / 2), vw - w - TIP_EDGE));
    const below = r.top + r.height + TIP_GAP, above = r.top - TIP_GAP - h;
    const top = below + h <= vh - TIP_EDGE ? below : above >= TIP_EDGE ? above : vh - h - TIP_EDGE;
    return { left: Math.max(TIP_EDGE, left), top: Math.round(Math.max(TIP_EDGE, top)) };
  }

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

  // What opens by itself after an update: the news steps not seen yet (one bubble), in order.
  const fresh = seen => STEPS.filter(s => s.news && !(seen || []).includes(s.key));

  const api = { STEPS, place, NARROW, fresh, HELP, helpKey, tip };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.TOUR = api;
})(this);
