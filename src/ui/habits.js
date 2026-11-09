// Habits (Settings → "Bobix nhớ gì về bạn", off by default): from the activity Bobix logged (src-tauri/src/habits.rs:
// times, folders, branches, issues, states, approvals, skills; never what you wrote), a few suggestions made by simple
// local rules, each with its reason. No AI, nothing leaves the machine. Works in the browser (window.HABITS) and in Node.
(function (root) {
  const MIN = 60e3, H = 60 * MIN, D = 24 * H;
  const MIN_DAYS = 5; // days with activity before the first suggestion
  const MAX_CARDS = 3;
  const KINDS = ['morning', 'allow', 'recurring', 'skill'];
  // Claude Code's own commands are not skills (the app does not log them as skills: BUILTIN in src-tauri/src/habits.rs).
  const BUILTIN = new Set(['clear', 'compact', 'help', 'model', 'config', 'rename', 'resume', 'exit', 'login', 'logout', 'status', 'cost', 'memory', 'init', 'doctor', 'permissions', 'hooks', 'mcp', 'agents', 'context', 'add-dir', 'bug', 'vim', 'theme', 'terminal-setup', 'upgrade', 'export', 'release-notes', 'usage']);

  const pad = n => String(n).padStart(2, '0');
  // Local day of a time, 'YYYY-MM-DD'; local midnight of a time.
  const dayOf = t => { const d = new Date(t); return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`; };
  const midnight = t => { const d = new Date(t); return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime(); };
  const minuteOf = t => { const d = new Date(t); return d.getHours() * 60 + d.getMinutes(); };
  const hhmm = m => `${Math.floor(m / 60)}:${pad(m % 60)}`;
  const base = p => String(p || '').split(/[\\/]/).filter(Boolean).pop() || '';

  // Days with activity, newest first: per session that day, its time span, folder, branch, issue, approvals and skills.
  function days(events) {
    const by = new Map();
    for (const e of events || []) {
      if (!e.s) continue;
      const d = dayOf(e.t), key = d + '\n' + e.s;
      if (!by.has(d)) by.set(d, new Map());
      const day = by.get(d);
      const x = day.get(key) || { sid: e.s, from: e.t, to: e.t, dir: e.d, project: e.p || null, branch: e.b || null, iid: e.i ?? null, asks: 0, waitMs: 0, skills: [], ended: false };
      if (e.p) x.project = e.p;
      x.from = Math.min(x.from, e.t); x.to = Math.max(x.to, e.t);
      if (e.b) x.branch = e.b;
      if (e.i != null) x.iid = e.i;
      if (e.k === 'ask') { x.asks++; x.waitMs += e.ms || 0; }
      if (e.k === 'skill' && !BUILTIN.has(e.v) && !x.skills.includes(e.v)) x.skills.push(e.v);
      if (e.k === 'end') x.ended = true;
      day.set(key, x);
    }
    return [...by].sort(([a], [b]) => (a < b ? 1 : -1)).map(([day, m]) => ({ day, items: [...m.values()].sort((a, b) => a.from - b.from) }));
  }
  const loggedDays = events => new Set((events || []).map(e => dayOf(e.t))).size;

  // The project of a folder, from the work model (repos and their folders).
  const projectOf = (model, dir) => (model?.repos || []).find(r => (r.dirs || []).some(x => x.path === dir))?.path || null;

  // Start of day: the issue you last worked on (before today, within a week) is still open, has no PR / MR, and nobody
  // touched it today. Only in the morning.
  function morning(events, model, sessions, now) {
    if (new Date(now).getHours() >= 12) return null;
    const today = midnight(now), last = new Map();
    for (const e of events || []) {
      const p = e.p || projectOf(model, e.d);
      if (e.i == null || !e.d || !p || e.t < today - 7 * D) continue;
      const key = p + '#' + e.i, x = last.get(key);
      if (!x || e.t > x.t) last.set(key, { t: e.t, project: p, dir: e.d, iid: e.i, branch: e.b || null });
    }
    const live = new Set((sessions || []).filter(s => s.issue).map(s => s.project + '#' + s.issue.iid));
    const cands = [...last.values()].filter(x => x.t < today && !live.has(x.dir + '#' + x.iid)).sort((a, b) => b.t - a.t);
    for (const x of cands) {
      const p = x.project, i = (model?.issues || []).find(y => y.project === p && y.iid === x.iid);
      if (!i || i.reviews?.length || i.sessions?.length) continue; // closed, in review, or someone is on it
      return { id: `morning:${p}#${x.iid}:${dayOf(now)}`, kind: 'morning', project: p, dir: x.dir, iid: x.iid, title: i.title, branch: x.branch, at: x.t };
    }
    return null;
  }

  // The same approval, allowed again and again in one folder over the last 7 days, never denied there.
  function allows(events, now, n) {
    const by = new Map();
    for (const e of events || []) {
      if (e.k !== 'ask' || !e.r || e.t < now - 7 * D) continue;
      const key = e.d + '\n' + e.r, x = by.get(key) || { dir: e.d, rule: e.r, ok: 0, no: 0 };
      e.ok ? x.ok++ : x.no++;
      by.set(key, x);
    }
    return [...by.values()].filter(x => !x.no && x.ok >= 10 * (1 + n)).sort((a, b) => b.ok - a.ok)
      .map(x => ({ id: `allow:${x.dir}:${x.rule}`, kind: 'allow', dir: x.dir, rule: x.rule, count: x.ok, what: (x.rule.match(/^Bash\((.*?)(?::\*)?\)$/) || [])[1] || x.rule }));
  }

  // Something you open at about the same time on the same weekday (the stand-up, the report), 3 of the last 4 weeks.
  // Shown on that weekday from 15 minutes before to an hour after, until you open it.
  function recurring(events, now, n) {
    const out = [], wd = new Date(now).getDay(), at = minuteOf(now), today = midnight(now);
    for (const what of ['standup', 'report']) {
      const xs = (events || []).filter(e => e.k === 'ui' && e.v === what && e.t >= today - 28 * D);
      if (xs.some(e => e.t >= today)) continue; // done today
      const same = xs.filter(e => new Date(e.t).getDay() === wd);
      const weeks = new Set(same.map(e => dayOf(e.t)));
      if (weeks.size < 3 + n) continue;
      const mins = same.map(e => minuteOf(e.t)).sort((a, b) => a - b), mid = mins[Math.floor(mins.length / 2)];
      const near = mins.filter(m => Math.abs(m - mid) <= 30);
      if (new Set(same.filter(e => Math.abs(minuteOf(e.t) - mid) <= 30).map(e => dayOf(e.t))).size < 3 + n) continue;
      if (at < mid - 15 || at > mid + 60) continue;
      out.push({ id: `recurring:${what}:${wd}:${dayOf(now)}`, kind: 'recurring', what, weekday: wd, at: mid, from: near[0], to: near[near.length - 1], count: weeks.size });
    }
    return out;
  }

  // The skill most sessions in a folder start with this week (60% of them, at least 5).
  function skills(events, now, n, chosen) {
    const by = new Map();
    for (const e of events || []) {
      if (!e.d || !e.s || e.t < now - 7 * D) continue;
      const x = by.get(e.d) || { dir: e.d, all: new Set(), first: new Map() };
      x.all.add(e.s);
      if (e.k === 'skill' && !BUILTIN.has(e.v) && !x.first.has(e.s)) x.first.set(e.s, e.v);
      by.set(e.d, x);
    }
    const out = [];
    for (const x of by.values()) {
      const count = new Map();
      for (const k of x.first.values()) count.set(k, (count.get(k) || 0) + 1);
      const [skill, c] = [...count].sort((a, b) => b[1] - a[1])[0] || [];
      if (!skill || c < 5 * (1 + n) || c / x.all.size < 0.6 || (chosen || {})[x.dir] === skill) continue;
      out.push({ id: `skill:${x.dir}:${skill}`, kind: 'skill', dir: x.dir, skill, count: c, of: x.all.size });
    }
    return out.sort((a, b) => b.count - a.count);
  }

  // Up to 3 suggestions, each kind once, the kinds you turned down less first. cfg: settings (habitsNo: kind → times
  // "Không cần"; habitsHidden: suggestion id → when; habitsSkill: folder → the skill its Start box begins with).
  // { ready, logged, need, cards }: not ready while fewer than MIN_DAYS days were logged.
  function suggest(events, ctx, cfg, now = Date.now()) {
    const logged = loggedDays(events), out = { ready: logged >= MIN_DAYS, logged, need: MIN_DAYS, cards: [] };
    if (!out.ready) return out;
    const no = k => (cfg?.habitsNo || {})[k] || 0, hidden = cfg?.habitsHidden || {};
    const by = {
      morning: no('morning') < 3 ? [morning(events, ctx?.model, ctx?.sessions, now)].filter(Boolean) : [],
      allow: allows(events, now, no('allow')),
      recurring: recurring(events, now, no('recurring')),
      skill: skills(events, now, no('skill'), cfg?.habitsSkill),
    };
    const kinds = [...KINDS].sort((a, b) => no(a) - no(b));
    for (const k of kinds) {
      const c = by[k].find(x => hidden[x.id] == null);
      if (c && out.cards.length < MAX_CARDS) out.cards.push(c);
    }
    return out;
  }

  const api = { MIN_DAYS, MAX_CARDS, KINDS, BUILTIN, dayOf, hhmm, base, days, loggedDays, projectOf, morning, allows, recurring, skills, suggest };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.HABITS = api;
})(this);
