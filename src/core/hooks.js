// Installs / removes Bobix hooks in ~/.claude/settings.json without touching anyone else's hooks.
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');

const EVENTS = ['SessionStart', 'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'PermissionRequest', 'Notification', 'Stop', 'SessionEnd'];
const TOOL_EVENTS = new Set(['PreToolUse', 'PostToolUse', 'PermissionRequest']);
const MARK = 'bobix';
// PermissionRequest waits for an answer from Bobix (node forwarder only), so it can't run in the background.
// The app decides how long it waits (Settings); this is only the hard cap Claude Code enforces.
const ANSWER_EVENT = 'PermissionRequest', ANSWER_TIMEOUT = 330;
const KEEP_BACKUPS = 3;

const claudeDir = (home = os.homedir()) => path.join(home, '.claude');
const appDir = home => path.join(claudeDir(home), MARK);
const settingsPath = home => path.join(claudeDir(home), 'settings.json');
const hookTarget = home => path.join(appDir(home), 'hook.js');
const tokenPath = home => path.join(appDir(home), 'token');
const headerPath = home => path.join(appDir(home), 'header'); // "X-Bobix: <token>", for curl -H @file

// Per-user secret the hook sends and the server requires: another local user can't feed or read the app.
function ensureToken(home) {
  fs.mkdirSync(appDir(home), { recursive: true, mode: 0o700 });
  let token = '';
  try { token = fs.readFileSync(tokenPath(home), 'utf8').trim(); } catch {}
  if (!/^[a-f0-9]{32,}$/.test(token)) {
    token = crypto.randomBytes(24).toString('hex');
    fs.writeFileSync(tokenPath(home), token + '\n', { mode: 0o600 });
  }
  fs.writeFileSync(headerPath(home), `X-Bobix: ${token}\n`, { mode: 0o600 });
  return token;
}

// Exact fingerprints only, so a foreign hook that merely mentions "bobix" or a local port is never touched.
const LEGACY = /^curl -s -m 1 -o \/dev\/null -X POST -H "Content-Type: application\/json" --data-binary @- http:\/\/127\.0\.0\.1:\d+\/event( 2>\/dev\/null)? \|\| true$/;
// The app was called Session Guild before: its hooks are ours too, and get replaced on the next install.
const OLD_MARK = 'session-guild';
const markOf = h => {
  if (!h || typeof h !== 'object') return null;
  const args = Array.isArray(h.args) ? h.args.filter(a => typeof a === 'string') : [];
  for (const m of [MARK, OLD_MARK]) {
    if (args.some(a => a.replace(/\\/g, '/').endsWith(`/.claude/${m}/hook.js`))) return m; // node
    if (h.command === 'curl' && args.some(a => a.endsWith(`/event#${m}`))) return m;
    if (h.command === 'powershell' && args.some(a => a.endsWith(`# ${m}`))) return m;
  }
  return typeof h.command === 'string' && !args.length && LEGACY.test(h.command.trim()) ? OLD_MARK : null; // shell-string hook from early builds
};
const isOurs = h => markOf(h) !== null;

// Exec form (command + args, no shell) works the same in bash, zsh, PowerShell and cmd.
function hookEntry({ port, nodePath, platform = process.platform, home, event }) {
  const base = { type: 'command', async: true, timeout: 5 };
  if (nodePath) {
    const entry = { ...base, command: nodePath, args: [hookTarget(home), String(port)] };
    if (event === ANSWER_EVENT) { delete entry.async; entry.timeout = ANSWER_TIMEOUT; }
    return entry;
  }
  const url = `http://127.0.0.1:${port}/event`;
  if (platform === 'win32') {
    // No node: PowerShell forwards stdin as UTF-8 (no window lookup, so "focus" falls back to the panel).
    const tok = tokenPath(home).replace(/'/g, "''");
    return { ...base, command: 'powershell', args: ['-NoProfile', '-Command',
      `try { [Console]::InputEncoding = [Text.Encoding]::UTF8; $t = (Get-Content -Raw '${tok}').Trim(); ` +
      `Invoke-RestMethod -Uri '${url}' -Method Post -ContentType 'application/json; charset=utf-8' -Headers @{'x-bobix'=$t} ` +
      `-Body ([Text.Encoding]::UTF8.GetBytes([Console]::In.ReadToEnd())) -TimeoutSec 2 | Out-Null } catch {} # ${MARK}`] };
  }
  return { ...base, command: 'curl', args: ['-s', '-m', '1', '-o', '/dev/null', '-X', 'POST', '-H', 'Content-Type: application/json',
    '-H', `@${headerPath(home)}`, '--data-binary', '@-', `${url}#${MARK}`] };
}

function readSettings(file) {
  if (!fs.existsSync(file)) return {};
  const txt = fs.readFileSync(file, 'utf8');
  if (!txt.trim()) return {};
  const obj = JSON.parse(txt); // invalid JSON throws: never overwrite a file we cannot read
  if (!obj || typeof obj !== 'object' || Array.isArray(obj)) throw new Error('settings.json is not a JSON object');
  return obj;
}

// Writes through a symlink (dotfiles repos) and keeps the file's permissions (it may hold tokens in "env").
function writeAtomic(file, obj) {
  const real = fs.existsSync(file) ? fs.realpathSync(file) : file;
  fs.mkdirSync(path.dirname(real), { recursive: true });
  const mode = fs.existsSync(real) ? fs.statSync(real).mode & 0o777 : 0o600;
  const tmp = `${real}.${process.pid}.tmp`;
  fs.writeFileSync(tmp, JSON.stringify(obj, null, 2) + '\n', { mode });
  fs.chmodSync(tmp, mode); // umask may have narrowed it
  fs.renameSync(tmp, real);
}

const groupsOf = (hooks, ev) => (Array.isArray(hooks?.[ev]) ? hooks[ev] : []);
const entriesOf = g => (g && typeof g === 'object' && Array.isArray(g.hooks) ? g.hooks : []);

// Removes our hooks; drops groups/events that become empty only because of us.
function strip(settings) {
  const hooks = settings.hooks;
  if (!hooks || typeof hooks !== 'object' || Array.isArray(hooks)) return settings;
  for (const ev of Object.keys(hooks)) {
    if (!Array.isArray(hooks[ev])) continue;
    const before = hooks[ev].length;
    hooks[ev] = hooks[ev].map(g => {
      const list = entriesOf(g);
      const kept = list.filter(h => !isOurs(h));
      return kept.length === list.length ? g : (kept.length ? { ...g, hooks: kept } : null);
    }).filter(g => g !== null);
    if (!hooks[ev].length && before) delete hooks[ev];
  }
  if (!Object.keys(hooks).length) delete settings.hooks;
  return settings;
}

function backup(file) {
  if (!fs.existsSync(file)) return null;
  const b = `${file}.bak-${MARK}-${new Date().toISOString().replace(/[:.]/g, '-')}`;
  fs.copyFileSync(file, b);
  fs.chmodSync(b, fs.statSync(file).mode & 0o777);
  // Keep only the newest few of our own backups.
  const dir = path.dirname(file), prefix = `${path.basename(file)}.bak-${MARK}-`;
  fs.readdirSync(dir).filter(f => f.startsWith(prefix)).sort().slice(0, -KEEP_BACKUPS).forEach(f => fs.rmSync(path.join(dir, f), { force: true }));
  return b;
}

function install({ port, nodePath, hookSource, home }) {
  const file = settingsPath(home);
  const settings = strip(readSettings(file));
  if (settings.hooks !== undefined && (typeof settings.hooks !== 'object' || Array.isArray(settings.hooks))) {
    throw new Error('"hooks" in settings.json is not an object; fix it by hand first');
  }
  ensureToken(home);
  if (nodePath) fs.copyFileSync(hookSource, hookTarget(home)); // stable path: survives app updates and moves
  settings.hooks ??= {};
  for (const ev of EVENTS) {
    if (settings.hooks[ev] !== undefined && !Array.isArray(settings.hooks[ev])) throw new Error(`hooks.${ev} in settings.json is not a list; fix it by hand first`);
    const entry = hookEntry({ port, nodePath, home, event: ev });
    const group = TOOL_EVENTS.has(ev) ? { matcher: '*', hooks: [entry] } : { hooks: [entry] };
    settings.hooks[ev] = [...groupsOf(settings.hooks, ev), group];
  }
  const bak = backup(file);
  writeAtomic(file, settings);
  fs.rmSync(path.join(claudeDir(home), OLD_MARK), { recursive: true, force: true }); // old build's forwarder + token
  return { file, backup: bak, mode: nodePath ? 'node' : (process.platform === 'win32' ? 'powershell' : 'curl') };
}

function uninstall({ home } = {}) {
  const file = settingsPath(home);
  let bak = null;
  if (fs.existsSync(file)) { const settings = strip(readSettings(file)); bak = backup(file); writeAtomic(file, settings); }
  for (const f of [hookTarget(home), headerPath(home)]) fs.rmSync(f, { force: true }); // token stays: the running app uses it
  return { file, backup: bak };
}

function status({ home } = {}) {
  const file = settingsPath(home);
  try {
    const hooks = readSettings(file).hooks;
    const events = EVENTS.filter(ev => groupsOf(hooks, ev).some(g => entriesOf(g).some(isOurs)));
    const one = EVENTS.flatMap(ev => groupsOf(hooks, ev).flatMap(entriesOf)).find(isOurs);
    const mode = !one ? null : one.command === 'curl' ? 'curl' : one.command === 'powershell' ? 'powershell' : Array.isArray(one.args) ? 'node' : 'legacy';
    const port = !one ? null : mode === 'node' ? Number(one.args[1]) || null : Number((JSON.stringify(one).match(/127\.0\.0\.1:(\d+)/) || [])[1]) || null;
    // node mode records an absolute node path: it goes stale when nvm / Homebrew remove that version.
    const stale = mode === 'node' && !fs.existsSync(one.command);
    // Installed by the old Session Guild build, or before Bobix could answer permission requests.
    const ask = groupsOf(hooks, ANSWER_EVENT).flatMap(entriesOf).find(isOurs);
    const outdated = !!one && (markOf(one) !== MARK || (mode === 'node' && !!ask?.async));
    return { file, events, complete: events.length === EVENTS.length, mode, port, nodePath: mode === 'node' ? one.command : null, stale, outdated };
  } catch (err) {
    return { file, error: `settings.json: ${err.message}`, events: [], complete: false, mode: null, port: null, stale: false };
  }
}

module.exports = { EVENTS, install, uninstall, status, strip, isOurs, hookEntry, settingsPath, hookTarget, tokenPath, ensureToken };
