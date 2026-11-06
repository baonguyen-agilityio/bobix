// Brings the window that runs a session to the front, using its process tree
// (claude → shell → terminal / IDE / Claude app), walked from the claude pid the hook sends.
const { execFile } = require('child_process');

const run = (cmd, args) => new Promise(res => execFile(cmd, args, { timeout: 4000, windowsHide: true }, err => res(!err)));
// PowerShell can take several seconds to start on a cold Windows machine.
const out = (cmd, args) => new Promise(res => execFile(cmd, args, { timeout: cmd === 'powershell' ? 15000 : 4000, windowsHide: true }, (err, so) => res(err ? '' : String(so).trim())));

// macOS: the outermost app bundle in the chain is the one with the window (Terminal, iTerm, VS Code, Claude…).
// Helpers live inside it ("…/Code.app/Contents/Frameworks/Code Helper.app/…"), so take the first ".app" of each path.
function outerBundle(execPaths) {
  const bundles = execPaths.map(p => (/^(\/.*?\.app)(\/|$)/.exec(p) || [])[1]).filter(Boolean);
  return bundles.at(-1) || null;
}

// claude → shell → … → terminal / IDE / app. Walked when a pet is clicked (the hook only sends the claude pid).
async function ancestors(pid, platform = process.platform) {
  const chain = [];
  if (!Number.isInteger(pid) || pid <= 1) return chain;
  if (platform === 'win32') {
    const txt = await out('powershell', ['-NoProfile', '-Command', 'Get-CimInstance Win32_Process | ForEach-Object { "$($_.ProcessId) $($_.ParentProcessId)" }']);
    const parent = new Map(txt.split(/\r?\n/).map(l => l.trim().split(/\s+/).map(Number)).filter(a => a.length === 2));
    for (let p = pid; p && chain.length < 16 && !chain.includes(p); p = parent.get(p)) chain.push(p);
    return chain;
  }
  for (let p = pid; p > 1 && chain.length < 16 && !chain.includes(p);) {
    chain.push(p);
    p = parseInt(await out('ps', ['-o', 'ppid=', '-p', String(p)]), 10);
  }
  return chain;
}

async function focusPids(pids, platform = process.platform) {
  const list = (pids || []).filter(p => Number.isInteger(p) && p > 1);
  if (!list.length) return false;
  if (platform === 'darwin') {
    // `open -a` activates the running app: no Automation permission needed (System Events would ask for one).
    const app = outerBundle(await Promise.all(list.map(p => out('ps', ['-o', 'comm=', '-p', String(p)]))));
    return app ? run('open', ['-a', app]) : false;
  }
  if (platform === 'win32') {
    return run('powershell', ['-NoProfile', '-Command',
      `$w = New-Object -ComObject WScript.Shell; foreach ($p in @(${list.join(',')})) { if ($w.AppActivate([int]$p)) { exit 0 } }; exit 1`]);
  }
  for (const p of list) { // Linux (X11): needs xdotool
    if (await run('xdotool', ['search', '--onlyvisible', '--pid', String(p), 'windowactivate'])) return true;
  }
  return false;
}

// Names of processes, for diagnostics ("claude", "Claude", "WindowsTerminal"…).
async function processNames(pids, platform = process.platform) {
  const list = (pids || []).filter(p => Number.isInteger(p) && p > 1);
  if (!list.length) return [];
  if (platform === 'win32') {
    // tasklist ships with Windows and answers in well under a second; PowerShell can take over 15 s to start cold.
    // CSV rows: "node.exe","5936","Console","1","45,000 K". Names lose ".exe", as PowerShell's ProcessName did.
    const txt = await out('tasklist', ['/FO', 'CSV', '/NH']);
    const names = new Map(txt.split(/\r?\n/).map(l => /^"([^"]*)","(\d+)"/.exec(l.trim())).filter(Boolean).map(m => [Number(m[2]), m[1].replace(/\.exe$/i, '')]));
    return list.map(p => names.get(p) || '?');
  }
  // Linux cuts `comm` to 15 characters ("gnome-terminal-"): read the program path from the command line there.
  const col = platform === 'linux' ? 'args=' : 'comm=';
  return Promise.all(list.map(async p => {
    const v = await out('ps', ['-o', col, '-p', String(p)]);
    return (platform === 'linux' ? v.split(/\s/)[0] : v).split('/').pop().replace(/^-/, '') || '?'; // "-bash": a login shell
  }));
}

// The app that holds the session's window, named the way people know it: the first process up the
// chain that is not claude itself, a runtime or a shell ("gnome-terminal-server" → "Terminal").
const SKIP = /^(claude|node|bun|npx|npm|sh|bash|zsh|fish|dash|ksh|tcsh|sudo|su|login|env|script|systemd|init)$/i;
const KNOWN = { 'gnome-terminal-server': 'Terminal', 'ptyxis-agent': 'Ptyxis', ptyxis: 'Ptyxis', kgx: 'Console', konsole: 'Konsole', 'xfce4-terminal': 'Xfce Terminal',
  tilix: 'Tilix', terminator: 'Terminator', alacritty: 'Alacritty', kitty: 'kitty', 'wezterm-gui': 'WezTerm', ghostty: 'Ghostty', code: 'VS Code', cursor: 'Cursor', tmux: 'tmux',
  'claude-desktop': 'Claude' };
function hostApp(names) {
  const n = (names || []).slice(1).find(x => x && x !== '?' && !SKIP.test(x));
  return n ? KNOWN[n.toLowerCase()] || KNOWN[n.toLowerCase().replace(/:.*$/, '')] || n : null;
}

module.exports = { focusPids, ancestors, outerBundle, processNames, hostApp };
