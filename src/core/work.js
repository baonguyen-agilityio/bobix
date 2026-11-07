// Work items: a session's local git status (no network), and the GitLab issues assigned to you, read through your own
// `glab` (Bobix keeps no token, writes nothing to GitLab). Commands get argument lists, never a shell.
const { execFile } = require('child_process');

const run = (cmd, args, opts = {}) => new Promise(res => execFile(cmd, args, { timeout: 15000, windowsHide: true, maxBuffer: 8 * 1024 * 1024, ...opts },
  (err, out, errOut) => res({ err, out: String(out || ''), errOut: String(errOut || '') })));
const git = (dir, args) => run('git', ['-C', dir, ...args], { env: { ...process.env, GIT_OPTIONAL_LOCKS: '0', LC_ALL: 'C' } }); // never takes the index lock from a running git

// `git status --porcelain=v2 --branch` → branch, upstream, ahead / behind, changed files (staged, unstaged, untracked, conflicts).
function parseStatus(text) {
  const out = { branch: null, upstream: null, ahead: 0, behind: 0, changed: 0 };
  for (const l of String(text).split('\n')) {
    if (l.startsWith('# branch.head ')) { const b = l.slice(14); out.branch = b === '(detached)' ? null : b; }
    else if (l.startsWith('# branch.upstream ')) out.upstream = l.slice(18);
    else if (l.startsWith('# branch.ab ')) { const m = /\+(\d+) -(\d+)/.exec(l); if (m) { out.ahead = +m[1]; out.behind = +m[2]; } }
    else if (/^[12u?] /.test(l)) out.changed++;
  }
  return out;
}

// The git status of the repository at `dir`, or null outside git.
async function gitStatus(dir) {
  const st = await git(dir, ['status', '--porcelain=v2', '--branch']);
  if (st.err) return null;
  const [top, log, unpushed, wt, remotes] = await Promise.all([
    git(dir, ['rev-parse', '--show-toplevel']),
    git(dir, ['log', '-1', '--format=%ct%x00%s']),
    git(dir, ['rev-list', '--count', 'HEAD', '--not', '--remotes']), // commits on no remote at all, upstream or not
    git(dir, ['worktree', 'list', '--porcelain']),
    git(dir, ['remote', '-v']),
  ]);
  const [t, subject] = log.out.trim().split('\0');
  return {
    ...parseStatus(st.out),
    top: top.out.trim() || dir,
    unpushed: unpushed.err ? 0 : Number(unpushed.out.trim()) || 0,
    last: t ? { t: Number(t) * 1000, subject: (subject || '').slice(0, 200) } : null,
    worktrees: (wt.out.match(/^worktree /gm) || []).length,
    remotes: [...new Set(remotes.out.split('\n').map(l => l.split(/\s+/)[1]).filter(Boolean))],
  };
}

// "host" (with its port) of the configured GitLab URL, or '' when it isn't one.
function hostOf(url) {
  try { const u = new URL(url); return /^https?:$/.test(u.protocol) && u.hostname ? u.host.toLowerCase() : ''; } catch { return ''; }
}

// A git remote URL → GitLab project path ("group/sub/project", lower case) when it points at `host`, else null.
// Forms: git@host:g/p.git, ssh://git@host[:port]/g/p.git, https://[user@]host[:port]/g/p(.git).
function projectPath(remote, host) {
  if (typeof remote !== 'string' || !host) return null;
  const h = host.toLowerCase().replace(/:\d+$/, '');
  let m = /^(?:[\w.-]+@)?([^:/\s]+):(?!\/)(.+)$/.exec(remote); // scp-like ssh
  let rhost, p;
  if (m && !/^[a-z][\w+.-]*:\/\//i.test(remote)) { rhost = m[1]; p = m[2]; }
  else {
    try { const u = new URL(remote); if (!/^(https?|ssh|git\+ssh):$/.test(u.protocol)) return null; rhost = u.hostname; p = decodeURIComponent(u.pathname); } catch { return null; }
  }
  if (rhost.toLowerCase() !== h) return null;
  p = p.replace(/^\/+|\/+$/g, '').replace(/\.git$/i, '');
  return p.includes('/') ? p.toLowerCase() : null;
}

// GitLab's "create branch" from an issue names it "<iid>-<title>", also under a prefix ("feat/<iid>-…"): the number, or null.
const issueOfBranch = branch => { const m = /(?:^|\/)(\d+)-/.exec(branch || ''); return m ? Number(m[1]) : null; };

// The name of a session that starts on an issue's branch: "<project> #<iid> <short title>", e.g. "shop-app #27 Back leaves without…".
// Leading tags such as "[Bug]" go; the title is cut at a word near 40 characters.
function sessionTitleFor(path, iid, title) {
  let d = String(title || '').replace(/^(\s*\[[^\]]*\])+/, '').replace(/\s+/g, ' ').trim();
  if (d.length > 40) d = d.slice(0, 41).replace(/\s+\S*$/, '').replace(/[\s,.;:-]+$/, '') + '…';
  return `${String(path).split('/').pop()} #${iid}${d ? ' ' + d : ''}`;
}

// The issue a session works on: the newest of its branches (the transcript's gitBranch values, oldest first, then the
// branch it is on now) that names one, else its parent's (a side session in a worktree of its own, on a branch Claude names).
// s: { path, branches, parentIid }.
function issueOfSession(s) {
  if (!s.path) return null;
  if (s.started?.path === s.path) return s.started.iid; // you said so when it started
  for (let i = s.branches.length - 1; i >= 0; i--) { const n = issueOfBranch(s.branches[i]); if (n) return n; }
  return s.parentIid || null;
}

// The fields Bobix shows of one issue from GitLab's API.
const slimIssue = i => ({
  iid: i.iid, title: String(i.title || '').slice(0, 300), url: i.web_url,
  project: String(i.references?.full || '').replace(/#\d+$/, '').toLowerCase(),
  labels: Array.isArray(i.labels) ? i.labels.slice(0, 10).map(String) : [], due: i.due_date || null,
});

// Issues by project. repos: [{ dir, path }] (local repositories and their GitLab project path); sessions: [{ id, path, iid }]
// (iid: the issue the session works on, see issueOfSession).
// Returns { local: [{ path, dir, issues }], away: [{ path, issues }] }; each issue gets `sessions`: the ids working on it.
function groupIssues(issues, repos, sessions) {
  const dirOf = new Map(repos.filter(r => r.path).map(r => [r.path, r.dir]));
  const groups = new Map();
  for (const i of issues) {
    const working = sessions.filter(s => s.path === i.project && s.iid === i.iid).map(s => s.id);
    if (!groups.has(i.project)) groups.set(i.project, { path: i.project, dir: dirOf.get(i.project) || null, issues: [] });
    groups.get(i.project).issues.push({ ...i, sessions: working });
  }
  const all = [...groups.values()].sort((a, b) => a.path.localeCompare(b.path));
  all.forEach(g => g.issues.sort((a, b) => (b.sessions.length - a.sessions.length) || ((a.due || '9') < (b.due || '9') ? -1 : (a.due || '9') > (b.due || '9') ? 1 : b.iid - a.iid)));
  return { local: all.filter(g => g.dir), away: all.filter(g => !g.dir).map(({ dir, ...g }) => g) };
}

// Open issues assigned to you on `host`, through `glab api` (GET). { ok, issues } or { ok: false, reason: noglab | auth | error, detail }.
// ponytail: first 100 only (one page), add paging when someone has more open issues than that.
async function fetchIssues(glab, host) {
  if (!glab) return { ok: false, reason: 'noglab' };
  const r = await run(glab, ['api', '--hostname', host, 'issues?scope=assigned_to_me&state=opened&per_page=100'], { timeout: 30000 });
  if (r.err) {
    if (r.err.code === 'ENOENT') return { ok: false, reason: 'noglab' };
    const text = `${r.errOut} ${r.out}`;
    if (/\b401\b|unauthori[sz]ed|not logged|no token|authenticat/i.test(text)) return { ok: false, reason: 'auth' };
    return { ok: false, reason: 'error', detail: text.replace(/\s+/g, ' ').trim().slice(0, 200) };
  }
  try { const a = JSON.parse(r.out); if (Array.isArray(a)) return { ok: true, issues: a.map(slimIssue) }; } catch {}
  return { ok: false, reason: 'error', detail: 'unexpected answer from GitLab' };
}

module.exports = { parseStatus, gitStatus, hostOf, projectPath, issueOfBranch, issueOfSession, sessionTitleFor, slimIssue, groupIssues, fetchIssues };
