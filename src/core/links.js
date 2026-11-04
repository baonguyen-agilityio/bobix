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

module.exports = { parentOf, sessionsDir };
