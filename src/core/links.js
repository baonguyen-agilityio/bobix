// Which session a Claude desktop session was started from ("side session"), read from that app's own
// session file: <sessions dir>/<org>/<user>/<local_id>.json, field spawnedFrom.sessionId. Only that field is
// read. The format is undocumented: a missing file or field just means "no parent".
const fs = require('fs');
const os = require('os');
const path = require('path');

const ID = /^local_[A-Za-z0-9-]{1,64}$/;

// Verified on macOS. Windows and the Linux beta follow Electron's usual app-data folders (not verified yet).
function sessionsDir(platform = process.platform, home = os.homedir(), env = process.env) {
  if (platform === 'darwin') return path.join(home, 'Library', 'Application Support', 'Claude', 'claude-code-sessions');
  if (platform === 'win32') return path.join(env.APPDATA || path.join(home, 'AppData', 'Roaming'), 'Claude', 'claude-code-sessions');
  return path.join(env.XDG_CONFIG_HOME || path.join(home, '.config'), 'Claude', 'claude-code-sessions');
}

const dirs = d => { try { return fs.readdirSync(d, { withFileTypes: true }).filter(e => e.isDirectory()).map(e => path.join(d, e.name)); } catch { return []; } };

function parentOf(id, root = sessionsDir()) {
  if (!ID.test(id || '')) return null;
  for (const org of dirs(root)) for (const user of dirs(org)) {
    const file = path.join(user, `${id}.json`);
    let o;
    try { o = JSON.parse(fs.readFileSync(file, 'utf8')); } catch { continue; }
    const p = o?.spawnedFrom?.sessionId;
    return typeof p === 'string' && ID.test(p) && p !== id ? p : null;
  }
  return null;
}

// Claude desktop sessions that are not archived: [{ id, cwd, title, at }] (at: last activity, ms). Only these fields are
// kept. A file is parsed again only when it changed (cache: file -> { mtimeMs, rec }).
const seen = new Map();
function claudeSessions(root = sessionsDir()) {
  const out = [], files = new Set();
  for (const org of dirs(root)) for (const user of dirs(org)) {
    let names = [];
    try { names = fs.readdirSync(user).filter(n => ID.test(n.replace(/\.json$/, '')) && n.endsWith('.json')); } catch { continue; }
    for (const n of names) {
      const file = path.join(user, n);
      files.add(file);
      let st, c = seen.get(file);
      try { st = fs.statSync(file); } catch { continue; }
      if (!c || c.mtimeMs !== st.mtimeMs) {
        let o = null;
        try { o = JSON.parse(fs.readFileSync(file, 'utf8')); } catch {}
        c = { mtimeMs: st.mtimeMs, rec: o && !o.isArchived && typeof o.cwd === 'string' && typeof o.title === 'string'
          ? { id: n.slice(0, -5), cwd: o.cwd, title: o.title, at: Number(o.lastActivityAt) || st.mtimeMs } : null };
        seen.set(file, c);
      }
      if (c.rec) out.push(c.rec);
    }
  }
  for (const f of seen.keys()) if (!files.has(f)) seen.delete(f);
  return out;
}

// The newest of those sessions in folder `dir` named after an issue: "<project> #<iid>" alone or followed by a space
// (the name Bobix gives it, see work.sessionTitleFor). null when there is none.
function issueSession(list, dir, key, iid) {
  const p = `${key} #${iid}`;
  return list.filter(s => s.cwd === dir && (s.title === p || s.title.startsWith(p + ' '))).sort((a, b) => b.at - a.at)[0]?.id || null;
}

module.exports = { parentOf, sessionsDir, claudeSessions, issueSession };
