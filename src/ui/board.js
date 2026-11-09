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

  // The issue to start next: one in To do that nobody started (no session, none named after it, no new session
  // waiting for it) in a project with a folder on this machine. Overdue or due soonest first, then the oldest.
  // proj: the project picked in To do ('all' for any); waiting: [{ dir, iid }].
  function next(st, waiting = [], proj = 'all') {
    const free = st.todo.filter(([i, g]) => g.dir && !i.resume && (proj === 'all' || g.path === proj) && !waiting.some(w => w.dir === g.dir && w.iid === i.iid));
    const due = ([i]) => i.due || '9999';
    return free.sort((a, b) => (due(a) < due(b) ? -1 : due(a) > due(b) ? 1 : 0) || a[0].iid - b[0].iid)[0] || null;
  }

  // What Claude gets in the new session: the issue's title, its description and its link.
  const brief = (i, body) => [`#${i.iid} ${i.title}`, String(body || '').trim(), i.url ? String(i.url) : ''].filter(Boolean).join('\n\n');

  // The skill to suggest for an issue, from its labels and your role (Settings: ba, qa, dev): words in a skill's name
  // (its last part, after a plugin's "name:") score most, a label's over the role's; the repo's own skills win a tie.
  // { name, why: 'label' | 'role', label } or null.
  const BY_LABEL = [[/bug|defect|fix|regression|error|crash/i, ['fix', 'bug', 'debug']], [/enhancement|feature|story|epic|request/i, ['feature', 'plan', 'cook', 'implement', 'story', 'stories']],
    [/doc/i, ['doc', 'release-note']], [/test|qa/i, ['test', 'verify', 'qa']], [/question|support/i, ['question', 'answer']], [/refactor|chore|debt/i, ['refactor', 'simplif', 'clean']]];
  const BY_ROLE = { ba: ['stor', 'question', 'gap', 'requirement', 'spec', 'brainstorm', 'plan'], qa: ['test', 'verify', 'qa', 'bug', 'review', 'check'], dev: ['fix', 'cook', 'implement', 'feature', 'debug', 'ticket', 'code'] };
  function skillFor(skills, labels = [], role = '') {
    const words = labels.flatMap(l => (BY_LABEL.find(([re]) => re.test(l)) || [, []])[1].map(w => [w, l]));
    let best = null;
    for (const s of skills) {
      const short = s.name.split(':').pop().toLowerCase();
      const byLabel = words.find(([w]) => short.includes(w)), byRole = (BY_ROLE[role] || []).find(w => short.includes(w));
      const score = (byLabel ? 4 : 0) + (byRole ? 2 : 0) + (byLabel || byRole ? (s.source === 'project' ? 1 : 0) : 0);
      if (score && (!best || score > best.score)) best = { score, name: s.name, why: byLabel ? 'label' : 'role', label: byLabel?.[1] };
    }
    return best && { name: best.name, why: best.why, label: best.label };
  }

  // The prompt with a skill: "/<skill> <link>" on its first line (replacing one there), or without it ('').
  function withSkill(text, skill, url) {
    const rest = String(text || '').replace(/^\/\S+[^\n]*\n*/, '');
    return skill ? `/${skill} ${url || ''}`.trimEnd() + '\n\n' + rest : rest;
  }

  const api = { PRI, day, stages, loose, short, report, next, brief, skillFor, withSkill };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.BOARD = api;
})(this);
