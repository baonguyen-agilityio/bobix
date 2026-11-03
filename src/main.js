// Bobix — Electron main process: local server for hooks, pets overlay, control panel, tray, notifications.
const { app, BrowserWindow, Tray, Menu, screen, ipcMain, Notification, nativeImage, shell } = require('electron');
const path = require('path');
const fs = require('fs');
const os = require('os');
const net = require('net');
const { execFile } = require('child_process');
const { apply, refresh, publicView } = require('./core/state');
const S = require('./core/settings');
const hooks = require('./core/hooks');
const autostart = require('./core/autostart');
const { focusPids, ancestors } = require('./core/focus');
const { startServer } = require('./core/server');
const { bitmap } = require('./core/icon');
const I18N = require('./ui/i18n');

if (!app.requestSingleInstanceLock()) { app.quit(); process.exit(0); }

const STALE_MS = 12 * 3600e3;
const sessions = new Map();
let settings, server, overlay, panel, tray, token, nodePath = null, lastEventAt = null;
const shown = new Set(); // live Notification objects: a collected one loses its click handler
let rects = [], interactive = false, lastMouse = '';

const settingsFile = () => path.join(app.getPath('userData'), 'settings.json');
const origin = () => `http://127.0.0.1:${settings.port}`;
const lang = () => (settings.language === 'auto' ? app.getLocale() : settings.language);
const tr = (k, v) => I18N.t(lang(), k, v);
const snapshot = () => ({ sessions: [...sessions.values()].map(publicView) });
const fromUs = e => (e.senderFrame?.url || '').startsWith(origin() + '/'); // IPC only from our own pages
const pageUrl = (p, q = {}) => `${origin()}${p}?${new URLSearchParams({ ...q, t: token })}`; // pages need the token to read /events

// ---------- hook events ----------
function onEvent(e) {
  lastEventAt = Date.now();
  const r = apply(sessions, e);
  if (!r) return;
  const isStop = e.hook_event_name === 'Stop', since = r.s?.since;
  const finish = () => {
    // A new prompt may land inside the Stop delay: then this Stop is stale (no old answer, no "finished" ping).
    const stale = isStop && (!r.s || r.s.state !== 'done' || r.s.since !== since);
    if (r.s && sessions.has(r.s.id) && !stale) refresh(r.s, isStop);
    server.broadcast(snapshot());
    if (r.notice && !stale) notify(r.notice);
  };
  isStop ? setTimeout(finish, 300) : finish(); // let Claude flush the final answer to the transcript first
}

function notify({ kind, id }) {
  const s = sessions.get(id);
  if (!s || settings.paused || !Notification.isSupported()) return;
  if ((kind === 'waiting' && !settings.notifyWaiting) || (kind === 'done' && !settings.notifyDone)) return;
  const n = new Notification({
    title: tr(`notify.${kind}`, { name: s.title || s.name }),
    body: kind === 'waiting' ? s.ask : (s.summary || s.quest),
    silent: !settings.sound,
  });
  shown.add(n);
  n.on('click', () => { shown.delete(n); openSession(id, true); });
  n.on('close', () => shown.delete(n));
  n.show();
}

// Open the session itself: in Claude desktop jump straight to it, else bring its terminal / IDE forward.
// Falls back to its details in the panel.
async function openSession(id, preferFocus = settings.clickAction === 'focus') {
  const s = sessions.get(id);
  if (!s) return false;
  if (preferFocus) {
    try {
      if (s.hostSession) { await shell.openExternal(`claude://code/continue?session=${encodeURIComponent(s.hostSession)}`); return true; }
      if (await focusPids(await ancestors(s.claudePid))) return true;
    } catch (err) { console.error('[bobix] open session:', err.message); }
  }
  showPanel({ select: id });
  return false;
}

// ---------- windows ----------
function targetDisplay() {
  const all = screen.getAllDisplays();
  return all.find(d => String(d.id) === settings.display) || screen.getPrimaryDisplay();
}

function createOverlay() {
  overlay = new BrowserWindow({
    show: false, transparent: true, frame: false, resizable: false, movable: false, minimizable: false, maximizable: false,
    fullscreenable: false, focusable: false, skipTaskbar: true, hasShadow: false, alwaysOnTop: true, backgroundColor: '#00000000',
    acceptFirstMouse: true, // macOS: the overlay is never key, so the first click must reach the page
    webPreferences: { preload: path.join(__dirname, 'preload.js'), backgroundThrottling: false },
  });
  overlay.setAlwaysOnTop(true, 'floating');
  overlay.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  overlay.setIgnoreMouseEvents(true);
  overlay.setBounds(targetDisplay().workArea);
  overlay.webContents.on('will-navigate', e => e.preventDefault());
  overlay.loadURL(pageUrl('/pets'));
  overlay.once('ready-to-show', () => settings.petsVisible && overlay.showInactive());
}

const placeOverlay = () => overlay && overlay.setBounds(targetDisplay().workArea);

// The overlay ignores the mouse except over a pet: poll the cursor, compare with the pets' rects from the page.
function trackMouse() {
  if (!overlay || !overlay.isVisible()) return;
  const p = screen.getCursorScreenPoint(), b = overlay.getBounds();
  const x = p.x - b.x, y = p.y - b.y;
  const over = rects.some(r => x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]);
  if (over !== interactive) { interactive = over; overlay.setIgnoreMouseEvents(!over); }
  const k = `${x},${y}`;
  if (k !== lastMouse) { lastMouse = k; overlay.webContents.send('mouse', { x, y }); }
}

function showPanel({ select, tab } = {}) {
  if (panel && !panel.isDestroyed()) {
    if (select) panel.webContents.send('select', select);
    if (tab) panel.webContents.send('show-tab', tab);
    panel.show(); panel.focus();
  } else {
    panel = new BrowserWindow({
      width: 1180, height: 780, minWidth: 420, minHeight: 480, title: 'Bobix', backgroundColor: '#f3f4f8',
      webPreferences: { preload: path.join(__dirname, 'preload.js') },
    });
    panel.removeMenu();
    panel.webContents.on('will-navigate', e => e.preventDefault());
    panel.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    panel.loadURL(pageUrl('/', { ...(select && { s: select }), ...(tab && { tab }) }));
    panel.on('closed', () => { panel = null; if (process.platform === 'darwin') app.dock?.hide(); });
  }
  if (process.platform === 'darwin') app.dock?.show();
}

const broadcastSettings = () => {
  for (const w of [overlay, panel]) if (w && !w.isDestroyed()) w.webContents.send('settings', settings);
};

// ---------- tray ----------
function buildTray() {
  // 32 px drawn for a 16 pt slot (2x), straight from pixels: no image file to ship.
  tray ??= new Tray(nativeImage.createFromBitmap(bitmap(32, 0, 'bgra'), { width: 32, height: 32, scaleFactor: 2 }));
  tray.setToolTip('Bobix');
  tray.setContextMenu(Menu.buildFromTemplate([
    { label: tr('tray.panel'), click: () => showPanel() },
    { label: tr(settings.petsVisible ? 'tray.hide' : 'tray.show'), click: () => updateSettings({ petsVisible: !settings.petsVisible }) },
    { label: tr('tray.pause'), type: 'checkbox', checked: settings.paused, click: m => updateSettings({ paused: m.checked }) },
    { type: 'separator' },
    { label: tr('tray.quit'), click: () => app.quit() },
  ]));
}

// ---------- settings ----------
const portFree = port => new Promise(res => {
  const t = net.createServer();
  t.once('error', () => res(false));
  t.listen(port, '127.0.0.1', () => t.close(() => res(true)));
});

let saveTimer;
const saveNow = () => { clearTimeout(saveTimer); S.save(settingsFile(), settings); };

async function updateSettings(patch) {
  const prev = settings;
  const next = S.sanitize({ ...settings, ...patch }, settings);
  if (next.port !== prev.port) {
    // Only move once the new port is free and the hooks point at it; otherwise the app could not start again.
    if (!(await portFree(next.port))) return { ...prev, error: `port ${next.port} is busy` };
    settings = next;
    if (hooks.status().events.length) {
      const r = installHooks();
      if (r.error) { settings = prev; return { ...prev, error: r.error }; }
    }
    saveNow();
    app.relaunch(); app.exit(0);
    return settings;
  }
  settings = next;
  clearTimeout(saveTimer);
  saveTimer = setTimeout(saveNow, 300); // sliders send a stream of changes
  if (settings.autostart !== prev.autostart) { try { autostart.set(app, settings.autostart); } catch (err) { console.error('[bobix] autostart:', err.message); } }
  if (settings.display !== prev.display) placeOverlay();
  if (settings.petsVisible !== prev.petsVisible && overlay) settings.petsVisible ? overlay.showInactive() : overlay.hide();
  if (['petsVisible', 'paused', 'language'].some(k => settings[k] !== prev[k])) buildTray();
  broadcastSettings();
  return settings;
}

// ---------- hooks setup ----------
// GUI apps don't get the shell's PATH (nvm, Homebrew…), so ask a login shell where node is.
function findNode() {
  const tryCmd = (cmd, args) => new Promise(res => execFile(cmd, args, { timeout: 5000, windowsHide: true }, (err, out) => {
    // `where` lists matches in PATH order (first wins); an interactive shell may print noise before the answer (last wins).
    const lines = String(out || '').split(/\r?\n/).map(l => l.trim()).filter(Boolean);
    const line = (process.platform === 'win32' ? lines : lines.reverse()).find(l => /node(\.exe)?$/i.test(l) && fs.existsSync(l));
    res(err && !line ? null : line || null);
  }));
  if (process.platform === 'win32') return tryCmd('where', ['node']);
  const sh = process.env.SHELL || '/bin/bash';
  return tryCmd(sh, ['-lc', 'command -v node']).then(p => p || tryCmd(sh, ['-lic', 'command -v node']));
}

function installHooks() {
  try {
    return hooks.install({ port: settings.port, nodePath, hookSource: path.join(__dirname, 'hook', 'hook.js') });
  } catch (err) { return { error: err.message }; }
}

// A pretend session walks through start → work → done so you can see pets and notifications without Claude.
function testEvent() {
  const id = `bobix-test-${Date.now().toString(36)}`, cwd = path.join(os.homedir(), 'bobix-test');
  const send = (name, extra = {}) => onEvent({ session_id: id, cwd, hook_event_name: name, ...extra });
  send('SessionStart', { source: 'test' });
  send('UserPromptSubmit', { prompt: tr('test.title') });
  setTimeout(() => send('PreToolUse', { tool_name: 'Read', tool_input: { file_path: 'README.md' } }), 1500);
  setTimeout(() => send('Stop'), 6000);
  setTimeout(() => send('SessionEnd'), 60000);
  return {};
}

function ipc() {
  const handle = (ch, fn) => ipcMain.handle(ch, (e, ...a) => (fromUs(e) ? fn(...a) : null));
  handle('state', () => ({
    settings, platform: process.platform, nodePath, lastEventAt, hooks: hooks.status(),
    screen: (({ width, height }) => ({ width, height }))(targetDisplay().workArea),
    displays: screen.getAllDisplays().map((d, i) => ({ id: d.id, label: `${d.label || `#${i + 1}`} (${d.size.width}×${d.size.height})` })),
  }));
  handle('settings', patch => updateSettings(patch));
  // Back to defaults, except the language you read and what changes the system (port → hooks + restart, start at login).
  handle('settings:reset', () => updateSettings({ ...S.DEFAULTS, language: settings.language, port: settings.port, autostart: settings.autostart }));
  handle('reveal-settings', () => shell.showItemInFolder(hooks.status().file));
  handle('hooks:install', () => installHooks());
  handle('hooks:uninstall', () => { try { return hooks.uninstall(); } catch (err) { return { error: err.message }; } });
  handle('hooks:test', () => testEvent());
  handle('open-session', id => openSession(id, true));
  ipcMain.on('pet-click', (e, id) => fromUs(e) && openSession(id));
  ipcMain.on('rects', (e, r) => { if (fromUs(e) && Array.isArray(r)) rects = r.filter(x => Array.isArray(x) && x.length === 4 && x.every(Number.isFinite)); });
}

// The app used to be called Session Guild: carry its settings and start-at-login over once.
function migrateFromOldName() {
  if (fs.existsSync(settingsFile())) return;
  const appData = app.getPath('appData');
  const old = ['Session Guild', 'session-guild'].map(d => path.join(appData, d, 'settings.json')).find(f => fs.existsSync(f));
  if (old) { fs.mkdirSync(path.dirname(settingsFile()), { recursive: true }); fs.copyFileSync(old, settingsFile()); }
  // Dev-mode autostart files carried the old name; packaged login items follow the app itself.
  const oldAuto = [path.join(os.homedir(), 'Library', 'LaunchAgents', 'dev.session-guild.plist'), path.join(os.homedir(), '.config', 'autostart', 'session-guild.desktop')].filter(f => fs.existsSync(f));
  if (oldAuto.length) {
    oldAuto.forEach(f => fs.rmSync(f, { force: true }));
    try { autostart.set(app, true); } catch (err) { console.error('[bobix] autostart:', err.message); }
  }
}

// ---------- boot ----------
app.on('second-instance', () => showPanel());
app.on('window-all-closed', () => {}); // keep running in the tray
app.on('activate', () => showPanel());

app.whenReady().then(async () => {
  if (process.platform === 'darwin') app.dock?.hide();
  migrateFromOldName();
  settings = S.load(settingsFile());
  try { settings.autostart = autostart.get(app); } catch {}
  token = hooks.ensureToken();
  server = startServer({ port: settings.port, token, onEvent, snapshot });
  try { await server.ready; } catch (err) {
    const { dialog } = require('electron');
    dialog.showErrorBox('Bobix', `Port ${settings.port} is busy (${err.code}). Is Bobix already running?\nChange "port" in ${settingsFile()}`);
    return app.exit(1);
  }
  ipc();
  nodePath = await findNode();
  try {
    const st = hooks.status();
    if (st.outdated || (st.mode === 'node' && st.stale && nodePath)) installHooks(); // old name, or recorded node removed (nvm/Homebrew)
    else if (st.mode === 'node' && fs.existsSync(hooks.hookTarget())) fs.copyFileSync(path.join(__dirname, 'hook', 'hook.js'), hooks.hookTarget()); // pick up app updates
  } catch (err) { console.error('[bobix] hook refresh:', err.message); }
  createOverlay();
  buildTray();
  setInterval(trackMouse, 33);
  setInterval(() => {
    const now = Date.now();
    let changed = false;
    for (const [id, s] of sessions) {
      if (now - s.updated > STALE_MS) { sessions.delete(id); changed = true; continue; }
      // Esc / a denied permission ends the turn without any hook: notice it in the transcript.
      if ((s.state === 'working' || s.state === 'waiting') && refresh(s, false, now)) changed = true;
    }
    if (changed) server.broadcast(snapshot());
  }, 5e3);
  screen.on('display-metrics-changed', placeOverlay);
  screen.on('display-added', placeOverlay);
  screen.on('display-removed', placeOverlay);
  if (!hooks.status().events.length) showPanel({ tab: 'setup' }); // first run: help connect Claude Code
});

app.on('before-quit', () => { if (settings) saveNow(); server?.close(); });
