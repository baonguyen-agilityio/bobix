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
  // ask while a live session on it waits for approval, doing while one works on it, else todo; ask is listed with doing. Done today: assigned issues closed since local midnight (the
  // panel's clock decides the day). Each list holds [issue, group] pairs, groups in the work view's order; doing is
  // most urgent first.
  function stages(work, sessions, now = Date.now()) {
    const out = { todo: [], doing: [], verify: [], done: [] };
    if (!work || work.status === 'off') return out;
    const byId = new Map(sessions.map(s => [s.id, s]));
    const groups = [...work.local || [], ...work.away || []];
    for (const g of groups) for (const i of g.issues || []) {
      const live = (i.sessions || []).map(id => byId.get(id)).filter(Boolean);
      (out[i.stage === 'ask' ? 'doing' : i.stage] || out.todo).push([i, g, live]);
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

  // The five steps a ticket goes through; the model gives the first four, a close today the last.
  const STEPS = ['todo', 'doing', 'ask', 'verify', 'done'];
  const stepOf = i => (i.closedAt ? 'done' : STEPS.includes(i.stage) ? i.stage : 'todo');

  // Why a ticket is at its step, in words: the open PR / MR, the requests waiting, the sessions and their branch.
  function why(i, live, T, locale) {
    const step = stepOf(i);
    if (step === 'done') return T('why.done', { t: new Date(i.closedAt).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }) });
    if (step === 'verify') return T(/\/merge_requests\//.test(i.review || '') ? 'why.mr' : 'why.pr', { n: (String(i.review || '').match(/(\d+)\/?$/) || [])[1] || '' });
    if (step === 'ask') return T('why.ask', { n: live.filter(s => s.st === 'waiting').length });
    if (!live.length) return T('why.todo');
    if (live.every(s => s.st === 'done' || s.st === 'idle')) return T('why.resting');
    const b = live.map(s => s.git && s.git.branch).find(Boolean);
    return T(b ? 'why.doingBranch' : 'why.doing', { n: live.length, b });
  }

  // Work that may be done twice: two sessions on one open issue, or on one branch of a project. A side session goes
  // with its parent (one piece of work), and a resting session works on nothing. model: the snapshot's work model.
  // Returns issues: "project#iid" → how many, and sessions: id → { branch, with } (the other session on that branch).
  function dups(model, sessions) {
    const live = new Map(sessions.filter(s => s.st && s.st !== 'idle').map(s => [s.id, s]));
    const roots = ids => new Set(ids.map(id => live.get(id)).filter(Boolean).map(s => s.parent || s.id));
    const out = { issues: new Map(), sessions: new Map() };
    for (const i of (model && model.issues) || []) {
      const n = roots(i.sessions.map(x => x.id)).size;
      if (n > 1) out.issues.set(`${i.project}#${i.iid}`, n);
    }
    for (const b of (model && model.branches) || []) {
      const on = b.sessions.filter(id => live.has(id));
      if (roots(on).size < 2 || (b.issue && out.issues.has(`${b.issue.project}#${b.issue.iid}`))) continue; // the issue warning says it
      for (const id of on) out.sessions.set(id, { branch: b.name, with: on.find(x => (live.get(x).parent || x) !== (live.get(id).parent || id)) });
    }
    return out;
  }

  // The last working day before now: yesterday, or Friday from a Monday (or a weekend).
  function lastWorkday(now) {
    const d = new Date(now);
    do d.setDate(d.getDate() - 1); while (d.getDay() === 0 || d.getDay() === 6);
    return day(d);
  }

  // A stand-up to paste: done on the last working day, in progress today, stuck. closed: the issues closed lately;
  // stuck: lines the panel made ({ text, why }). Plain text, copied by the person; Bobix sends nothing.
  function standup(st, closed, stuck, T, now = Date.now(), locale) {
    const y = lastWorkday(now), seen = new Set();
    const line = (i, p, extra) => `- [${short(p)}] #${i.iid} ${i.title}${extra ? ` (${extra})` : ''}`;
    const done = (closed || []).filter(i => i.closedAt && day(i.closedAt) === y && !seen.has(i.project + '#' + i.iid) && seen.add(i.project + '#' + i.iid)).map(i => line(i, i.project));
    const today = [...st.doing.map(([i, g]) => line(i, g.path, i.stage === 'ask' ? T('stage.ask') : '')), ...st.verify.map(([i, g]) => line(i, g.path, T('stage.verify')))];
    const sec = (k, xs) => `${T(k)}:\n${xs.length ? xs.join('\n') : '- ' + T('su.none')}`;
    return `${T('su.head', { d: new Date(now).toLocaleDateString(locale) })}\n\n${[sec('su.yesterday', done), sec('su.today', today), sec('su.stuck', stuck.map(x => `- ${x.text}: ${x.why}`))].join('\n\n')}\n`;
  }

  const api = { PRI, STEPS, day, stages, loose, short, report, stepOf, why, dups, lastWorkday, standup };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.BOARD = api;
})(this);
