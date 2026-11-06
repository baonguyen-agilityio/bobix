// Bobix — Electron main process: local server for hooks, pets overlay, control panel, tray, notifications.
const { app, BrowserWindow, Tray, Menu, screen, ipcMain, Notification, nativeImage, shell, desktopCapturer, clipboard, dialog } = require('electron');
const path = require('path');
const { Worker } = require('worker_threads');
const fs = require('fs');
const os = require('os');
const net = require('net');
const { execFile } = require('child_process');
const { apply, refresh, publicView, setPet, pinProject, gone, CLAUDE_PROC } = require('./core/state');
const S = require('./core/settings');
const hooks = require('./core/hooks');
const autostart = require('./core/autostart');
const { focusPids, ancestors, processNames, hostApp } = require('./core/focus');
const { startServer } = require('./core/server');
const { checkLatest } = require('./core/update');
const { parentOf } = require('./core/links');
const feedback = require('./core/feedback');
const { bitmap } = require('./core/icon');
const petstore = require('./core/petstore');
const I18N = require('./ui/i18n');
const news = require('./core/news');
const work = require('./core/work');

// Linux on Wayland: apps may not keep a window on top or position it, so the pets overlay would sit behind
// whatever you work in. Run through XWayland (X11) when it is there; BOBIX_WAYLAND=1 keeps native Wayland.
// Electron picks Wayland before this script runs, so a switch set from here is too late: restart once with the flag.
const WAYLAND = process.platform === 'linux' && (process.env.XDG_SESSION_TYPE === 'wayland' || !!process.env.WAYLAND_DISPLAY);
const OZONE = (process.argv.find(a => a.startsWith('--ozone-platform=')) || '').split('=')[1] || 'default';
if (WAYLAND && process.env.DISPLAY && process.env.BOBIX_WAYLAND !== '1' && OZONE === 'default') {
  // Started directly rather than with app.relaunch. Safe to overlap: this process holds no lock yet.
  require('child_process').spawn(process.execPath, [...process.argv.slice(1), '--ozone-platform=x11'], { detached: true, stdio: 'inherit' }).unref();
  app.exit(0);
}
// Linux on X11 (XWayland included): the overlay's window shape lets the mouse through everywhere but over the pets.
// XWayland only knows where the cursor is while it is over an X11 window, so polling the cursor would never see it
// reach a pet that sits over a Wayland app (most of them), and the pets could not be clicked or dragged there.
const SHAPED = process.platform === 'linux' && (OZONE === 'x11' || !WAYLAND);
const XWAYLAND = WAYLAND && OZONE === 'x11';

// One Bobix at a time. The copy that is opened tells the running one its version (see 'second-instance').
const ME = { version: app.getVersion() };
if (!app.requestSingleInstanceLock(ME)) { app.quit(); process.exit(0); }

const STALE_MS = 12 * 3600e3;
const sessions = new Map();
let settings, server, overlay, panel, tray, token, nodePath = null, lastEventAt = null;
const shown = new Set(); // live Notification objects: a collected one loses its click handler
let rects = [], interactive = false, lastMouse = '', overlayReady = false, timers = [];
const pageErrors = []; // last errors from the pets page, for diagnostics
const mainErrors = []; // last errors of this process, kept in memory for feedback
const logError = console.error;
console.error = (...a) => { mainErrors.push(a.join(' ').slice(0, 300)); if (mainErrors.length > 20) mainErrors.shift(); logError(...a); };

const settingsFile = () => path.join(app.getPath('userData'), 'settings.json');
const origin = () => `http://127.0.0.1:${settings.port}`;
const lang = () => (settings.language === 'auto' ? app.getLocale() : settings.language);
const tr = (k, v) => I18N.t(lang(), k, v);
// parent: the session (in Bobix) a Claude desktop side session was started from, when both are known.
const snapshot = () => {
  const byHost = new Map([...sessions.values()].filter(s => s.hostSession).map(s => [s.hostSession, s.id]));
  const pics = settings?.petImages || {};
  const w = workView();
  return { work: w.view, sessions: [...sessions.values()].map(s => {
    const project = s.project || '', pic = pics[project];
    return { ...publicView(s), parent: (s.parentHost && byHost.get(s.parentHost)) || null, project, image: petstore.served(pic), pixel: !!pic && typeof pic === 'object', whole: !!pic?.opts?.whole, issue: w.issueOf.get(s.id) || null };
  }) };
};
const fromUs = e => (e.senderFrame?.url || '').startsWith(origin() + '/'); // IPC only from our own pages
const pageUrl = (p, q = {}) => `${origin()}${p}?${new URLSearchParams({ ...q, t: token })}`; // pages need the token to read /events

// ---------- hook events ----------
// signal: set when the hook waits for an answer (PermissionRequest); returns { secs, answer } then (see waitForAnswer).
function onEvent(e, signal) {
  lastEventAt = Date.now();
  const r = apply(sessions, e);
  if (!r) return null;
  if (r.s) { pinProject(r.s); findParent(r.s); refreshGit(r.s, e.hook_event_name === 'Stop' || e.hook_event_name === 'SessionStart'); }
  const answered = signal && r.s && e.hook_event_name === 'PermissionRequest' ? waitForAnswer(r.s, e, signal) : null;
  const isStop = e.hook_event_name === 'Stop', since = r.s?.since;
  const finish = () => {
    // A new prompt may land inside the Stop delay: then this Stop is stale (no old answer, no "finished" ping).
    const stale = isStop && (!r.s || r.s.state !== 'done' || r.s.since !== since);
    if (r.s && sessions.has(r.s.id) && !stale) refresh(r.s, isStop);
    server.broadcast(snapshot());
    if (r.notice && !stale) notify(r.notice);
  };
  isStop ? setTimeout(finish, 300) : finish(); // let Claude flush the final answer to the transcript first
  // A session that starts on an issue's branch gets its name from Bobix, the way /rename names it; one that has a name keeps it.
  if (r.s && e.hook_event_name === 'SessionStart' && ['startup', 'resume', 'fork'].includes(e.source) && !e.session_title) return { output: titleFor(r.s) };
  return answered;
}

// Side session of another Claude desktop session? That app writes the link into its session file some time after
// the session starts (seen: after its first events), so look again every 5 s, for up to 10 minutes.
// Is the process there? Signal 0 checks without touching it; EPERM means it runs under another user.
const isAlive = pid => { try { process.kill(pid, 0); return true; } catch (err) { return err.code === 'EPERM'; } };
// Look once at what each new claudePid is (see gone in core/state.js).
function checkPid(s) {
  const pid = s.claudePid;
  s.pidChecked = pid;
  processNames([pid]).then(([name]) => { if (s.claudePid === pid && CLAUDE_PROC.test(name || '')) s.pidIsClaude = pid; }).catch(() => {});
}
function findParent(s, now = Date.now()) {
  if (!s.hostSession || s.parentHost || (s.linkTries || 0) >= 120 || now - (s.linkAt || 0) < 5000) return false;
  s.linkAt = now; s.linkTries = (s.linkTries || 0) + 1;
  s.parentHost = parentOf(s.hostSession);
  return !!s.parentHost;
}

// ---------- work items: git status, GitLab issues ----------
// Git: per folder, read on a session's events and by the 30 s timer, at most every 30 s (on Stop and SessionStart at once).
// GitLab: only while its switch is on and a URL is set, through the person's own glab (see core/work.js and docs/privacy.md).
const gitCache = new Map(); // folder -> { at, busy, git }
const remotesOf = new Map(); // repository top folder -> its remote URLs (kept here, never sent to the pages: they may hold credentials)
let glabPath = null, issues = { status: 'off' }, issuesBusy = false;
// A session is named after an issue only when its own folder is on that issue's branch ("<iid>-…") of a repository of
// that GitLab project, and the issue is one assigned to you. Nothing is guessed from timing; other sessions keep Claude's name.
async function titleFor(s) {
  const host = gitlabHost(), dir = s.cwd;
  if (!host || !dir || !path.isAbsolute(dir)) return null;
  const g = await work.gitStatus(dir).catch(() => null);
  const project = g && g.remotes.map(u => work.projectPath(u, host)).find(Boolean), iid = g && work.issueOfBranch(g.branch);
  const i = project && iid && (issues.list || []).find(x => x.project === project && x.iid === iid);
  return i ? { hookSpecificOutput: { hookEventName: 'SessionStart', sessionTitle: work.sessionTitleFor(project, iid, i.title) } } : null;
}
async function refreshGit(s, force = false) {
  const dir = s.cwd;
  if (!dir || !path.isAbsolute(dir)) return;
  if (gitCache.size > 300) gitCache.clear();
  const c = gitCache.get(dir) || {};
  gitCache.set(dir, c);
  if (c.git && !s.git) s.git = c.git;
  if (c.busy || (!force && Date.now() - (c.at || 0) < 30e3)) return;
  c.busy = true;
  let g = null;
  try { g = await work.gitStatus(dir); } catch (err) { console.error('[bobix] git status:', err.message); } finally { c.busy = false; c.at = Date.now(); }
  if (!g) return; // not a repository (a scratchpad…): the session keeps the status it had
  const { remotes, ...git } = g;
  remotesOf.set(git.top, remotes);
  const was = JSON.stringify(c.git);
  c.git = git;
  let changed = was !== JSON.stringify(git);
  for (const x of sessions.values()) if (x.cwd === dir && x.git !== git) { changed ||= JSON.stringify(x.git) !== was; x.git = git; }
  if (changed && server) server.broadcast(snapshot());
}
const gitlabHost = () => (settings.gitlab ? work.hostOf(settings.gitlabUrl) : '');
async function refreshIssues() {
  const host = gitlabHost();
  if (!host) { issues = { status: 'off' }; return server?.broadcast(snapshot()); }
  if (issuesBusy) return;
  issuesBusy = true;
  if (issues.host !== host) { issues = { status: 'loading', host }; server.broadcast(snapshot()); }
  try {
    glabPath ??= await findBin('glab');
    const r = await work.fetchIssues(glabPath, host);
    if (r.reason === 'noglab') glabPath = null; // look again next time: it may be installed by then
    if (gitlabHost() !== host) return; // switched off or to another GitLab meanwhile
    issues = r.ok ? { status: 'ok', host, at: Date.now(), list: r.issues } : { status: r.reason, host, at: Date.now(), detail: r.detail || '' };
  } catch (err) { issues = { status: 'error', host, at: Date.now(), detail: err.message }; } finally { issuesBusy = false; }
  server.broadcast(snapshot());
}
// What the pages see: issues grouped by project, and for each session the issue its branch names (GitLab's "<iid>-…").
function workView() {
  const host = gitlabHost(), issueOf = new Map();
  if (!host) return { view: { status: 'off' }, issueOf };
  const pathOf = top => (remotesOf.get(top) || []).map(u => work.projectPath(u, host)).find(Boolean) || null;
  const own = s => work.issueOfSession({ path: pathOf(s.git.top), branches: [...(s.branches || []), s.git.branch].filter(Boolean) });
  const byHost = new Map([...sessions.values()].filter(s => s.hostSession && s.git).map(s => [s.hostSession, s]));
  const live = [...sessions.values()].filter(s => s.git).map(s => {
    const path = pathOf(s.git.top), parent = s.parentHost && byHost.get(s.parentHost);
    const parentIid = parent && pathOf(parent.git.top) === path ? own(parent) : null; // a side session works on its parent's issue
    return { id: s.id, path, dir: s.project || s.git.top, iid: work.issueOfSession({ path, parentIid, branches: [...(s.branches || []), s.git.branch].filter(Boolean) }) };
  });
  const list = issues.list || [];
  for (const s of live) {
    if (s.iid) { const i = list.find(x => x.project === s.path && x.iid === s.iid); issueOf.set(s.id, { iid: s.iid, title: i?.title || '', url: i?.url || '' }); }
  }
  const { local, away } = work.groupIssues(list, live, live);
  return { view: { status: issues.status, host, at: issues.at || null, detail: issues.detail || '', local, away }, issueOf };
}
// The Work tab's button on an issue: Resume opens the session working on it, exactly like a click on its pet;
// New opens a new Claude desktop session in the issue's own folder: a git worktree on its branch (see work.issueWorktree),
// made the first time, so the session is that issue's by its branch (the link Claude documents, no prompt sent).
async function startIssue(path, iid) {
  const { view } = workView();
  const g = view.local?.find(x => x.path === path), i = g?.issues.find(x => x.iid === iid);
  if (!g || !i) return { ok: false, reason: 'gone' };
  const id = i.sessions.map(x => sessions.get(x)).filter(Boolean).sort((a, b) => b.updated - a.updated)[0]?.id;
  if (id) return { mode: 'resume', ...(await openSession(id, true, false)) };
  const wt = await work.issueWorktree(g.dir, iid, i.title);
  if (wt.error) return { ok: false, mode: 'new', reason: 'worktree', detail: wt.error };
  try { await shell.openExternal(`claude://code/new?folder=${encodeURIComponent(wt.dir)}`); } catch (err) { return { ok: false, mode: 'new', reason: err.message }; }
  return { ok: true, mode: 'new', dir: wt.dir, branch: wt.branch, created: wt.created };
}
// Only links to the configured GitLab open from the Work tab.
function openIssue(url) {
  const base = settings.gitlab && work.hostOf(settings.gitlabUrl) ? settings.gitlabUrl + '/' : null;
  if (!base || typeof url !== 'string' || !url.startsWith(base)) return false;
  shell.openExternal(url);
  return true;
}

// ---------- a project's own pet picture ----------
// Chosen in the panel from a file dialog run here (pages never get file access). The picture is turned into pixel art
// (core/pixelate.js, in a worker thread) with options the person adjusts in a preview; only Save writes anything.
// The original is kept next to the result, so the picture can be adjusted again later (core/petstore.js).
const petDir = () => path.join(app.getPath('userData'), 'pets');
const MAX_PICTURE = 2 * 1024 * 1024, DECODE_SIDE = 1024; // bytes of the file; long side before the pixel art shrinks it
const PIXEL_WAIT_MS = 20e3, WEBP_WAIT_MS = 15e3; // longer than this, a decoder is stuck on a broken file
let importing = null; // the one import being adjusted: { project, original, kind, input, token }

// One worker for the import in progress; it is stopped when the import ends, so it holds no memory in between.
let worker = null, workerToken = null, workerReq = 0;
function pixelWorker() {
  if (worker) return worker;
  const w = worker = new Worker(path.join(__dirname, 'core', 'pixelate-worker.js'));
  w.waiting = new Map(); // request id -> { resolve, reject, timer }; of this worker only, a stopped one must not fail the next
  const ended = err => {
    if (worker === w) { worker = null; workerToken = null; }
    for (const r of w.waiting.values()) { clearTimeout(r.timer); r.reject(err); }
    w.waiting.clear();
  };
  w.on('message', m => { const r = w.waiting.get(m.id); if (!r) return; w.waiting.delete(m.id); clearTimeout(r.timer); m.error ? r.reject(new Error(m.error)) : r.resolve(m); });
  w.on('error', err => ended(err));
  w.on('exit', () => ended(new Error('pixel worker stopped')));
  return w;
}
const stopWorker = () => { const w = worker; worker = null; workerToken = null; w?.terminate(); };
// A hung worker (a broken file in a decoder) is stopped after PIXEL_WAIT_MS, which rejects every request waiting on it.
const runPixel = msg => new Promise((resolve, reject) => {
  const w = pixelWorker(), id = ++workerReq;
  w.waiting.set(id, { resolve, reject, timer: setTimeout(() => w.terminate(), PIXEL_WAIT_MS) });
  w.postMessage({ ...msg, id });
});

// A WebP: nativeImage cannot read it, a page can. First frame only, drawn at no more than DECODE_SIDE.
async function decodeWebp(buf) {
  const win = new BrowserWindow({ show: false, width: 1, height: 1, webPreferences: { sandbox: true, contextIsolation: true } });
  try {
    await win.loadURL('about:blank');
    const b64 = await Promise.race([win.webContents.executeJavaScript(`(async () => {
      const bmp = await createImageBitmap(await (await fetch('data:image/webp;base64,${buf.toString('base64')}')).blob());
      const k = Math.min(1, ${DECODE_SIDE} / Math.max(bmp.width, bmp.height)), w = Math.max(1, Math.round(bmp.width * k)), h = Math.max(1, Math.round(bmp.height * k));
      const c = new OffscreenCanvas(w, h), g = c.getContext('2d');
      g.drawImage(bmp, 0, 0, w, h);
      const d = g.getImageData(0, 0, w, h).data;
      let s = '';
      for (let i = 0; i < d.length; i += 0x8000) s += String.fromCharCode(...d.subarray(i, i + 0x8000));
      return [w, h, btoa(s)];
    })()`), new Promise((_, no) => setTimeout(() => no(new Error('webp decode timed out')), WEBP_WAIT_MS))]);
    return { kind: 'rgba', width: b64[0], height: b64[1], data: Buffer.from(b64[2], 'base64') };
  } finally { win.destroy(); }
}

// What the worker needs to decode a picture; null when it cannot be read.
async function importInput(buf, kind) {
  if (kind === 'gif') return { kind: 'gif', data: buf };
  if (kind === 'webp') return decodeWebp(buf);
  let img = nativeImage.createFromBuffer(buf);
  if (img.isEmpty()) return null;
  const { width, height } = img.getSize();
  if (Math.max(width, height) > DECODE_SIDE) img = img.resize(width >= height ? { width: DECODE_SIDE, quality: 'best' } : { height: DECODE_SIDE, quality: 'best' });
  return { kind: 'bgra', ...img.getSize(), data: img.toBitmap() };
}

// Runs the pixel art on the import in progress. `reply` answers the panel with a Preview (see 'pet:image'); `result`,
// the encoded picture, comes with it when reply.ok.
async function runImport(p, opts) {
  let r;
  try {
    const first = workerToken !== p.token;
    r = await runPixel({ token: p.token, input: first ? p.input : undefined, opts });
    if (r.gone) return { reply: { ok: false, reason: 'gone' } };
    workerToken = p.token;
  } catch (err) {
    if (importing !== p) return { reply: { ok: false, reason: 'gone' } }; // its worker was stopped by a newer import or a cancel
    console.error('[bobix] pet picture:', err.message); return { reply: { ok: false, reason: 'bad' } };
  }
  if (importing !== p) return { reply: { ok: false, reason: 'gone' } }; // cancelled or replaced while it ran
  const result = { data: Buffer.from(r.data.buffer, r.data.byteOffset, r.data.length), ext: r.ext };
  return { result, reply: { ok: true, preview: `data:image/${r.ext};base64,${result.data.toString('base64')}`, opts: r.opts, opaque: r.opaque, animated: r.frames > 1, frames: r.frames, ms: r.ms } };
}
const importReply = async (p, opts) => (await runImport(p, opts)).reply;
function endImport() { importing = null; stopWorker(); }

// choose and edit: make `original` the import in progress of this project and answer with its first Preview.
async function startImport(project, original, opts) {
  const kind = petstore.kindOf(original);
  if (!kind) return { ok: false, reason: 'bad' };
  let input;
  try { input = await importInput(original, kind); } catch (err) { console.error('[bobix] pet picture:', err.message); }
  if (!input) return { ok: false, reason: 'bad' };
  endImport();
  const p = importing = { project, original, kind, input, token: Math.random().toString(36).slice(2) };
  const r = await importReply(p, opts);
  if (!r.ok && importing === p) endImport();
  return r;
}
async function choosePetImage(id) {
  const s = sessions.get(id), project = s?.project;
  if (!project) return { ok: false };
  const win = panel && !panel.isDestroyed() ? panel : undefined;
  const r = await dialog.showOpenDialog(win, { title: tr('pane.petPick'), properties: ['openFile'], filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }] });
  if (r.canceled || !r.filePaths[0]) return { ok: false };
  let original;
  try {
    if (fs.statSync(r.filePaths[0]).size > MAX_PICTURE) return { ok: false, reason: 'big' };
    original = fs.readFileSync(r.filePaths[0]);
  } catch { return { ok: false, reason: 'bad' }; } // gone or unreadable since the dialog closed
  return startImport(project, original);
}
function editPetImage(id) {
  const s = sessions.get(id), project = s?.project;
  const stored = project && petstore.readOriginal(petDir(), settings.petImages[project]);
  return stored ? startImport(project, stored.original, stored.opts) : { ok: false, reason: 'gone' };
}
async function savePetImage(id, opts) {
  const s = sessions.get(id), p = importing;
  if (!p || !s || p.project !== s.project) return { ok: false, reason: 'gone' };
  const { reply, result } = await runImport(p, opts);
  if (!reply.ok) return reply;
  const old = settings.petImages[p.project];
  let petImages;
  try {
    petImages = petstore.saveImport(petDir(), settings.petImages, p.project, { original: p.original, kind: p.kind, result, opts: reply.opts });
  } catch (err) { console.error('[bobix] pet picture:', err.message); return { ok: false, reason: 'save' }; }
  endImport();
  updateSettings({ petImages });
  saveNow(); // before the old files go: settings on disk still naming them would show a lost picture after a crash or kill
  petstore.release(petDir(), petImages, old);
  server.broadcast(snapshot());
  return { ok: true };
}
function resetPetImage(id) {
  const s = sessions.get(id), project = s?.project;
  if (!project || !settings.petImages[project]) return { ok: false };
  const { [project]: old, ...rest } = settings.petImages;
  updateSettings({ petImages: rest });
  saveNow(); // as in savePetImage
  petstore.release(petDir(), rest, old);
  server.broadcast(snapshot());
  return { ok: true };
}
const petImageActs = {
  choose: id => choosePetImage(id),
  edit: id => editPetImage(id),
  preview: (id, opts) => { const s = sessions.get(id); return importing && s && importing.project === s.project ? importReply(importing, opts) : { ok: false, reason: 'gone' }; },
  save: (id, opts) => savePetImage(id, opts),
  cancel: () => { endImport(); return { ok: true }; },
  reset: id => resetPetImage(id),
};

// ---------- answering permission requests ----------
// A permission request waits up to settings.approveWait seconds (or until answered) for Allow / Deny from a pet, the
// panel or the notification; then the hook gets no answer and Claude asks in the terminal as usual. It is never held
// while the person can't see it (S.holdSecs). Answers come only over
// IPC from our own pages (and the notification), never over HTTP.
const pending = new Map(); // answer id -> { suggestions, done }
const DECISIONS = {
  allow: () => ({ behavior: 'allow' }),
  always: p => ({ behavior: 'allow', updatedPermissions: p.suggestions }), // Claude's own "don't ask again" rules
  deny: () => ({ behavior: 'deny', message: 'Denied from Bobix.' }),
};
function waitForAnswer(s, e, signal) {
  const secs = S.holdSecs(settings, s);
  if (!secs || signal.aborted) return null;
  if (s.answer) pending.get(s.answer.id)?.done(null); // one open request per session
  const id = require('crypto').randomUUID();
  const suggestions = Array.isArray(e.permission_suggestions) ? e.permission_suggestions : [];
  const answer = new Promise(resolve => {
    const done = decision => {
      if (!pending.delete(id)) return;
      clearTimeout(timer);
      if (s.answer?.id === id) s.answer = null;
      if (decision) { s.ask = ''; s.askTool = ''; s.state = 'working'; s.since = Date.now(); }
      server.broadcast(snapshot());
      resolve(decision && { hookSpecificOutput: { hookEventName: 'PermissionRequest', decision } });
    };
    const timer = secs === Infinity ? null : setTimeout(() => done(null), secs * 1000);
    signal.addEventListener('abort', () => done(null), { once: true }); // Claude moved on (Esc, session closed)
    pending.set(id, { suggestions, done });
    s.answer = { id, until: secs === Infinity ? null : Date.now() + secs * 1000, always: suggestions.length > 0 }; // until: null = no countdown
  });
  return { secs: secs === Infinity ? 'forever' : secs, answer };
}
// A held request whose pet is now hidden (or the pets paused / off) goes back to Claude, so it asks in its own UI.
function releaseUnseen() { for (const s of sessions.values()) if (s.answer && !S.holdSecs(settings, s)) pending.get(s.answer.id)?.done(null); }
function answer(id, choice) {
  const p = typeof id === 'string' && pending.get(id);
  if (!p || !Object.hasOwn(DECISIONS, choice)) return false;
  p.done(DECISIONS[choice](p));
  return true;
}

function notify({ kind, id }) {
  const s = sessions.get(id);
  if (!s || settings.paused || !Notification.isSupported()) return;
  if ((kind === 'waiting' && !settings.notifyWaiting) || (kind === 'done' && !settings.notifyDone)) return;
  const aid = kind === 'waiting' && process.platform === 'darwin' ? s.answer?.id : null; // action buttons: macOS only
  const n = new Notification({
    title: tr(`notify.${kind}`, { name: s.title || s.name }),
    body: kind === 'waiting' ? s.ask : (s.summary || s.quest),
    silent: !settings.sound,
    ...(aid && { actions: [{ type: 'button', text: tr('ask.allow') }, { type: 'button', text: tr('ask.deny') }] }),
  });
  shown.add(n);
  if (aid) n.on('action', (_e, i) => { shown.delete(n); answer(aid, i === 0 ? 'allow' : 'deny'); });
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
        // A CLI in the Claude app's own terminal: the app's link brings its window forward, also on Wayland (no window
        // is raised by Bobix itself). Its tab is not chosen: there is no session id to send for it.
        if (app === 'Claude') { await shell.openExternal('claude://'); return { ok: true }; }
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
  if (want && !overlay.isVisible()) {
    overlay.showInactive();
    // Linux drops "always on top" when a window is hidden and shown again: set it every time.
    overlay.setAlwaysOnTop(true, 'floating');
    overlay.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  }
  else if (!want && overlay.isVisible()) { overlay.hide(); interactive = false; overlay.setIgnoreMouseEvents(true); }
}

// Only a strip along the chosen edge, tall enough for a pet, its card and a few bubble lanes:
// the compositor redraws this window while pets walk, so every pixel it does not cover is saved work.
function overlayBounds() {
  const wa = targetDisplay().workArea, h = Math.min(wa.height, Math.round(settings.size * 1.5) + 320);
  return { x: wa.x, width: wa.width, height: h, y: settings.edge === 'top' ? wa.y : wa.y + wa.height - h };
}
const placeOverlay = () => { if (overlay && !overlay.isDestroyed()) { overlay.setBounds(overlayBounds()); shapeOverlay(); } };

// SHAPED: the window is only the pets' rects, padded because the shape also clips drawing and rects arrive
// ~10 times a second. Shape first, then take the mouse: without a shape the window would block the whole strip.
// No rects means no pet is out and the overlay hides (showOverlay), so it never gets an empty shape, which means no shape.
const PAD = 16;
function shapeOverlay() {
  if (!SHAPED || !overlay || overlay.isDestroyed()) return;
  const { width, height } = overlay.getBounds();
  const shape = rects.map(([x, y, w, h]) => {
    const l = Math.max(0, Math.floor(x - PAD)), t = Math.max(0, Math.floor(y - PAD));
    return { x: l, y: t, width: Math.min(width, Math.ceil(x + w + PAD)) - l, height: Math.min(height, Math.ceil(y + h + PAD)) - t };
  }).filter(r => r.width > 0 && r.height > 0);
  if (!shape.length) return;
  overlay.setShape(shape);
  if (!interactive) { interactive = true; overlay.setIgnoreMouseEvents(false); }
}

// The overlay ignores the mouse except over a pet: poll the cursor, compare with the pets' rects from the page.
// SHAPED: the shape does that, the poll only feeds the cursor to the page (not on XWayland, see SHAPED).
function trackMouse() {
  if (!overlay || overlay.isDestroyed() || !overlay.isVisible()) return;
  const p = screen.getCursorScreenPoint(), b = overlay.getBounds();
  const x = p.x - b.x, y = p.y - b.y;
  const over = rects.some(r => x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]);
  if (!SHAPED && over !== interactive) { interactive = over; overlay.setIgnoreMouseEvents(!over); }
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

// ---------- new version ----------
// Once at start and then daily: ask GitHub for the latest release (see core/update.js). A newer one shows a
// notification once per version and a tray item that opens its release page.
const REPO = 'baonguyen-agilityio/bobix';
let update = null, updateTold = null;
async function checkForUpdate() {
  if (!settings.checkUpdates) return;
  const u = await checkLatest({ repo: REPO, current: app.getVersion() });
  if (!u || u.version === update?.version) return;
  update = u;
  buildTray();
  if (updateTold === u.version || !Notification.isSupported()) return;
  updateTold = u.version;
  const n = new Notification({ title: tr('update.title', { v: u.version }), body: tr('update.body', { v: app.getVersion() }) });
  shown.add(n);
  n.on('click', () => { shown.delete(n); shell.openExternal(u.url); });
  n.on('close', () => shown.delete(n));
  n.show();
}

// ---------- tray ----------
function buildTray() {
  // 32 px drawn for a 16 pt slot (2x), straight from pixels: no image file to ship.
  tray ??= new Tray(nativeImage.createFromBitmap(bitmap(32, 0, 'bgra'), { width: 32, height: 32, scaleFactor: 2 }));
  tray.setToolTip('Bobix');
  tray.setContextMenu(Menu.buildFromTemplate([
    { label: `Bobix ${app.getVersion()}`, enabled: false },
    { type: 'separator' },
    { label: tr('tray.panel'), click: () => showPanel() },
    { label: tr(settings.petsVisible ? 'tray.hide' : 'tray.show'), click: () => updateSettings({ petsVisible: !settings.petsVisible }) },
    { label: tr('tray.showAll'), click: () => { setPet(sessions, '*', 'shown'); server.broadcast(snapshot()); if (!settings.petsVisible) updateSettings({ petsVisible: true }); } },
    { label: tr('tray.pause'), type: 'checkbox', checked: settings.paused, click: m => updateSettings({ paused: m.checked }) },
    { label: tr('tray.feedback'), click: () => showPanel({ tab: 'feedback' }) },
    ...(news.newsFor(settings, app.getVersion(), I18N.NEWS) ? [{ label: tr('tray.news', { v: I18N.NEWS }), click: () => showPanel({ tab: 'sessions' }) }] : []),
    ...(update ? [{ type: 'separator' }, { label: tr('tray.update', { v: update.version }), click: () => shell.openExternal(update.url) }] : []),
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
  if (['petsVisible', 'paused', 'language', 'seenVersion'].some(k => settings[k] !== prev[k])) buildTray();
  releaseUnseen();
  if (settings.gitlab !== prev.gitlab || settings.gitlabUrl !== prev.gitlabUrl) refreshIssues();
  if (settings.checkUpdates && !prev.checkUpdates) checkForUpdate();
  if (!settings.checkUpdates && update) { update = null; buildTray(); }
  broadcastSettings();
  return settings;
}

// ---------- hooks setup ----------
// GUI apps don't get the shell's PATH (nvm, Homebrew…), so ask a login shell where node (or glab) is.
function findBin(name) {
  const exe = new RegExp(`${name}(\\.exe)?$`, 'i');
  const tryCmd = (cmd, args) => new Promise(res => execFile(cmd, args, { timeout: 5000, windowsHide: true }, (err, out) => {
    // `where` lists matches in PATH order (first wins); an interactive shell may print noise before the answer (last wins).
    const lines = String(out || '').split(/\r?\n/).map(l => l.trim()).filter(Boolean);
    const line = (process.platform === 'win32' ? lines : lines.reverse()).find(l => exe.test(l) && fs.existsSync(l));
    res(err && !line ? null : line || null);
  }));
  if (process.platform === 'win32') return tryCmd('where', [name]);
  const sh = process.env.SHELL || '/bin/bash';
  return tryCmd(sh, ['-lc', `command -v ${name}`]).then(p => p || tryCmd(sh, ['-lic', `command -v ${name}`]));
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
    settings, version: app.getVersion(), news: news.newsFor(settings, app.getVersion(), I18N.NEWS), platform: process.platform, nodePath, lastEventAt, hooks: hooks.status(),
    screen: (({ width, height }) => ({ width, height }))(targetDisplay().workArea),
    displays: screen.getAllDisplays().map((d, i) => ({ id: d.id, label: `${d.label || `#${i + 1}`} (${d.size.width}×${d.size.height})` })),
  }));
  handle('settings', patch => updateSettings(patch));
  // Back to defaults, except the language you read, what changes the system (port → hooks + restart, start at login) and
  // the projects' pet pictures (chosen files, not settings: wiping them would turn pets default at their next event).
  handle('settings:reset', () => updateSettings({ ...S.DEFAULTS, language: settings.language, port: settings.port, autostart: settings.autostart, petImages: settings.petImages, guideSeen: settings.guideSeen, seenVersion: settings.seenVersion }));
  handle('reveal-settings', () => shell.showItemInFolder(hooks.status().file));
  handle('diagnostics', async () => { const text = JSON.stringify(await diagnostics(), null, 2); clipboard.writeText(text); return text; });
  handle('feedback:info', () => feedbackInfo());
  handle('feedback:send', v => sendFeedback(v));
  handle('hooks:install', () => installHooks());
  handle('hooks:uninstall', () => { try { return hooks.uninstall(); } catch (err) { return { error: err.message }; } });
  handle('hooks:test', () => testEvent());
  handle('open-session', id => openSession(id, true, false));
  ipcMain.on('pet-click', (e, id) => fromUs(e) && openSession(id));
  handle('answer', (id, choice) => answer(id, choice));
  handle('pet:image', (id, act, opts) => (typeof id !== 'string' || !Object.hasOwn(petImageActs, act) ? null : petImageActs[act](id, opts && typeof opts === 'object' ? opts : undefined)));
  handle('work:refresh', () => refreshIssues());
  handle('work:open', url => openIssue(url));
  handle('work:start', (path, iid) => (typeof path === 'string' && Number.isInteger(iid) ? startIssue(path, iid) : null));
  handle('pet:set', (id, mode) => { if (typeof id === 'string' && setPet(sessions, id, mode)) { releaseUnseen(); server.broadcast(snapshot()); } });
  ipcMain.on('rects', (e, r) => { if (fromUs(e) && Array.isArray(r)) { rects = r.filter(x => Array.isArray(x) && x.length === 4 && x.every(Number.isFinite)); shapeOverlay(); showOverlay(); } });
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
    readyToShow: overlayReady, visible: overlay.isVisible(), onTop: overlay.isAlwaysOnTop(), minimized: overlay.isMinimized(), bounds: overlay.getBounds(),
    workArea: targetDisplay().workArea, sessions: sessions.size, loading: wc.isLoading(), loadedUrl: wc.getURL().replace(/t=[^&]+/, 't=…'),
  });
  log('window ' + JSON.stringify(report));
  for (const [key, js] of [['pets', 'document.querySelectorAll(".pet").length'], ['petLabels', '[...document.querySelectorAll(".pet .tag")].map(t => t.textContent)'],
    ['petRects', '[...document.querySelectorAll(".pet")].map(p => { const r = p.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round); })']]) {
    try { report[key] = await within(wc.executeJavaScript(js), 5000, key); } catch (err) { report[key] = `error: ${err.message}`; }
  }
  // The pet picture worker must start from where the app is installed (inside app.asar when packaged).
  try {
    const r = await within(runPixel({ token: 'smoke', input: { kind: 'bgra', width: 8, height: 8, data: Buffer.alloc(8 * 8 * 4, 255) } }), 10000, 'pixel worker');
    report.pixel = r.ext === 'png' && r.data.length ? 'ok' : 'unexpected result';
  } catch (err) { report.pixel = `error: ${err.message}`; }
  stopWorker();
  write(); log('page ' + JSON.stringify({ pets: report.pets, petRects: report.petRects }));
  try { fs.writeFileSync(path.join(dir, 'overlay.png'), (await within(wc.capturePage(), 8000, 'capturePage')).toPNG()); } catch (err) { report.overlayShot = err.message; }
  try {
    const [src] = await within(desktopCapturer.getSources({ types: ['screen'], thumbnailSize: targetDisplay().size }), 8000, 'desktopCapturer');
    if (src) fs.writeFileSync(path.join(dir, 'screen.png'), src.thumbnail.toPNG());
  } catch (err) { report.screenShot = err.message; }
  write(); log('done ' + JSON.stringify(report));
  fs.writeFileSync(path.join(dir, 'done'), ''); // CI waits for this, then stops the app itself
  app.exit(report.visible && report.onTop && report.pets >= 2 && !report.pageErrors.length && report.pixel === 'ok' ? 0 : 1);
}

// The Feedback button: what a bug report will say, and opening GitHub's form.
async function feedbackInfo() {
  return feedback.summarize(await diagnostics(), [...pageErrors, ...mainErrors], os.homedir());
}
async function sendFeedback({ kind, title, description } = {}) {
  if (typeof title !== 'string' || !title.trim()) return { ok: false };
  const idea = kind === 'idea';
  const { url, truncated } = feedback.issueUrl({ repo: REPO, kind: idea ? 'idea' : 'bug', title, description: typeof description === 'string' ? description.slice(0, 5000) : '', info: idea ? null : await feedbackInfo() });
  await shell.openExternal(url);
  return { ok: true, truncated };
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
// Opening Bobix again shows the panel. Opening another version (an update installed while this one runs,
// or an old copy started at login) would otherwise go unnoticed: say which one runs and how to switch.
app.on('second-instance', (_e, _argv, _cwd, other) => {
  if (!settings) return;
  showPanel();
  if (other?.version && other.version !== ME.version && Notification.isSupported()) {
    new Notification({ title: tr('other.title', { v: other.version }), body: tr('other.body', { v: ME.version }) }).show();
  }
});
app.on('window-all-closed', () => {}); // keep running in the tray
app.on('activate', () => settings && showPanel());

app.whenReady().then(async () => {
  if (process.platform === 'darwin') app.dock?.hide();
  migrateFromOldName();
  settings = S.load(settingsFile());
  // Start at login: point at this copy, not one from an old path. Run from a clone, a missing login entry of its own
  // doesn't mean "off": the installed app shares this settings file and owns the setting, so leave it as stored.
  try { const on = autostart.get(app); if (app.isPackaged || on) settings.autostart = on; if (on) autostart.set(app, true); } catch {}
  { const f = news.firstStart(settings, app.getVersion(), hooks.status().events.length > 0); if (f) { settings = { ...settings, ...f }; saveNow(); } }
  token = hooks.ensureToken();
  server = startServer({ port: settings.port, token, onEvent, snapshot, petDir: petDir() });
  try { await server.ready; } catch (err) {
    const { dialog } = require('electron');
    dialog.showErrorBox('Bobix', `Port ${settings.port} is busy (${err.code}). Is Bobix already running?\nChange "port" in ${settingsFile()}`);
    return app.exit(1);
  }
  ipc();
  nodePath = await findBin('node');
  try {
    const st = hooks.status();
    if (st.outdated || (st.mode === 'node' && st.stale && nodePath)) installHooks(); // old name, or recorded node removed (nvm/Homebrew)
    else if (st.mode === 'node' && fs.existsSync(hooks.hookTarget())) fs.copyFileSync(path.join(__dirname, 'hook', 'hook.js'), hooks.hookTarget()); // pick up app updates
  } catch (err) { console.error('[bobix] hook refresh:', err.message); }
  createOverlay();
  buildTray();
  if (!XWAYLAND) timers.push(setInterval(trackMouse, 33)); // XWayland: the page's own mouse events only
  timers.push(setInterval(() => {
    const now = Date.now();
    let changed = false;
    for (const id of gone(sessions, isAlive)) { sessions.delete(id); changed = true; }
    for (const [id, s] of sessions) {
      if (now - s.updated > STALE_MS) { sessions.delete(id); changed = true; continue; }
      if (s.claudePid && s.pidChecked !== s.claudePid) checkPid(s);
      // Esc / a denied permission ends the turn without any hook: notice it in the transcript.
      if ((s.state === 'working' || s.state === 'waiting') && refresh(s, false, now)) changed = true;
      if (findParent(s, now)) changed = true;
    }
    if (changed) server.broadcast(snapshot());
  }, 5e3));
  screen.on('display-metrics-changed', placeOverlay);
  screen.on('display-added', placeOverlay);
  screen.on('display-removed', placeOverlay);
  timers.push(setInterval(() => { for (const s of sessions.values()) refreshGit(s); }, 30e3));
  if (process.env.BOBIX_SMOKE) return smoke(process.env.BOBIX_SMOKE);
  refreshIssues();
  timers.push(setInterval(refreshIssues, 5 * 60e3)); // does nothing while GitLab is off
  setTimeout(checkForUpdate, 15e3);
  timers.push(setInterval(checkForUpdate, 24 * 3600e3));
  if (!hooks.status().events.length) showPanel({ tab: 'setup' }); // first run: help connect Claude Code
});

app.on('before-quit', () => { timers.forEach(clearInterval); if (settings) saveNow(); server?.close(); });
