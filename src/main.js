// Bobix — Electron main process: local server for hooks, pets overlay, control panel, tray, notifications.
const { app, BrowserWindow, Tray, Menu, screen, ipcMain, Notification, nativeImage, shell, desktopCapturer, clipboard } = require('electron');
const path = require('path');
const fs = require('fs');
const os = require('os');
const net = require('net');
const { execFile } = require('child_process');
const { apply, refresh, publicView, setPet } = require('./core/state');
const S = require('./core/settings');
const hooks = require('./core/hooks');
const autostart = require('./core/autostart');
const { focusPids, ancestors, processNames, hostApp } = require('./core/focus');
const { startServer } = require('./core/server');
const { bitmap } = require('./core/icon');
const I18N = require('./ui/i18n');

// Linux on Wayland: apps may not keep a window on top or position it, so the pets overlay would sit behind
// whatever you work in. Run through XWayland (X11) when it is there; BOBIX_WAYLAND=1 keeps native Wayland.
// Electron picks Wayland before this script runs, so a switch set from here is too late: restart once with the flag.
const WAYLAND = process.platform === 'linux' && (process.env.XDG_SESSION_TYPE === 'wayland' || !!process.env.WAYLAND_DISPLAY);
const OZONE = (process.argv.find(a => a.startsWith('--ozone-platform=')) || '').split('=')[1] || 'default';
if (WAYLAND && process.env.DISPLAY && process.env.BOBIX_WAYLAND !== '1' && OZONE === 'default') {
  // Started directly, not with app.relaunch: an AppImage runs from a temporary mount that disappears with this
  // process, and Electron's relaunch helper runs from that mount. Safe to overlap: this process holds no lock yet.
  require('child_process').spawn(process.env.APPIMAGE || process.execPath, [...process.argv.slice(1), '--ozone-platform=x11'], { detached: true, stdio: 'inherit' }).unref();
  app.exit(0);
}

if (!app.requestSingleInstanceLock()) { app.quit(); process.exit(0); }

const STALE_MS = 12 * 3600e3;
const sessions = new Map();
let settings, server, overlay, panel, tray, token, nodePath = null, lastEventAt = null;
const shown = new Set(); // live Notification objects: a collected one loses its click handler
let rects = [], interactive = false, lastMouse = '', overlayReady = false, timers = [];
const pageErrors = []; // last errors from the pets page, for diagnostics

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
// Returns { ok } or { ok: false, reason } so the panel can say why (nopid | wayland | xdotool | notfound).
// tell: show the reason in the panel (pet clicks); the panel's own Open button shows it itself.
async function openSession(id, preferFocus = settings.clickAction === 'focus', tell = true) {
  const s = sessions.get(id);
  if (!s) return { ok: false, reason: 'notfound' };
  let reason = 'notfound', app = null;
  if (preferFocus) {
    try {
      if (s.hostSession) { await shell.openExternal(`claude://code/continue?session=${encodeURIComponent(s.hostSession)}`); return { ok: true }; }
      if (!s.claudePid) reason = 'nopid'; // hooks run without Node.js (curl / PowerShell): no process to start from
      else {
        const chain = await ancestors(s.claudePid);
        if (await focusPids(chain)) return { ok: true };
        app = hostApp(await processNames(chain));
        if (process.platform === 'linux') reason = WAYLAND ? 'wayland' : (await hasXdotool()) ? 'notfound' : 'xdotool';
      }
    } catch (err) { console.error('[bobix] open session:', err.message); }
  }
  // Say why, and where the session is, so you can switch to it yourself.
  const fail = preferFocus ? { reason, app: app || '', folder: s.cwd || '' } : null;
  showPanel({ select: id, fail: tell ? fail : null });
  return { ok: false, ...(fail || { reason }) };
}
const hasXdotool = () => new Promise(res => execFile('xdotool', ['version'], err => res(!err)));

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
  overlay.setBounds(overlayBounds());
  overlay.webContents.on('will-navigate', e => e.preventDefault());
  overlay.webContents.on('console-message', (e, level, message) => {
    const lvl = e.level ?? level, msg = String(e.message ?? message);
    if ((lvl === 'error' || lvl === 3) && !/Electron Security Warning/.test(msg)) { pageErrors.push(msg.slice(0, 300)); if (pageErrors.length > 20) pageErrors.shift(); }
  });
  overlay.webContents.once('did-finish-load', () => overlay.webContents.executeJavaScript(
    `addEventListener('error', e => console.error('uncaught: ' + e.message + ' @' + e.lineno)); addEventListener('unhandledrejection', e => console.error('rejection: ' + e.reason))`).catch(() => {}));
  overlay.loadURL(pageUrl('/pets'));
  overlay.once('ready-to-show', () => { overlayReady = true; showOverlay(); });
}

// A full-screen transparent window costs the compositor even when empty: keep it hidden while no pet is out.
function showOverlay() {
  if (!overlay || overlay.isDestroyed() || !overlayReady) return;
  const want = settings.petsVisible && rects.length > 0;
  if (want && !overlay.isVisible()) overlay.showInactive();
  else if (!want && overlay.isVisible()) { overlay.hide(); interactive = false; overlay.setIgnoreMouseEvents(true); }
}

// Only a strip along the chosen edge, tall enough for a pet, its card and a few bubble lanes:
// the compositor redraws this window while pets walk, so every pixel it does not cover is saved work.
function overlayBounds() {
  const wa = targetDisplay().workArea, h = Math.min(wa.height, Math.round(settings.size * 1.5) + 320);
  return { x: wa.x, width: wa.width, height: h, y: settings.edge === 'top' ? wa.y : wa.y + wa.height - h };
}
const placeOverlay = () => overlay && !overlay.isDestroyed() && overlay.setBounds(overlayBounds());

// The overlay ignores the mouse except over a pet: poll the cursor, compare with the pets' rects from the page.
function trackMouse() {
  if (!overlay || overlay.isDestroyed() || !overlay.isVisible()) return;
  const p = screen.getCursorScreenPoint(), b = overlay.getBounds();
  const x = p.x - b.x, y = p.y - b.y;
  const over = rects.some(r => x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]);
  if (over !== interactive) { interactive = over; overlay.setIgnoreMouseEvents(!over); }
  const k = `${x},${y}`;
  if (k !== lastMouse) { lastMouse = k; overlay.webContents.send('mouse', { x, y }); }
}

function showPanel({ select, tab, fail } = {}) {
  if (panel && !panel.isDestroyed()) {
    if (select) panel.webContents.send('select', select);
    if (tab) panel.webContents.send('show-tab', tab);
    if (fail) panel.webContents.send('open-fail', fail);
    panel.show(); panel.focus();
  } else {
    panel = new BrowserWindow({
      width: 1180, height: 780, minWidth: 420, minHeight: 480, title: 'Bobix', backgroundColor: '#f3f4f8',
      webPreferences: { preload: path.join(__dirname, 'preload.js') },
    });
    panel.removeMenu();
    panel.webContents.on('will-navigate', e => e.preventDefault());
    panel.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    panel.loadURL(pageUrl('/', { ...(select && { s: select }), ...(tab && { tab }), ...(fail && { fail: JSON.stringify(fail) }) }));
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
    { label: tr('tray.showAll'), click: () => { setPet(sessions, '*', 'shown'); server.broadcast(snapshot()); if (!settings.petsVisible) updateSettings({ petsVisible: true }); } },
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
  if (['display', 'edge', 'size'].some(k => settings[k] !== prev[k])) placeOverlay();
  if (settings.petsVisible !== prev.petsVisible) showOverlay();
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
  handle('diagnostics', async () => { const text = JSON.stringify(await diagnostics(), null, 2); clipboard.writeText(text); return text; });
  handle('hooks:install', () => installHooks());
  handle('hooks:uninstall', () => { try { return hooks.uninstall(); } catch (err) { return { error: err.message }; } });
  handle('hooks:test', () => testEvent());
  handle('open-session', id => openSession(id, true, false));
  ipcMain.on('pet-click', (e, id) => fromUs(e) && openSession(id));
  handle('pet:set', (id, mode) => { if (typeof id === 'string' && setPet(sessions, id, mode)) server.broadcast(snapshot()); });
  ipcMain.on('rects', (e, r) => { if (fromUs(e) && Array.isArray(r)) { rects = r.filter(x => Array.isArray(x) && x.length === 4 && x.every(Number.isFinite)); showOverlay(); } });
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

// Smoke check for CI (BOBIX_SMOKE=<folder>): a pretend session, then is the pets overlay really on screen?
// Writes report.json first, then overlay.png (the page) and screen.png (the real desktop). Every step is time-boxed
// (a never-shown window can leave capturePage pending forever); exits 0 only if pets are visible.
async function smoke(dir) {
  const log = m => console.log(`[bobix] smoke: ${m}`);
  const within = (p, ms, what) => Promise.race([p, new Promise((_, no) => setTimeout(() => no(new Error(`${what} timed out after ${ms} ms`)), ms))]);
  const report = { platform: process.platform, wayland: WAYLAND, ozone: OZONE };
  const write = () => fs.writeFileSync(path.join(dir, 'report.json'), JSON.stringify(report, null, 2));
  fs.mkdirSync(dir, { recursive: true });
  setTimeout(() => { report.watchdog = 'gave up after 60 s'; write(); log(JSON.stringify(report)); app.exit(2); }, 60000).unref?.();
  const wc = overlay.webContents;
  // Collect every error the pets page logs or throws while sessions arrive.
  report.pageErrors = [];
  wc.on('console-message', (e, level, message) => {
    const lvl = e.level ?? level, msg = e.message ?? message;
    if ((lvl === 'error' || lvl === 3) && !/Electron Security Warning/.test(msg)) report.pageErrors.push(String(msg).slice(0, 300));
  });
  await within(wc.executeJavaScript(`addEventListener('error', e => console.error('uncaught: ' + e.message + ' @' + e.lineno)); addEventListener('unhandledrejection', e => console.error('rejection: ' + e.reason)); 1`), 5000, 'hook errors').catch(() => {});
  testEvent();
  // A session delivered the way Claude Code delivers it: hook.js on stdin, real cwd, a transcript, CLAUDE_PID.
  const transcript = path.join(dir, 'real-session.jsonl');
  fs.writeFileSync(transcript, [
    { type: 'ai-title', aiTitle: 'Smoke real session' },
    { type: 'assistant', message: { content: [{ type: 'text', text: 'Looking at the code.' }], usage: { input_tokens: 10, cache_read_input_tokens: 40000 } } },
  ].map(l => JSON.stringify(l)).join('\n') + '\n');
  const real = { session_id: '3f1c2a9e-7b4d-4e21-9a0c-5d6e7f8a9b0c', cwd: process.cwd(), transcript_path: transcript };
  const viaHook = ev => new Promise(res => {
    const child = require('child_process').execFile(process.execPath, [path.join(__dirname, 'hook', 'hook.js'), String(settings.port)],
      { env: { ...process.env, ELECTRON_RUN_AS_NODE: '1', CLAUDE_PID: String(process.pid) } }, err => res(err ? err.message : 'ok'));
    child.stdin.end(JSON.stringify({ ...real, ...ev }));
  });
  report.hookRuns = [];
  for (const ev of [{ hook_event_name: 'SessionStart', source: 'startup' }, { hook_event_name: 'UserPromptSubmit', prompt: 'Fix the login bug' },
    { hook_event_name: 'PreToolUse', tool_name: 'Read', tool_input: { file_path: path.join(process.cwd(), 'README.md') } }]) report.hookRuns.push(await viaHook(ev));
  await new Promise(r => setTimeout(r, 6000));
  const rs = sessions.get(real.session_id);
  report.realSession = rs ? { state: rs.state, title: rs.title, name: rs.name, claudePid: rs.claudePid, hostSession: rs.hostSession } : 'not received';
  report.chain = await ancestors(process.pid).catch(e => `error: ${e.message}`);
  report.focus = await within(focusPids(Array.isArray(report.chain) ? report.chain : []), 8000, 'focus').catch(e => `error: ${e.message}`);
  Object.assign(report, {
    readyToShow: overlayReady, visible: overlay.isVisible(), minimized: overlay.isMinimized(), bounds: overlay.getBounds(),
    workArea: targetDisplay().workArea, sessions: sessions.size, loading: wc.isLoading(), loadedUrl: wc.getURL().replace(/t=[^&]+/, 't=…'),
  });
  log('window ' + JSON.stringify(report));
  for (const [key, js] of [['pets', 'document.querySelectorAll(".pet").length'], ['petLabels', '[...document.querySelectorAll(".pet .tag")].map(t => t.textContent)'],
    ['petRects', '[...document.querySelectorAll(".pet")].map(p => { const r = p.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round); })']]) {
    try { report[key] = await within(wc.executeJavaScript(js), 5000, key); } catch (err) { report[key] = `error: ${err.message}`; }
  }
  write(); log('page ' + JSON.stringify({ pets: report.pets, petRects: report.petRects }));
  try { fs.writeFileSync(path.join(dir, 'overlay.png'), (await within(wc.capturePage(), 8000, 'capturePage')).toPNG()); } catch (err) { report.overlayShot = err.message; }
  try {
    const [src] = await within(desktopCapturer.getSources({ types: ['screen'], thumbnailSize: targetDisplay().size }), 8000, 'desktopCapturer');
    if (src) fs.writeFileSync(path.join(dir, 'screen.png'), src.thumbnail.toPNG());
  } catch (err) { report.screenShot = err.message; }
  write(); log('done ' + JSON.stringify(report));
  fs.writeFileSync(path.join(dir, 'done'), ''); // CI waits for this, then stops the app itself
  app.exit(report.visible && report.pets >= 2 && !report.pageErrors.length ? 0 : 1);
}

// "Copy diagnostics" in Setup: everything needed to debug pets / opening sessions, no prompts, tokens or full paths.
async function diagnostics() {
  const wc = overlay?.webContents;
  const page = wc ? await wc.executeJavaScript(`({ pets: document.querySelectorAll('.pet').length, labels: [...document.querySelectorAll('.pet .tag')].map(t => t.textContent.length) })`).catch(e => ({ error: e.message })) : null;
  const { language, edge, size, opacity, showIdle, sleepHide, onlyAttention, petsVisible, zoneStart, zoneEnd, display, clickAction } = settings;
  const st = hooks.status();
  return {
    app: app.getVersion(), platform: process.platform, release: os.release(), arch: process.arch,
    session: process.env.XDG_SESSION_TYPE || null, wayland: WAYLAND, ozone: OZONE,
    settings: { language, edge, size, opacity, showIdle, sleepHide, onlyAttention, petsVisible, zoneStart, zoneEnd, display, clickAction },
    hooks: { mode: st.mode, complete: st.complete, port: st.port, stale: st.stale, outdated: st.outdated, error: st.error || null }, node: !!nodePath,
    displays: screen.getAllDisplays().map(d => ({ id: d.id, bounds: d.bounds, workArea: d.workArea, scale: d.scaleFactor, primary: d.id === screen.getPrimaryDisplay().id })),
    overlay: overlay && { ready: overlayReady, visible: overlay.isVisible(), bounds: overlay.getBounds(), alwaysOnTop: overlay.isAlwaysOnTop(), page, errors: pageErrors },
    sessions: await Promise.all([...sessions.values()].map(async s => {
      const chain = s.claudePid ? await ancestors(s.claudePid).catch(() => []) : [];
      return { state: s.state, sinceMin: Math.round((Date.now() - s.since) / 60000), events: s.log.length, title: !!s.title,
        claudePid: !!s.claudePid, hostSession: !!s.hostSession, hostShape: s.diag?.hostShape ?? null, env: s.diag?.env ?? null,
        chain: await processNames(chain).catch(() => []) };
    })),
  };
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
  timers.push(setInterval(trackMouse, 33));
  timers.push(setInterval(() => {
    const now = Date.now();
    let changed = false;
    for (const [id, s] of sessions) {
      if (now - s.updated > STALE_MS) { sessions.delete(id); changed = true; continue; }
      // Esc / a denied permission ends the turn without any hook: notice it in the transcript.
      if ((s.state === 'working' || s.state === 'waiting') && refresh(s, false, now)) changed = true;
    }
    if (changed) server.broadcast(snapshot());
  }, 5e3));
  screen.on('display-metrics-changed', placeOverlay);
  screen.on('display-added', placeOverlay);
  screen.on('display-removed', placeOverlay);
  if (process.env.BOBIX_SMOKE) return smoke(process.env.BOBIX_SMOKE);
  if (!hooks.status().events.length) showPanel({ tab: 'setup' }); // first run: help connect Claude Code
});

app.on('before-quit', () => { timers.forEach(clearInterval); if (settings) saveNow(); server?.close(); });
