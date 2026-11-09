// The board's working day: which issues are to do, in progress, waiting for review or done today, and the end-of-day
// report. Works in the browser (window.BOARD) and in Node (require).
(function (root) {
  // A ticket's sessions: the most urgent state stands for all of them.
  const PRI = { waiting: 0, working: 1, done: 2, idle: 3 };

  // 'YYYY-MM-DD' of a time (ms or ISO text) in local time.
  function day(t) {
    const d = new Date(t);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }

  // work: the panel's work view (local / away groups, closed); sessions: the live list, each with its look (st).
  // An open issue's stage comes from the work model (src-tauri/src/model.rs): verify when an open PR / MR closes it,
  // doing while a live session works on it, else todo. Done today: assigned issues closed since local midnight (the
  // panel's clock decides the day). Each list holds [issue, group] pairs, groups in the work view's order; doing is
  // most urgent first.
  function stages(work, sessions, now = Date.now()) {
    const out = { todo: [], doing: [], verify: [], done: [] };
    if (!work || work.status === 'off') return out;
    const byId = new Map(sessions.map(s => [s.id, s]));
    const groups = [...work.local || [], ...work.away || []];
    for (const g of groups) for (const i of g.issues || []) {
      const live = (i.sessions || []).map(id => byId.get(id)).filter(Boolean);
      (out[i.stage] || out.todo).push([i, g, live]);
    }
    const lead = x => Math.min(...x[2].map(s => PRI[s.st] ?? 3));
    const last = x => Math.max(0, ...x[2].map(s => s.since || 0));
    out.doing.sort((a, b) => lead(a) - lead(b) || last(b) - last(a));
    const today = day(now);
    for (const i of work.closed || []) {
      if (!i.closedAt || day(i.closedAt) !== today) continue;
      const g = groups.find(x => x.path === i.project) || { path: i.project, issues: [] };
      if (!out.done.some(([d, dg]) => d.iid === i.iid && dg.path === g.path)) out.done.push([i, g, []]);
    }
    return out;
  }

  // Sessions that work on no open issue of the board.
  const loose = (st, sessions) => {
    const on = new Set([...st.doing, ...st.verify].flatMap(x => x[2].map(s => s.id)));
    return sessions.filter(s => !on.has(s.id));
  };

  // "[x/y] title" → the project's short name.
  const short = p => String(p || '').split('/').pop();

  // Plain text to paste for the team: per project, what was done today and what waits for review.
  function report(st, T, now = Date.now(), locale) {
    const by = new Map();
    const add = (k, [i, g]) => { const p = g.path; if (!by.has(p)) by.set(p, { done: [], verify: [] }); by.get(p)[k].push(i); };
    st.done.forEach(x => add('done', x));
    st.verify.forEach(x => add('verify', x));
    const list = (title, items) => items.length ? `${title}:\n` + items.map(i => `- #${i.iid} ${i.title}`).join('\n') : '';
    const parts = [...by].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([p, x]) => `[${short(p)}]\n` + [list(T('report.done'), x.done), list(T('report.verify'), x.verify)].filter(Boolean).join('\n'));
    return `${T('report.head', { d: new Date(now).toLocaleDateString(locale) })}\n\n${parts.join('\n\n') || T('report.none')}\n`;
  }

  const api = { PRI, day, stages, loose, short, report };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.BOARD = api;
})(this);
