// No layout shift: the control panel (src/ui/panel.html) and the pets' hover card in a real headless Chrome, with a
// stand-in for the app (its commands answer after a short delay, as the real ones do; the event stream is pushed by
// hand). Wide and narrow, English and Vietnamese, light and dark. What may not move: the window's frame (search, scope
// tabs, the list and detail boxes, the action bar) on any tab switch, a count appearing, a picker or typing; rows and
// fields once shown, while a scope's data arrives; a button whose label says it is busy; the command bar's footer while
// typing; a toast when an older one goes; a dialog's heading when its form changes; the Start box's prompt while the
// skill list is searched; the card's title while its session works on. Scrollbars come with classic tracks here (no
// --hide-scrollbars), so their tracks may not come and go either. Needs Chrome (CHROME_PATH, or the usual install
// place); without it the test is skipped. CPU_THROTTLE=<n> runs the panel n times slower (a slow runner).
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path'), os = require('os'), http = require('http');
const { spawn } = require('child_process');

const UI = path.join(__dirname, '../src/ui');
const sleep = ms => new Promise(r => setTimeout(r, ms));

function chrome() {
  const c = [process.env.CHROME_PATH,
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/google-chrome', '/usr/bin/google-chrome-stable', '/usr/bin/chromium', '/usr/bin/chromium-browser',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe', 'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe'];
  return c.find(p => p && fs.existsSync(p));
}

// ---- Chrome over the DevTools protocol (Node's own WebSocket) ----
async function browser(exe) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-layout-'));
  const proc = spawn(exe, ['--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-gpu', '--mute-audio',
    '--remote-debugging-port=0', `--user-data-dir=${dir}`, '--window-size=1180,780', '--force-device-scale-factor=1', 'about:blank'], { stdio: 'ignore' });
  let port;
  for (let k = 0; k < 200 && !port; k++) { try { port = fs.readFileSync(path.join(dir, 'DevToolsActivePort'), 'utf8').split('\n')[0]; } catch { await sleep(50); } }
  let page;
  for (let k = 0; k < 100 && !page; k++) { try { page = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(t => t.type === 'page'); } catch {} if (!page) await sleep(50); }
  const ws = new WebSocket(page.webSocketDebuggerUrl), calls = new Map(), on = new Map(); let id = 0;
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = no; });
  ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id) { const c = calls.get(d.id); calls.delete(d.id); d.error ? c[1](new Error(d.error.message)) : c[0](d.result); } else on.get(d.method)?.(d.params); };
  // A busy machine is slow, never silent: a call or an event that does not come fails the run instead of hanging it.
  const late = (what, ms = 30e3) => new Promise((_, no) => setTimeout(() => no(new Error('no answer in time: ' + what)), ms).unref());
  const send = (method, params = {}) => Promise.race([new Promise((ok, no) => { calls.set(++id, [ok, no]); ws.send(JSON.stringify({ id, method, params })); }), late(method)]);
  const once = method => Promise.race([new Promise(ok => on.set(method, p => { on.delete(method); ok(p); })), late(method)]);
  const errors = []; on.set('Runtime.exceptionThrown', p => errors.push(p.exceptionDetails.exception?.description || p.exceptionDetails.text));
  const ev = async x => { const r = await send('Runtime.evaluate', { expression: x, returnByValue: true, awaitPromise: true }); if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text); return r.result.value; };
  // Each input the test gives is noted with its time (window.__acts): a shift right after one was asked for.
  const acted = () => ev('window.__acts && __acts.push(performance.now())');
  const mouse = async (x, y) => { await acted(); for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 }); };
  // Waits (up to 10 s, a busy runner is slow) for what the page should show next.
  const until = async (x, what, tries = 100) => { for (let k = 0; k < tries; k++) { const v = await ev(x); if (v) return v; await sleep(100); }
    throw new Error(`not seen in time: ${what} (page: ${await ev('window.__state ? __state() : ""').catch(() => '?')})`); };
  // A click where a person would click: the element's middle, once it is there, nothing covers it (off screen or
  // under the narrow window's sticky action bar, it is scrolled to the middle first) and it has held still for a poll
  // (on a busy runner the list above can still grow and push it down between the look and the press).
  const click = async sel => { const at = await until(`(() => { const e = document.querySelector(${JSON.stringify(sel)}), was = window.__aim; window.__aim = null;
    if (!e || !e.getClientRects().length) return null;
    const b = e.getBoundingClientRect(), x = b.x + b.width / 2, y = b.y + b.height / 2, hit = document.elementFromPoint(x, y);
    if (!hit || (hit !== e && !e.contains(hit))) { e.scrollIntoView({ block: 'center' }); return null; }
    window.__aim = [x, y]; return was && was[0] === x && was[1] === y ? [x, y] : null; })()`, sel);
    await mouse(...at); };
  // Picks a row of the list: a redraw between the press and the release (data arriving) loses the click, so it clicks
  // again until the row is the one shown.
  const pick = async sel => { for (let k = 0; ; k++) { await click(`#b-list ${sel}`);
    try { return await until(`document.querySelector(${JSON.stringify('#b-list ' + sel)})?.dataset.bsel === (cur && cur.id)`, 'the picked row ' + sel, 20); } catch (e) { if (k === 4) throw e; } } };
  const key = async (k, code, vk, modifiers = 0) => { await acted(); for (const type of ['rawKeyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key: k, code, windowsVirtualKeyCode: vk, modifiers }); };
  const type = async t => { await acted(); for (const c of t) await send('Input.insertText', { text: c }); };
  const close = async () => { try { ws.close(); } catch {} proc.kill(); await sleep(200); try { fs.rmSync(dir, { recursive: true, force: true }); } catch {} };
  return { send, once, ev, until, acted, click, pick, mouse, key, type, close, errors };
}

// Serves the pages as the app does: / is the panel, /pets the overlay, and under /ui/ only the files the app's server
// lists (ASSETS in src-tauri/src/server.rs).
const APP_ASSETS = JSON.parse(fs.readFileSync(path.join(__dirname, '../src-tauri/src/server.rs'), 'utf8').match(/const ASSETS: \[&str; \d+\] = (\[[^\]]*\])/)[1]);
function server() {
  const types = { '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html' };
  const s = http.createServer((q, r) => {
    const u = new URL(q.url, 'http://x').pathname, f = u === '/' ? path.join(UI, 'panel.html') : u === '/pets' ? path.join(UI, 'pets.html') : u.startsWith('/ui/') && APP_ASSETS.includes(u.slice(4)) ? path.join(UI, u.slice(4)) : null;
    if (!f || !fs.existsSync(f)) { r.writeHead(404); r.end(); return; }
    r.writeHead(200, { 'content-type': types[path.extname(f)] || 'application/octet-stream' }); r.end(fs.readFileSync(f));
  });
  return new Promise(ok => s.listen(0, '127.0.0.1', () => ok(s)));
}

// ---- a made-up world ----
const NOW = Date.now(), MIN = 60e3;
const ses = (id, o) => ({ id, name: path.basename(o.cwd), cwd: o.cwd, project: o.cwd, title: '', transcript: '', state: 'working', since: NOW - 5 * MIN, updated: NOW - MIN, lv: 2, xp: 0,
  quest: '', prompt: 'Make it work', tool: 'Edit', toolArg: '', askTool: '', ask: '', summary: '', report: '', hits: 0, ctx: 60, claudePid: null, hostSession: null, log: [], pet: 'auto',
  parent: null, linking: false, image: null, pixel: false, whole: false, issue: null, ...o });
const SESSIONS = [
  ses('w1', { cwd: '/home/dev/acme-shop', state: 'waiting', askTool: 'Bash', ask: 'npm test', answer: { id: 'q1', until: null, full: 'npm test' }, issue: { iid: 41, title: 'Cart totals drift', url: 'https://gitlab.example.com/acme/shop/-/issues/41' } }),
  ses('r1', { cwd: '/home/dev/acme-api', tool: 'Bash' }),
  ses('d1', { cwd: '/home/dev/notes', state: 'done', summary: 'Release notes written.' }),
];
const issue = (iid, title, project, o = {}) => ({ iid, title, url: `https://gitlab.example.com/${project}/-/issues/${iid}`, project, labels: [], due: null, sessions: [], ...o });
const WORK = { status: 'ok', at: NOW - MIN, problems: [], failed: [], hosts: ['gitlab.example.com'], away: [], closed: [],
  local: [{ path: 'acme/shop', dir: '/home/dev/acme-shop', key: 'SHOP', autoKey: 'SHOP', issues: [issue(41, 'Cart totals drift', 'acme/shop', { stage: 'doing', sessions: ['w1'] }), issue(42, 'Show the delivery date', 'acme/shop')] },
    { path: 'acme/api', dir: '/home/dev/acme-api', key: 'API', autoKey: 'API', issues: [issue(8, 'Rate-limit the export', 'acme/api')] }] };
const event = (sessions, work = WORK) => ({ sessions, work, model: {}, waiting: [], waits: [], tickets: { list: [] }, usage: {} });
const skill = (name, o = {}) => ({ key: `/home/dev/.claude/skills/${name}`, name, source: 'personal', uses: 3, lastUsed: NOW - 864e5, size: 800, unused: false, dup: null, change: { can: true }, description: `What ${name} does.`, ...o });
// What the app's commands answer, and after how long (ms): the slow ones arrive after the tab's first paint.
const APP = {
  skills_mine: [[{ name: 'review', description: 'Review a change', source: 'personal' }], 150],
  process_status: [[{ path: 'acme/shop', dir: '/home/dev/acme-shop', skills: [], repo: null, imported: null, missing: [] }], 150],
  habit_skills: [{ content: true, on: true, scan: { at: NOW - 864e5, prompts: 90, next: NOW + 4 * 864e5 }, rules: { unusedDays: 30, minUses: 3, editsPerUse: 1, fixesPerUse: 1 },
    suggestions: [{ id: 'x1', label: 'Weekly release notes', weeks: [3, 4], action: 'new', summary: 'Every Friday.', sessions: 5 }], made: [] }, 150],
  skills_audit: [{ days: 30, slashCounted: true, summary: { skills: 4, unused: 1, duplicateGroups: 1, descriptionChars: 3200, tokensPerTurn: 800, bySource: { personal: 3, plugin: 1 } },
    duplicates: [{ why: 'name', names: ['review', 'acme:review'] }], skills: [skill('review', { dup: 0 }), skill('acme:review', { dup: 0, source: 'plugin', plugin: 'acme', change: { can: false, why: 'plugin' } }), skill('notes'), skill('old', { unused: true, uses: 0, lastUsed: null })] }, 450],
  skills_rules: [{ file: '/home/dev/acme-shop/CLAUDE.md', present: false, text: '## When to split, which model\n- Plan with Opus', block: '## When to split, which model\n', undo: null }, 450],
  skills_backups: [[], 450],
  room: [{ ok: true, tailnet: { me: { login: 'dev@example.com', name: 'Dev' }, ip: '100.64.0.10', port: 4748, serving: true }, hosted: [], joined: [] }, 300],
  ticket_prep: [{ ok: true, dir: '/home/dev/acme-shop-42', branch: '42-delivery', room: 5, free: 50e9, need: 2e9, models: 'Plan with Opus.', rules: [], exists: false }, 1000],
  work_brief: [{ ok: true, body: 'Customers want to know when the order arrives.', labels: [], process: null,
    skills: ['review', 'notes', 'deploy', 'perf', 'docs', 'tests', 'i18n', 'style'].map(name => ({ name, description: `What ${name} does, in a line long enough to wrap in the list.`, source: 'personal' })) }, 200],
  feedback_info: [{ lines: ['Bobix', 'linux'], errs: [] }, 0],
};
const prelude = lang => `(() => {
  const A = ${JSON.stringify(APP)}, settings = { language: '${lang}', tourSeen: [], seenVersion: '9.9.9', role: 'dev', skillPins: {}, habitsOn: false };
  const state = { settings, version: '0.0.0', news: null, platform: 'linux', nodePath: '/usr/bin/node', lastEventAt: ${NOW}, screen: { width: 1920, height: 1080 }, displays: [],
    hooks: { events: ['Stop'], complete: true, mode: 'node', port: 4747, stale: false, file: '/home/dev/.claude/settings.json' } };
  window.__TAURI_INTERNALS__ = { invoke: (cmd, a) => {
    if (cmd === 'state') { if (window.TOUR) settings.tourSeen = TOUR.STEPS.map(s => s.key); return Promise.resolve(state); }
    if (cmd === 'settings_set') return Promise.resolve(Object.assign(settings, a.patch));
    const [v, ms] = cmd === 'room' && a.act !== 'list' ? [null, 0] : A[cmd] || [null, 0];
    return new Promise(ok => setTimeout(() => ok(v == null ? null : JSON.parse(JSON.stringify(v))), ms)); } };
  window.EventSource = class { constructor() { window.__es = this; } close() {} };
  window.__push = d => window.__es.onmessage({ data: JSON.stringify(d) });
  window.__shifts = []; window.__acts = [];
  window.__state = () => { try { return JSON.stringify({ scope, cur: cur && cur.id, q: bq, focus: document.activeElement && (document.activeElement.id || document.activeElement.tagName),
    dialogs: [...document.querySelectorAll('dialog[open]')].map(d => d.id), start: starting && { iid: starting.iid, skills: starting.skills.length, all: starting.all } }); } catch (e) { return String(e); } };
  new PerformanceObserver(l => { for (const e of l.getEntries()) window.__shifts.push({ v: e.value, input: e.hadRecentInput, t: e.startTime,
    src: (e.sources || []).map(s => (s.node?.id || s.node?.className || s.node?.nodeName || '?') + ' ' + [s.previousRect.y, s.currentRect.y].map(Math.round).join('→')).join(', ') }); })
    .observe({ type: 'layout-shift', buffered: true });
})();\n` + fs.readFileSync(path.join(__dirname, '../src-tauri/bridge.js'), 'utf8');

// Every box worth watching, by a name that survives a redraw, in page coordinates; what sits over the page (an open
// dialog, a toast) in the window's, since it stays put when the page scrolls under it.
const BOXES = `(() => { const o = {}, r = el => { const b = el.getBoundingClientRect(), fixed = el.closest('dialog[open], .toasts'); return [b.x, b.y + (fixed ? 0 : scrollY), b.width, b.height].map(v => Math.round(v * 2) / 2); };
  const add = (k, el) => { if (el && el.getClientRects().length && !(k in o)) o[k] = r(el); };
  for (const s of ['.b-search', '#bq', '#bq-x', '#scopes', '.b-split', '#b-list', '#b-bar', '#cmdk-ft', '#fb-dialog h2', '#fb-dialog .acts', '#st-text']) add(s, document.querySelector(s));
  for (const el of document.querySelectorAll('#scopes .tab')) add('tab:' + el.dataset.tab, el);
  // Narrow, the list sits above its detail in one scroll: their tops stay, their heights are the scope's content.
  if (getComputedStyle(document.querySelector('.b-split')).gridTemplateColumns.split(' ').length > 1) add('#b-detail', document.querySelector('#b-detail'));
  else for (const k of ['.b-split', '#b-list']) o[k][3] = 0;
  for (const el of document.querySelectorAll('#b-list [data-f], #b-dyn [data-f], #b-dyn .b-dh h2')) add(el.dataset.f || 'h2:' + el.textContent.slice(0, 30), el);
  for (const el of document.querySelectorAll('.toast')) add('toast:' + el.dataset.text, el);
  const sb = s => { const e = document.querySelector(s); return e.offsetWidth - e.clientWidth; };
  o.$tracks = [sb('#b-list'), sb('#b-detail'), innerWidth - document.documentElement.clientWidth];
  return o; })()`;
const FRAME = ['.b-search', '#bq', '#scopes', '.b-split', '#b-list', '#b-detail', '#b-bar', 'tab:overview', 'tab:sessions', 'tab:work', 'tab:skills', 'tab:room', 'tab:settings'];
const same = (a, b) => a && b && a.every((v, i) => Math.abs(v - b[i]) <= .5);
const moved = (a, b, keys = Object.keys(a)) => keys.filter(k => k[0] !== '$' && a[k] && b[k] && !same(a[k], b[k])).map(k => `${k} ${a[k].join(',')} → ${b[k].join(',')}`);

async function panel(b, url, { width, height, lang, dark }) {
  const bad = [], fail = m => bad.push(m);
  await b.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
  if (process.env.CPU_THROTTLE) await b.send('Emulation.setCPUThrottlingRate', { rate: +process.env.CPU_THROTTLE });
  await b.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: dark ? 'dark' : 'light' }] });
  const { identifier } = await b.send('Page.addScriptToEvaluateOnNewDocument', { source: prelude(lang) });
  try {
    const loaded = b.once('Page.loadEventFired'); await b.send('Page.navigate', { url }); await loaded;
    await b.ev(`__push(${JSON.stringify(event(SESSIONS, { status: 'loading' }))})`);
    await sleep(300);
    await b.ev(`__push(${JSON.stringify(event(SESSIONS))})`);
    await sleep(300);
    const frame = await b.ev(BOXES), wide = width > 720;
    const tab = async t => { await b.ev('scrollTo(0, 0)'); await b.click(`#scopes [data-tab="${t}"]`); }; // narrow: the tabs scroll with the page
    const frameStill = async (what, at) => { const now = await b.ev(BOXES); for (const m of moved(frame, now, FRAME)) fail(`${what}${at}: ${m}`);
      if (now.$tracks.join() !== frame.$tracks.join()) fail(`${what}${at}: scrollbar tracks ${frame.$tracks} → ${now.$tracks}`); return now; };
    // Each tab, there and back: the frame stays, and once a row or field is shown it stays where it is as the data comes in.
    for (const t of ['sessions', 'work', 'skills', 'room', 'settings', 'overview', 'skills', 'sessions', 'overview']) {
      await tab(t);
      let last = null;
      for (const ms of [0, 250, 650]) {
        await sleep(ms - (last ? last.ms : 0));
        const now = await frameStill(`tab ${t}`, ` (+${ms} ms)`);
        if (last) for (const m of moved(last.box, now).filter(m => !FRAME.some(k => m.startsWith(k + ' ')))) fail(`tab ${t}, as its data arrives (+${last.ms}→${ms} ms): ${m}`);
        last = { ms, box: now };
      }
    }
    // Each part on its own: one that cannot run is a failure, and the rest still run.
    const part = async (name, fn) => { try { await b.ev(`document.querySelectorAll('dialog[open]').forEach(d => d.close())`); await fn(); } catch (e) { fail(`${name}: ${e.message.split('\n')[0]}`); } };
    let before = [];
    // A request answered, then a new one: the count on Overview comes and goes (seen from Settings, whose list stays).
    await part('count', async () => {
      await tab('settings'); await sleep(200);
      await b.ev(`__push(${JSON.stringify(event(SESSIONS.map(s => s.id === 'w1' ? { ...s, state: 'working', answer: null, askTool: '', ask: '' } : s)))})`);
      await frameStill('count gone', '');
      await b.ev(`__push(${JSON.stringify(event(SESSIONS))})`);
      await frameStill('count back', '');
    });
    // The app answers a new ask for the issues with a newer read (work.gen); until then the page shows "Reading your issues…".
    let gen = 0;
    const answered = async () => { await b.ev(`__push(${JSON.stringify(event(SESSIONS, { ...WORK, gen: ++gen }))})`); await b.until('!reading()', 'the issues read again'); };
    // The pickers in the scope bar: Skills' project, Work's filter. Choosing another changes the list, not the bar.
    for (const [t, sel] of [['skills', '[data-skdir]'], ['work', '[data-wfilter]']]) await part(`${t} picker`, async () => {
      await tab(t); await sleep(300);
      await b.until(`!!document.querySelector('${sel}')`, `the ${t} picker`); await b.acted();
      await b.ev(`(() => { const s = document.querySelector('${sel}'); s.value = s.options[s.options.length - 1].value; s.dispatchEvent(new Event('change', { bubbles: true })); })()`);
      await answered(); await sleep(100); await frameStill(`${t} picked`, '');
      await b.acted();
      await b.ev(`(() => { const s = document.querySelector('${sel}'); s.value = s.options[0].value; s.dispatchEvent(new Event('change', { bubbles: true })); })()`);
      await answered(); await sleep(100);
    });
    // Typing in the search: its clear button shows up.
    await part('search', async () => {
      await b.click('#bq'); await b.type('ac'); await sleep(100);
      const typed = await frameStill('search typed', '');
      if (!same(typed['#bq-x'], frame['#bq-x'])) fail(`search clear button ${frame['#bq-x']} → ${typed['#bq-x']}`);
      await b.ev(`document.querySelector('#bq-x').click()`); await sleep(100);
    });
    // Work: Prepare says it is busy (reading the issue) in a button of the same size.
    await part('Prepare', async () => {
      await tab('work'); await sleep(300);
      await b.pick('[data-bsel^="n:acme/shop"]');
      const prep = await b.until(`document.querySelector('#b-dyn [data-tk-prep]:not([disabled])')?.getBoundingClientRect().toJSON()`, 'Prepare');
      await b.click('#b-dyn [data-tk-prep]');
      const busy = await b.ev(`(() => { const e = document.querySelector('#b-dyn [data-tk-prep]'); return e && { ...e.getBoundingClientRect().toJSON(), off: e.disabled }; })()`);
      if (!busy?.off) fail('Prepare: no busy state seen');
      else if (Math.abs(busy.width - prep.width) > .5 || Math.abs(busy.x - prep.x) > .5) fail(`Prepare button while busy: ${Math.round(prep.x)}+${prep.width} → ${Math.round(busy.x)}+${busy.width}`);
      await b.until(`!document.querySelector('#b-dyn [data-tk-prep]')`, 'the prepared issue'); await sleep(200);
    });
    // The command bar: its footer stays while the results change under it.
    await part('command bar', async () => {
      await b.key('k', 'KeyK', 75, 2); await b.until(`document.querySelector('#cmdk')?.open`, 'the command bar');
      const k0 = await b.ev(BOXES);
      for (const t of ['se', 'zzz']) { await b.type(t); await sleep(100); for (const m of moved(k0, await b.ev(BOXES), ['#cmdk-ft'])) fail(`command bar, typed ${t}: ${m}`); }
    });
    // Toasts: the older one (below) goes first, the newer one above stays where it is.
    await part('toasts', async () => {
      await b.ev(`toast('Removed the skill', { label: 'Undo', run() {} })`); await sleep(50);
      await b.ev(`toast('Copied the link')`); await sleep(300);
      await b.ev(`Promise.all([...document.querySelectorAll('.toast')].flatMap(e => e.getAnimations()).map(a => a.finished)).then(() => 0)`); // their rise in is on purpose
      const t0 = await b.ev(BOXES);
      await b.click('.toast[data-text="Removed the skill"] button'); await sleep(100);
      for (const m of moved(t0, await b.ev(BOXES), ['toast:Copied the link'])) fail(`toast when an older one goes: ${m}`);
    });
    // Feedback: Bug / Idea changes the form under the heading, not the heading.
    await part('feedback', async () => {
      await tab('settings'); await sleep(300);
      await b.ev(`document.querySelector('#b-list [data-bsel="feedback"]').scrollIntoView({ block: 'center' })`);
      await b.pick('[data-bsel="feedback"]');
      await b.click('#settings [data-act="feedback"]'); await sleep(300);
      const f0 = await b.ev(BOXES);
      for (const k of ['idea', 'bug']) { await b.click(`#fb-kind [data-kind="${k}"]`); await sleep(100); for (const m of moved(f0, await b.ev(BOXES), ['#fb-dialog h2', '#fb-dialog .acts'])) fail(`feedback, ${k}: ${m}`); }
    });
    // Work's Full / Quick start: switching the choice changes the box under it, not the frame or the heading.
    await part('Full / Quick', async () => {
      await tab('work'); await sleep(300);
      await b.pick('[data-bsel="n:acme/shop#42"]');
      await b.until(`!!document.querySelector('#b-dyn [data-tk-how]')`, 'the how-to-start choice'); await sleep(300);
      const h0 = await b.ev(BOXES);
      for (const v of ['quick', 'full', 'quick', 'full']) {
        await b.click(`#b-dyn [data-tk-how][value="${v}"]`); await sleep(250);
        for (const m of moved(h0, await b.ev(BOXES), [...FRAME.filter(k => !k.startsWith('#b-')), '.b-split', '#b-list', '#b-detail'])) fail(`Full / Quick, ${v}: ${m}`);
      }
    });
    // The Start box (Quick start): See all opens the list (the prompt goes down, as asked), then searching it moves nothing.
    await part('Start box', async () => {
      await tab('work'); await sleep(300);
      await b.pick('[data-bsel="n:acme/shop#42"]');
      await b.until(`!!document.querySelector('#b-dyn [data-tk-how]')`, 'the how-to-start choice');
      // A redraw between the press and the release (data arriving) loses a click: press again until it took.
      for (let k = 0; k < 5 && !(await b.ev(`!!document.querySelector('#b-dyn [data-start][data-dir]')`)); k++) { await b.click('#b-dyn [data-tk-how][value="quick"]'); await sleep(400); }
      await b.until(`!!document.querySelector('#b-dyn [data-start][data-dir]')`, 'the Quick start button');
      for (let k = 0; k < 5 && !(await b.ev(`!!document.querySelector('#st-dialog')?.open`)); k++) { await b.click('#b-dyn [data-start][data-dir]'); await sleep(600); }
      await b.until(`document.querySelector('#st-dialog')?.open && !!document.querySelector('#st-all')`, 'the Start box with its skills'); await sleep(300);
      before = await b.ev('__shifts.splice(0)');
      await b.click('#st-all'); await sleep(150);
      await b.ev('__shifts.length = 0');
      const s0 = await b.ev(BOXES);
      for (const t of ['no', 'zz']) { await b.type(t); await sleep(100); for (const m of moved(s0, await b.ev(BOXES), ['#st-text'])) fail(`Start box, searched ${t}: ${m}`); }
    });
    // The "?" tips: by mouse, keyboard and touch, each shows by its "?", inside the window, as a tooltip of it (role,
    // aria-describedby), with its own words; showing, hiding or a redraw under it moves nothing, not even on a narrow
    // window (no layout shift at all here, asked or not). The pointer leaving, Esc, a second tap or a tap elsewhere hide it;
    // ↵ and Space on a "?" never run the selected item's action.
    await part('help tips', async () => {
      const at = sel => b.until(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e || !e.getClientRects().length) return null;
        const r = e.getBoundingClientRect(), x = r.x + r.width / 2, y = r.y + r.height / 2, hit = document.elementFromPoint(x, y);
        if (hit && (hit === e || e.contains(hit))) return [x, y];
        e.scrollIntoView({ block: 'center' }); return null; })()`, sel);
      const tipOf = sel => b.ev(`(() => { const t = document.querySelector('#tip'), h = document.querySelector(${JSON.stringify(sel)}), r = t.getBoundingClientRect(), q = h.getBoundingClientRect(), cs = getComputedStyle(t);
        return { on: t.classList.contains('on') && cs.visibility === 'visible' && cs.position === 'fixed', role: t.getAttribute('role'), by: h.getAttribute('aria-describedby'), text: t.textContent, want: T('help.' + h.dataset.help),
          inside: r.left >= 8 && r.top >= 8 && r.right <= innerWidth - 8 && r.bottom <= innerHeight - 8, near: Math.min(Math.abs(r.top - q.bottom), Math.abs(q.top - r.bottom)) <= 8,
          rect: [r.left, r.top, r.width, r.height].map(Math.round).join() }; })()`);
      const off = () => b.ev(`!document.querySelector('#tip').classList.contains('on') && !document.querySelector('[aria-describedby="tip"]')`);
      const shows = async (how, sel) => {
        await b.until(`document.querySelector('#tip').classList.contains('on') || null`, `${how}: the tip of ${sel}`);
        const s = await tipOf(sel);
        if (!s.on || s.role !== 'tooltip' || s.by !== 'tip' || !s.text || s.text !== s.want) fail(`help ${how} ${sel}: ${JSON.stringify(s)}`);
        if (!s.inside) fail(`help ${how} ${sel}: the tip is out of the window: ${s.rect}`);
        if (!s.near) fail(`help ${how} ${sel}: the tip is not by its "?": ${s.rect}`);
      };
      const hides = async how => { for (let k = 0; k < 10 && !(await off()); k++) await sleep(100); if (!(await off())) fail(`help: ${how} does not hide the tip`); };
      const still = async (what, f0) => { for (const m of moved(f0, await b.ev(BOXES))) fail(`help ${what}: ${m}`); };
      const noShift = async from => { for (const s of await b.ev(`__shifts.slice(${from})`)) fail(`help: layout shift ${s.v.toFixed(4)}: ${s.src}`); };
      // a key as a keyboard sends it, with its text: ↵ and Space press a focused button
      const press = async (k, code, vk, text) => { await b.acted(); await b.send('Input.dispatchKeyEvent', { type: 'keyDown', key: k, code, windowsVirtualKeyCode: vk, text }); await b.send('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code, windowsVirtualKeyCode: vk }); };
      const hover = async sel => { const [x, y] = await at(sel); await sleep(150); const f0 = await b.ev(BOXES);
        await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y }); await shows('hover', sel); await still('hover ' + sel, f0); return f0; };
      await tab('work'); await sleep(400);
      const n0 = await b.ev('__shifts.length');
      const g = '#b-list [data-help="grp.fstart"]';
      // mouse: shows while pointed at, goes when the pointer leaves
      const f0 = await hover(g);
      await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 2, y: 2 }); await hides('the pointer leaving');
      await still('hidden', f0);
      // keyboard: Tab to it from the row before; Esc hides it and keeps focus and the search; ↵ shows it, Space hides it, the selection stays
      const sel0 = await b.ev(`document.querySelector('#b-list [aria-selected="true"]')?.dataset.bsel`);
      await b.ev(`(() => { const bs = [...document.querySelectorAll('#b-list button')]; bs[bs.indexOf(document.querySelector(${JSON.stringify(g)})) - 1].focus(); })()`);
      await b.key('Tab', 'Tab', 9);
      if (!(await b.ev(`document.activeElement === document.querySelector(${JSON.stringify(g)})`))) fail('help: Tab does not reach the "?"');
      await shows('focus', g);
      await b.key('Escape', 'Escape', 27); await hides('Esc');
      if (!(await b.ev(`document.activeElement?.dataset.help === 'grp.fstart' && document.querySelector('#bq').value === ''`))) fail('help: Esc moved the focus or touched the search');
      await press('Enter', 'Enter', 13, '\r'); await shows('↵', g);
      // a redraw under it (the list's refresh time changes): the tip stays by the new "?", nothing moves
      const f1 = await b.ev(BOXES);
      await b.ev(`window.__help = document.querySelector(${JSON.stringify(g)})`);
      await b.ev(`__push(${JSON.stringify(event(SESSIONS, { ...WORK, at: NOW - 3 * MIN }))})`); await sleep(300);
      if (await b.ev(`__help === document.querySelector(${JSON.stringify(g)})`)) fail('help: the redraw did not replace the "?"');
      await shows('after a redraw', g); await still('redraw', f1);
      await press(' ', 'Space', 32, ' '); await hides('Space on a shown tip');
      if (await b.ev(`document.activeElement !== document.querySelector(${JSON.stringify(g)}) || document.querySelector('#bq').value !== ''`)) fail('help: Space left the "?" or typed in the search');
      if (await b.ev(`document.querySelector('#b-list [aria-selected="true"]')?.dataset.bsel`) !== sel0 || await b.ev(`!!document.querySelector('dialog[open]')`)) fail('help: ↵ or Space on the "?" ran the selected item');
      await b.ev('document.activeElement.blur()');
      // touch: a tap shows it, a second tap hides it, a tap elsewhere hides it too
      await b.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 1 });
      try {
        const tap = async sel => { const [x, y] = await at(sel); await sleep(150);
          await b.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] }); await b.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] }); await sleep(250); };
        const f2 = await b.ev(BOXES);
        await tap(g); await shows('tap', g); await still('tap', f2);
        await tap(g); await hides('a second tap');
        await tap(g); await shows('tap again', g);
        await tap('#b-list [data-grp="fstart"] .b-grp > span'); await hides('a tap elsewhere');
      } finally { await b.send('Emulation.setTouchEmulationEnabled', { enabled: false }); }
      await noShift(n0);
      // a settings row's "?" (the tab switch and the pick before it may move things on a narrow window, as asked)
      await tab('settings'); await sleep(300);
      await b.click('#b-list [data-bsel="set:stuck"]'); await sleep(800);
      const n1 = await b.ev('__shifts.length');
      await hover('#settings [data-help="row.stuckIdle"]');
      await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 2, y: 2 }); await hides('the pointer leaving a row\'s "?"');
      await noShift(n1);
    });
    // The tour: every step points at an element that is on screen (or, when it has an alt, at that one), with the bubble up.
    await part('tour targets', async () => {
      const keys = await b.ev('TOUR.STEPS.map(s => s.key)'), n0 = await b.ev('__shifts.length');
      for (const [i, key] of keys.entries()) {
        const r = await b.ev(`(async () => { tourStart(false); tourAt = ${i}; await tourShow(); const s = tourSteps[tourAt], t = tourTarget(s.at) || (s.alt && tourTarget(s.alt.at)), r = t && t.getBoundingClientRect();
          return { key: s.key, ok: !!r && r.width > 0 && r.height > 0, shown: document.querySelector('#tour-pop').getBoundingClientRect().height > 0 }; })()`);
        if (r.key !== key || !r.ok) fail(`tour step ${key}: the element it points at is not on screen`);
        if (!r.shown) fail(`tour step ${key}: no bubble`);
      }
      await b.ev('tourEnd()'); await sleep(400);
      await b.ev(`__shifts.length = ${n0}`); // the bubble and its highlight are an overlay that moves from step to step on purpose
    });
    await b.ev(`document.querySelectorAll('dialog[open]').forEach(d => d.close())`);
    // Nothing the layout-shift API sees either (narrow: a tab switch moves the detail below the list, as asked).
    const acts = await b.ev('__acts'), asked = s => s.input || acts.some(a => s.t >= a && s.t - a < 3000);
    for (const s of [...before, ...await b.ev('__shifts')]) if (!asked(s) || wide) fail(`layout shift ${s.v.toFixed(4)}${s.input ? ' (after input)' : ''}: ${s.src}`);
    bad.push(...b.errors.splice(0).map(e => `page error: ${e}`));
  } finally { await b.send('Page.removeScriptToEvaluateOnNewDocument', { identifier }); }
  return bad.map(m => `${width}×${height} ${lang}${dark ? ' dark' : ''}: ${m}`);
}

// The pets' card: its title stays put while the session works on (the card grows upward from the pet).
async function card(b, url) {
  const bad = [];
  await b.send('Emulation.setDeviceMetricsOverride', { width: 1920, height: 404, deviceScaleFactor: 1, mobile: false });
  const { identifier } = await b.send('Page.addScriptToEvaluateOnNewDocument', { source: `(() => { const noop = () => {};
    window.bobix = { onMouse: noop, onSettings: f => { window.__apply = f; }, getState: () => new Promise(noop), petClick: noop, setPet: noop, setRects: noop, answer: noop };
    window.EventSource = class { constructor() { window.__es = this; } close() {} };
    window.__push = list => window.__es.onmessage({ data: JSON.stringify({ sessions: list }) }); })();` });
  try {
    const loaded = b.once('Page.loadEventFired'); await b.send('Page.navigate', { url }); await loaded;
    for (const lang of ['en', 'vi']) {
      await b.ev(`__apply(${JSON.stringify({ language: lang, size: 64, opacity: 1, petsVisible: true, showIdle: true, edge: 'bottom', zoneStart: 0, zoneEnd: 100, walkDir: 'ltr', label: 'title', clickAction: 'panel', usageCost: true, petMode: 'edge', fadeNearCursor: false })})`);
      const usage = n => ({ times: n, warnings: [], turns: 14, mainTurns: 12, activeMs: n * 30 * MIN, tokens: { input: 1, cacheWrite: 1, cacheRead: 1, output: 1 }, reread: .8, models: [{ model: 'claude-opus-5-5', turns: 12 }], cost: { usd: n * 10, date: '2026-11-01' } });
      const push = o => b.ev(`__push(${JSON.stringify([ses('c1', { cwd: '/home/dev/acme-shop', title: 'Cart totals', quest: 'Fix the flaky cart test', tool: 'Read', usage: usage(1.6), ...o })])})`);
      await push({}); await sleep(1500);
      const at = await b.ev(`(() => { const r = document.querySelector('.pet .spr').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
      const title = async () => { await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] }); await sleep(400); // after the card's pop-in animation, which moves it on purpose
        return b.ev(`(async () => { const c = document.querySelector('.pet .card'); if (!c) return null; await Promise.all(c.getAnimations().map(a => a.finished)); return c.querySelector('h2').getBoundingClientRect().top; })()`); };
      const y0 = await title();
      if (y0 == null) { bad.push(`${lang}: no card on hover`); continue; }
      for (const [what, o] of [['an MCP tool', { tool: 'mcp__github__create_pull_request_review_comment' }], ['a long usage line', { usage: usage(12.5) }], ['Grep', { tool: 'Grep' }]]) {
        await push(o); const y = await title();
        if (Math.abs(y - y0) > .5) bad.push(`${lang}: card title moved with ${what}: ${y0} → ${y}`);
      }
      await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 1, y: 1 }); await sleep(400);
    }
    bad.push(...b.errors.splice(0).map(e => `page error: ${e}`));
  } finally { await b.send('Page.removeScriptToEvaluateOnNewDocument', { identifier }); }
  return bad.map(m => `pets card: ${m}`);
}

const exe = chrome();
test('nothing moves in the panel or the pets card unless asked (tabs, data arriving, typing, toasts, dialogs; wide and narrow, en and vi)', { skip: !exe && 'no Chrome', timeout: 300e3 }, async () => {
  const srv = await server(), base = `http://127.0.0.1:${srv.address().port}`;
  // One browser each, side by side: the checks wait on the stand-in's delays, not on the machine.
  const runs = [{ width: 1180, height: 780, lang: 'vi' }, { width: 1180, height: 780, lang: 'en', dark: true }, { width: 420, height: 780, lang: 'vi' }, { width: 420, height: 780, lang: 'en', dark: true }, null];
  try {
    const bad = await Promise.all(runs.map(async c => {
      const b = await browser(exe);
      try { await b.send('Page.enable'); await b.send('Runtime.enable'); return c ? await panel(b, base + '/?t=x', c) : await card(b, base + '/pets'); } finally { await b.close(); }
    }));
    assert.deepEqual(bad.flat(), []);
  } finally { srv.close(); }
});
