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

  // Work that stands still, one item each (thresholds in hours, Settings → Stuck work; defaults as in settings.rs):
  // - nopr: an issue's sessions all stopped, one finished its turn, and no PR / MR closes the issue;
  // - noreview: a PR / MR of yours nobody has reviewed since it was opened;
  // - branch: a session's branch with commits of its own (not on the remote's default branch), no PR / MR from it,
  //   no new commit for a while;
  // - idle: an issue's sessions all resting (none finished a turn), no activity for a while.
  // model: the snapshot's work model; sessions: the live list. Most stuck first within each kind. Each item has `at`,
  // its last activity (ms): one quiet for longer than stuckMaxAge days is `old` (stuck long ago, not today's nudge);
  // one you skipped (cfg.stuckSkipped: id → its `at` then) is left out until it moves again.
  const STUCK = { stuckNoPr: 2, stuckNoReview: 24, stuckBranch: 4, stuckIdle: 4, stuckMaxAge: 14 };
  function stuck(model, sessions, cfg, now = Date.now()) {
    const out = [], H = 3600e3, byId = new Map(sessions.map(s => [s.id, s]));
    if (!model || cfg?.stuckOn === false) return out;
    const hours = k => cfg?.[k] ?? STUCK[k], over = (ms, k) => ms >= hours(k) * H;
    const busy = s => s.state === 'working' || s.state === 'waiting', newest = (ss, k) => ss.reduce((a, b) => (b[k] > a[k] ? b : a));
    const reviewed = new Set((model.reviews || []).map(r => r.project + '\n' + r.branch));
    const items = { nopr: [], noreview: [], branch: [], idle: [] };
    for (const i of model.issues || []) {
      const ss = i.sessions.map(x => byId.get(x.id)).filter(Boolean);
      if (i.reviews.length || !ss.length || ss.some(busy)) continue;
      const done = ss.filter(s => s.state === 'done');
      const kind = done.length ? 'nopr' : 'idle', s = done.length ? newest(done, 'since') : newest(ss, 'updated');
      const ms = now - (done.length ? s.since : s.updated), k = done.length ? 'stuckNoPr' : 'stuckIdle';
      if (over(ms, k)) items[kind].push({ id: `${kind}:${i.project}#${i.iid}`, kind, project: i.project, iid: i.iid, title: i.title, ms, limit: hours(k), session: s.id, branch: s.git?.branch || null, at: newest(ss, 'updated').updated });
    }
    for (const r of model.reviews || []) {
      const ms = now - Date.parse(r.created);
      if (r.reviewed || !(ms >= 0) || !over(ms, 'stuckNoReview')) continue;
      const l = r.issues[0], i = l && (model.issues || []).find(x => x.project === l.project && x.iid === l.iid);
      const at = Math.max(Date.parse(r.created), Date.parse(r.updated) || 0);
      items.noreview.push({ id: 'noreview:' + r.url, kind: 'noreview', project: r.project, iid: i?.iid ?? null, title: i?.title || r.branch, ms, limit: hours('stuckNoReview'), url: r.url, review: r.kind, at });
    }
    for (const b of model.branches || []) {
      const ss = b.sessions.map(id => byId.get(id)).filter(Boolean), g = ss.find(s => s.git?.branch === b.name)?.git;
      if (!g || !(g.own > 0) || !g.last?.t || ss.some(busy) || reviewed.has(b.project + '\n' + b.name)) continue;
      if ([...items.nopr, ...items.idle].some(x => x.project === b.project && x.branch === b.name)) continue; // already there with its issue
      const ms = now - g.last.t;
      if (over(ms, 'stuckBranch')) items.branch.push({ id: `branch:${b.project}:${b.name}`, kind: 'branch', project: b.project, iid: b.issue?.iid ?? null, title: b.name, ms, limit: hours('stuckBranch'), commits: g.own, branch: b.name, session: newest(ss, 'updated').id, at: Math.max(g.last.t, newest(ss, 'updated').updated || 0) });
    }
    const skipped = cfg?.stuckSkipped || {}, maxAge = hours('stuckMaxAge') * 24 * H;
    for (const k of Object.keys(items)) {
      out.push(...items[k].filter(x => !(skipped[x.id] >= x.at)).map(x => ({ ...x, old: now - x.at > maxAge })).sort((a, b) => b.ms - a.ms));
    }
    return out;
  }

  // A session's project as the wait log names it: its folder's name (src-tauri/src/state.rs, keep_waits).
  const folder = s => String(s.project || s.cwd || s.name || '').split(/[\\/]/).filter(Boolean).pop() || '';

  // Minutes Claude waited for your answer today (since local midnight), per project, most first: the waits that
  // ended (the app's log) and the ones going on now (sessions waiting since). [{ project, mins }]
  function waited(waits, sessions, now = Date.now()) {
    const d = new Date(now), midnight = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime(), by = new Map();
    const add = (p, from, to) => { const ms = Math.min(to, now) - Math.max(from, midnight); if (ms > 0) by.set(p, (by.get(p) || 0) + ms); };
    for (const w of waits || []) add(w.project, w.from, w.to);
    for (const s of sessions) if (s.state === 'waiting') add(folder(s), s.since, now);
    return [...by].map(([project, ms]) => ({ project, mins: Math.round(ms / 60e3) })).filter(x => x.mins > 0)
      .sort((a, b) => b.mins - a.mins || (a.project < b.project ? -1 : 1));
  }

  // Plain text to paste for the team: per project, what was done today and what waits for review; then how long
  // Claude waited for you today (waits: from waited(); left out when not given).
  function report(st, T, now = Date.now(), locale, waits) {
    const by = new Map();
    const add = (k, [i, g]) => { const p = g.path; if (!by.has(p)) by.set(p, { done: [], verify: [] }); by.get(p)[k].push(i); };
    st.done.forEach(x => add('done', x));
    st.verify.forEach(x => add('verify', x));
    const list = (title, items) => items.length ? `${title}:\n` + items.map(i => `- #${i.iid} ${i.title}`).join('\n') : '';
    const parts = [...by].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([p, x]) => `[${short(p)}]\n` + [list(T('report.done'), x.done), list(T('report.verify'), x.verify)].filter(Boolean).join('\n'));
    const total = (waits || []).reduce((a, w) => a + w.mins, 0);
    const wait = !waits ? '' : total ? `\n${T('wait.total', { n: total })}\n${waits.map(w => `- ${w.project}: ${T('wait.mins', { n: w.mins })}`).join('\n')}\n` : `\n${T('wait.none')}\n`;
    return `${T('report.head', { d: new Date(now).toLocaleDateString(locale) })}\n\n${parts.join('\n\n') || T('report.none')}\n${wait}`;
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
  // (its last part, after a plugin's "name:") score, a label's over the role's; the repo's own skills win a tie.
  // A suggestion only: the person sees every skill and picks. { name, label, role } (what it was suggested for:
  // the matching label and / or the role) or null.
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
      if (score && (!best || score > best.score)) best = { score, name: s.name, label: byLabel?.[1], role: byRole ? role : undefined };
    }
    return best && { name: best.name, label: best.label, role: best.role };
  }

  // Where a skill comes from, as the Start box groups them: the project's process (the repository's skills, the team's
  // plugins, your organization's) first, then yours (only the ones you pinned in Settings).
  const TEAM = ['project', 'team', 'managed'];

  // What to tell Claude to get a skill a process names but Claude doesn't have: the process's own hint, else
  // "/plugin install <plugin>" for a plugin's skill ("plugin:skill"), else '' (the panel asks in words).
  const installHint = (name, process) => ((process && process.install) || {})[name] || (name.includes(':') ? `/plugin install ${name.split(':')[0]}` : '');

  // The skill to suggest, never chosen for you. With a process (the repository's .bobix/skills.json or one you
  // imported), its first rule that fits the issue and names an installed skill; it replaces the label / role rules.
  // Without one, the team's skills by label and role, then your pins (one pinned for your role first).
  // o: { labels, role, stage ('todo' when starting), process, pins: { name: [roles] } }.
  // { pick: { name, from: 'process' | 'match' | 'pin', label, role, stage } | null, missing: [{ name, hint }] }:
  // missing are the skills the fitting rules name that Claude doesn't have.
  function suggest(skills, o = {}) {
    const has = new Set(skills.map(s => s.name)), labels = (o.labels || []).map(l => String(l).toLowerCase());
    const missing = [];
    let pick = null;
    if (o.process) {
      for (const r of o.process.rules || []) {
        const w = r.when || {}, label = w.label ? labels.find(l => w.label.includes(l)) : undefined;
        if ((w.label && !label) || (w.role && w.role !== o.role) || (w.stage && w.stage !== (o.stage || 'todo'))) continue;
        if (!has.has(r.skill)) { if (!missing.some(m => m.name === r.skill)) missing.push({ name: r.skill, hint: installHint(r.skill, o.process) }); continue; }
        if (!pick) pick = { name: r.skill, from: 'process', label, role: w.role, stage: w.stage };
      }
      return { pick, missing };
    }
    const team = skillFor(skills.filter(s => TEAM.includes(s.source)), o.labels, o.role);
    if (team) return { pick: { ...team, from: 'match' }, missing };
    const pins = o.pins || {}, pinned = skills.filter(s => pins[s.name] && !TEAM.includes(s.source));
    const mine = o.role && pinned.find(s => pins[s.name].includes(o.role));
    if (mine) return { pick: { name: mine.name, from: 'pin', role: o.role }, missing };
    const m = skillFor(pinned, o.labels, o.role);
    return { pick: m && { ...m, from: 'match' }, missing };
  }

  // The prompt with a skill: "/<skill> <link>" on its first line (replacing one there), or without it ('').
  function withSkill(text, skill, url) {
    const rest = String(text || '').replace(/^\/\S+[^\n]*\n*/, '');
    return skill ? `/${skill} ${url || ''}`.trimEnd() + '\n\n' + rest : rest;
  }

  // The ticket flow (#112): its four steps, the one a flow's status is at, the Start prompt (the issue, the model
  // proposal, how to work: both last parts come from the app, tickets.rs) and sizes for the disk line.
  const FLOW = ['plan', 'approve', 'run', 'done'];
  const flowStep = st => ({ starting: 'plan', planning: 'plan', approve: 'approve', running: 'run', done: 'done', blocked: 'done' })[st] || 'plan';
  const flowPrompt = (i, body, models, rules) => [brief(i, body), models, rules].map(x => String(x || '').trim()).filter(Boolean).join('\n\n');
  const gb = b => (b == null ? '?' : (b / 1024 ** 3).toFixed(b < 10 * 1024 ** 3 ? 1 : 0));

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

  // The panel's search: a word matches the letters under the accents and case (ngon finds Ngôn ngữ, dang nhap finds
  // đăng nhập). fold keeps, for each folded letter, where it came from in the text, so a hit maps back to the text.
  function fold(s) {
    const src = String(s ?? '').normalize('NFC'), from = [], to = [];
    let t = '', i = 0;
    for (const ch of src) {
      const f = ch.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase().replace(/đ/g, 'd');
      for (let k = 0; k < f.length; k++) { from.push(i); to.push(i + ch.length); }
      t += f; i += ch.length;
    }
    return { src, t, from, to };
  }
  const words = q => fold(q).t.split(/\s+/).filter(Boolean);
  const matches = (text, ws) => { const t = fold(text).t; return ws.every(w => t.includes(w)); };
  // The parts of the text the words hit, as [start, end) in src (the text in NFC), in order and merged.
  function hits(text, ws) {
    const f = fold(text), r = [], out = [];
    for (const w of ws) for (let k = f.t.indexOf(w); k >= 0; k = f.t.indexOf(w, k + w.length)) r.push([f.from[k], f.to[k + w.length - 1]]);
    r.sort((a, b) => a[0] - b[0]);
    for (const x of r) { const l = out.at(-1); if (l && x[0] <= l[1]) l[1] = Math.max(l[1], x[1]); else out.push(x); }
    return { src: f.src, r: out };
  }

  const api = { FLOW, flowStep, flowPrompt, gb, PRI, STEPS, day, stages, loose, short, folder, waited, stuck, report, stepOf, why, dups, lastWorkday, standup, next, brief, skillFor, withSkill, suggest, installHint, TEAM, words, matches, hits };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.BOARD = api;
})(this);
