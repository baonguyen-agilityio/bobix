// The pets overlay (src/ui/pets.html) in a real headless Chrome, its clock stepped by hand: no pixel any effect draws may
// land on another pet, on any name tag, balloon, bubble or card, or on another pet's effect (a prop, an aura, a bar), and
// no two pets in sight ever stand on each other (also while one is dragged and let go, after which the line must be
// evenly spaced again). Checked on the composited page: for each pet with an effect, a
// screenshot of that effect alone against a screenshot of everything else on the page, pixel by pixel. Also "show when
// needed" (onDemand below). Needs Chrome (CHROME_PATH, or the usual install place); without it the test is skipped.
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path'), os = require('os'), http = require('http'), zlib = require('zlib');
const { spawn } = require('child_process');

const UI = path.join(__dirname, '../src/ui');
const W = 1920, H = 404; // the pets window

function chrome() {
  const c = [process.env.CHROME_PATH,
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/google-chrome', '/usr/bin/google-chrome-stable', '/usr/bin/chromium', '/usr/bin/chromium-browser',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe', 'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe'];
  return c.find(p => p && fs.existsSync(p));
}

// ---- a PNG reader (8-bit RGB / RGBA, not interlaced: what Chrome writes) ----
function png(buf) {
  let i = 8, w = 0, h = 0, type = 0; const idat = [];
  while (i < buf.length) {
    const len = buf.readUInt32BE(i), kind = buf.toString('ascii', i + 4, i + 8), d = buf.subarray(i + 8, i + 8 + len);
    if (kind === 'IHDR') { w = d.readUInt32BE(0); h = d.readUInt32BE(4); type = d[9]; assert.equal(d[8], 8); assert.equal(d[12], 0); }
    if (kind === 'IDAT') idat.push(d);
    i += 12 + len;
  }
  const bpp = type === 6 ? 4 : 3, raw = zlib.inflateSync(Buffer.concat(idat)), stride = w * bpp, px = Buffer.alloc(h * stride), out = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)], src = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)), row = px.subarray(y * stride, (y + 1) * stride), up = y ? px.subarray((y - 1) * stride, y * stride) : null;
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? row[x - bpp] : 0, b = up ? up[x] : 0, c = up && x >= bpp ? up[x - bpp] : 0;
      const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
      row[x] = (src[x] + [0, a, b, (a + b) >> 1, pa <= pb && pa <= pc ? a : pb <= pc ? b : c][f]) & 255;
    }
  }
  for (let k = 0; k < w * h; k++) { for (let j = 0; j < 3; j++) out[k * 4 + j] = px[k * bpp + j]; out[k * 4 + 3] = bpp === 4 ? px[k * bpp + 3] : 255; }
  return { w, h, data: out };
}

// A PNG writer, for OVERLAY_DUMP (a folder): each failing check saved, the effect over the rest, its overlap in magenta;
// with it the two screenshots as taken, the whole window, and every pet's boxes (state.json).
function pngOut(w, h, rgba) {
  const crc = b => { let c = ~0; for (const x of b) { c ^= x; for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1; } return ~c >>> 0; };
  const chunk = (t, d) => { const l = Buffer.alloc(4), c = Buffer.alloc(4), td = Buffer.concat([Buffer.from(t), d]); l.writeUInt32BE(d.length); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); };
  const raw = Buffer.alloc(h * (w * 4 + 1)); for (let y = 0; y < h; y++) Buffer.from(rgba.buffer, y * w * 4, w * 4).copy(raw, y * (w * 4 + 1) + 1);
  const hd = Buffer.alloc(13); hd.writeUInt32BE(w); hd.writeUInt32BE(h, 4); hd[8] = 8; hd[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', hd), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

// ---- Chrome over the DevTools protocol (Node's own WebSocket) ----
async function browser(exe) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-overlay-'));
  const proc = spawn(exe, ['--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-gpu', '--hide-scrollbars', '--mute-audio',
    // Frames on demand, not at 60 Hz: each screenshot is drawn right away instead of waiting for the next vsync (about 3x
    // faster, and the runner's macOS Chrome is slow enough that this is what keeps the test in its time).
    '--disable-frame-rate-limit', '--disable-gpu-vsync',
    '--remote-debugging-port=0', `--user-data-dir=${dir}`, `--window-size=${W},${H}`, '--force-device-scale-factor=1', 'about:blank'], { stdio: 'ignore' });
  let port;
  for (let k = 0; k < 200 && !port; k++) { try { port = fs.readFileSync(path.join(dir, 'DevToolsActivePort'), 'utf8').split('\n')[0]; } catch { await sleep(50); } }
  const page = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(t => t.type === 'page');
  const ws = new WebSocket(page.webSocketDebuggerUrl), calls = new Map(), on = new Map(); let id = 0;
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = no; });
  ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id) { const c = calls.get(d.id); calls.delete(d.id); d.error ? c[1](new Error(d.error.message)) : c[0](d.result); } else on.get(d.method)?.(d.params); };
  const send = (method, params = {}) => new Promise((ok, no) => { calls.set(++id, [ok, no]); ws.send(JSON.stringify({ id, method, params })); });
  const once = method => new Promise(ok => on.set(method, p => { on.delete(method); ok(p); }));
  const errors = []; on.set('Runtime.exceptionThrown', p => errors.push(p.exceptionDetails.exception?.description || p.exceptionDetails.text));
  const close = () => { try { ws.close(); } catch {} proc.kill(); try { fs.rmSync(dir, { recursive: true, force: true }); } catch {} };
  return { send, once, close, errors };
}
const sleep = ms => new Promise(r => setTimeout(r, ms));

// Serves the pages as the app does: /pets, and under /ui/ only the files the app's server lists (ASSETS in
// src-tauri/src/server.rs) and its fonts, so a file the app would not serve fails here too.
const APP_ASSETS = JSON.parse(fs.readFileSync(path.join(__dirname, '../src-tauri/src/server.rs'), 'utf8').match(/const ASSETS: \[&str; \d+\] = (\[[^\]]*\])/)[1]);
const appServes = rel => APP_ASSETS.includes(rel) || /^fonts\/[a-z0-9-]+\.(woff2|css)$/.test(rel);
function server() {
  const types = { '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png' };
  const s = http.createServer((q, r) => {
    const u = new URL(q.url, 'http://x').pathname, f = u === '/pets' ? path.join(UI, 'pets.html') : u.startsWith('/ui/') && appServes(u.slice(4)) ? path.join(UI, u.slice(4)) : null;
    if (!f || !f.startsWith(UI) || !fs.existsSync(f)) { r.writeHead(404); r.end(); return; }
    r.writeHead(200, { 'content-type': types[path.extname(f)] || 'application/octet-stream' }); r.end(fs.readFileSync(f));
  });
  return new Promise(ok => s.listen(0, '127.0.0.1', () => ok(s)));
}

// Before the page runs: a clock that only moves when the test says (requestAnimationFrame, performance.now, Date.now),
// the app's bridge with the settings, and the session list pushed by the test instead of the app's event stream.
const PRELUDE = `(() => {
  let T = 1000; const D0 = 1.8e12, q = [], noop = () => {};
  performance.now = () => T; Date.now = () => D0 + T;
  window.requestAnimationFrame = f => q.push(f); window.cancelAnimationFrame = noop;
  window.__step = ms => { T += ms; const fs = q.splice(0); for (const f of fs) f(T); if (typeof wake === 'function') wake(); };
  window.__now = () => D0 + T;
  window.bobix = { onMouse: noop, onSettings: f => { window.__apply = f; }, getState: () => new Promise(noop), petClick: noop, setPet: noop, setRects: noop, answer: noop };
  window.EventSource = class { constructor() { window.__es = this; } close() {} };
  window.__push = list => window.__es.onmessage({ data: JSON.stringify({ sessions: list }) });
  addEventListener('DOMContentLoaded', () => { const st = document.createElement('style'); st.textContent =
    '*,*::before,*::after{animation-play-state:paused!important;transition:none!important}' +
    'body.__only *{visibility:hidden!important} body.__only .__on,body.__only .__on *{visibility:visible!important}' +
    '.__off,.__off *{visibility:hidden!important}'; document.head.append(st); });
})();`;

// Every pet that shows an effect now: its effect element and the box to look at (its element on screen).
const PROBE = `(() => { const out = []; let k = 0; for (const e of document.querySelectorAll('[data-probe]')) delete e.dataset.probe;
  for (const el of document.querySelectorAll('.pet')) {
    if (el.style.visibility === 'hidden') continue;
    const node = el.querySelector('.spr>.fxs') || el.querySelector('.spr>canvas.fxc');
    if (!node || getComputedStyle(node).visibility === 'hidden') continue;
    const r = node.getBoundingClientRect(); node.dataset.probe = el.dataset.probe = String(++k);
    out.push({ k, id: [...pets.values()].find(q => q.el === el)?.s.id || '', x: r.left, y: r.top, w: r.width, h: r.height });
  } return out; })()`;

// Every pet's state and boxes (left, right, top, bottom), for a failing check's dump.
const STATE = `[...pets.values(), ...leaving].map(p => { const r = e => e ? (b => [b.left, b.right, b.top, b.bottom].map(v => Math.round(v * 10) / 10))(e.getBoundingClientRect()) : null, n = p.sheetShown ? p.sheetEl : p.cv;
  return { id: p.s.id, state: p.s.state, parent: p.s.parent, cls: p.el.className, x: p.x, fade: p.el.style.opacity, probe: p.el.dataset.probe || null, kind: p.fx?.kind, total: p.fx && p.fx.total, queue: (p.queue || []).map(q => q[0]),
    sheet: !!p.sheetShown, el: r(p.el), spr: r(p.el.querySelector('.spr')), effect: n?.isConnected ? r(n) : null, effectVisibility: n?.isConnected ? getComputedStyle(n).visibility : null, clip: n?.style.clipPath || '',
    lay: p.lay && { L: p.lay.L, R: p.lay.R, T: p.lay.T, B: p.lay.B, crowded: p.lay.crowded }, tags: [...p.el.querySelectorAll('.tag')].map(e => ({ cls: e.className, text: e.textContent, box: r(e) })),
    others: [...p.el.querySelectorAll('.bb, .say, .card')].map(e => ({ cls: e.className, box: r(e) })) }; })`;

// A session as the app sends it.
function ses(id, o = {}) {
  return { id, name: o.cwd ? path.basename(o.cwd) : 'alpha', cwd: o.cwd || '/tmp/alpha', project: o.cwd || '/tmp/alpha', title: '', transcript: '', state: 'idle', since: 0, updated: 0,
    lv: 1, xp: 0, quest: 'go', prompt: 'go', tool: '', toolArg: '', askTool: '', ask: '', summary: '', report: '', hits: 0, ctx: null, claudePid: null,
    hostSession: null, log: [], pet: 'auto', parent: null, linking: false, image: null, pixel: false, whole: false, issue: null, ...o };
}

async function run(b, url, settings, script, label) {
  await b.send('Page.navigate', { url }); await b.once('Page.loadEventFired');
  const ev = async x => { const r = await b.send('Runtime.evaluate', { expression: x, returnByValue: true, awaitPromise: true }); if (r.exceptionDetails) b.errors.push(r.exceptionDetails.exception?.description || r.exceptionDetails.text); return r.result.value; };
  await ev(`__apply(${JSON.stringify(settings)})`);
  const now = () => ev('__now()'), bad = []; let seen = 0;
  // Effects are off with reduced motion (pets.html, FX_ON): say so instead of "no effect was shown".
  if (!await ev('FX_ON')) return [`effects are off in the page (prefers-reduced-motion: reduce: ${await ev("matchMedia('(prefers-reduced-motion: reduce)').matches")})`];
  let t = 0;
  const push = async list => { const n = await now(); await ev(`__push(${JSON.stringify(list.map(s => ({ ...s, since: n, updated: n })))})`); };
  const steps = async (sec, sample) => {
    for (let i = 0; i < Math.round(sec * 30); i++) {
      await ev('__step(33.4)'); t += 33.4; if (i % 3 === 0) await sleep(2); // let pictures decode
      if (sample && i % sample === sample - 1) await check();
    }
  };
  // No two pets in sight overlap (their boxes, a side pet's drawn at 80%).
  const BODIES = `(() => { const b = [...document.querySelectorAll('.pet:not(.line-off)')].filter(e => +e.style.opacity > .05).map(e => { const r = e.querySelector('.spr').getBoundingClientRect(), k = e.classList.contains('side') ? .1 : 0, w = r.width * k; return [r.left + w, r.right - w]; });
    const out = []; for (let i = 0; i < b.length; i++) for (let j = i + 1; j < b.length; j++) if (b[i][1] > b[j][0] + .5 && b[j][1] > b[i][0] + .5) out.push([b[i], b[j]].map(r => r.map(Math.round).join('-')).join(' / '));
    // Name tags never touch (1 px apart at least), except while a pet is held or is still sliding from a drop.
    if (!LINEUP.held && !LINEUP.vis.some(e => e.slide)) {
      const t = [...document.querySelectorAll('.pet:not(.line-off) .tag:not(.ghost)')].filter(e => +e.closest('.pet').style.opacity > .05).map(e => e.getBoundingClientRect());
      for (let i = 0; i < t.length; i++) for (let j = i + 1; j < t.length; j++) if (t[i].right + 1 > t[j].left && t[j].right + 1 > t[i].left) out.push('tags touch: ' + [t[i], t[j]].map(r => Math.round(r.left) + '-' + Math.round(r.right)).join(' / '));
    }
    return out; })()`;
  // One effect against the rest: pixels both screenshots draw.
  const check = async () => {
    for (const o of await ev(BODIES)) bad.push(`t=${(t / 1000).toFixed(2)}s pets overlap: ${o}`);
    for (const p of await ev(PROBE)) {
      seen++;
      const x = Math.max(0, Math.floor(p.x)), y = Math.max(0, Math.floor(p.y)), r = Math.min(W, Math.ceil(p.x + p.w)), bo = Math.min(H, Math.ceil(p.y + p.h));
      if (r <= x || bo <= y) continue;
      const clip = { x, y, width: r - x, height: bo - y, scale: 1 }, shot = async () => { const d = (await b.send('Page.captureScreenshot', { format: 'png', clip })).data; return { ...png(Buffer.from(d, 'base64')), d }; };
      // A loop's sheet can take its canvas's place between two calls (its picture decodes in real time): then this pet
      // waits for the next look. The marks are taken off the very elements they were put on (window.__look), never looked
      // up again: a canvas swapped out mid-look would keep its mark, come back with its next effect, and show in every
      // other pet's "effect alone" screenshot.
      if (!await ev(`(() => { const n = document.querySelector('[data-probe="${p.k}"]:not(.pet)'); if (!n) return false; window.__look = [n, document.querySelector('.pet[data-probe="${p.k}"] .spr')];
        document.body.classList.add('__only'); n.classList.add('__on'); return true; })()`)) continue;
      const fx = await shot();
      await ev(`document.body.classList.remove('__only'); __look[0].classList.remove('__on'); __look[1]?.classList.add('__off')`);
      const rest = await shot();
      if (!await ev(`(() => { __look[1]?.classList.remove('__off'); return __look[0].isConnected; })()`)) continue;
      let n = 0, x0 = 1e9, y0 = 1e9, x1 = -1, y1 = -1;
      for (let j = 0; j < fx.w * fx.h; j++) if (fx.data[j * 4 + 3] > 8 && rest.data[j * 4 + 3] > 8) { n++; const px = j % fx.w, py = (j / fx.w) | 0; x0 = Math.min(x0, px); x1 = Math.max(x1, px); y0 = Math.min(y0, py); y1 = Math.max(y1, py); }
      if (n && process.env.OVERLAY_DUMP) {
        const o = new Uint8Array(fx.w * fx.h * 4);
        for (let j = 0; j < fx.w * fx.h; j++) { const f = fx.data[j * 4 + 3] > 8, q = rest.data[j * 4 + 3] > 8, s = f && q ? [255, 0, 255, 255] : f ? [...fx.data.subarray(j * 4, j * 4 + 3), 255] : q ? [...rest.data.subarray(j * 4, j * 4 + 3), 120] : [0, 0, 0, 0]; o.set(s, j * 4); }
        const base = path.join(process.env.OVERLAY_DUMP, `${label}-${(t / 1000).toFixed(2)}-${p.k}`.replace(/[^\w.-]+/g, '_'));
        fs.mkdirSync(process.env.OVERLAY_DUMP, { recursive: true }); fs.writeFileSync(base + '.png', pngOut(fx.w, fx.h, o));
        fs.writeFileSync(base + '-effect.png', Buffer.from(fx.d, 'base64')); fs.writeFileSync(base + '-rest.png', Buffer.from(rest.d, 'base64'));
        fs.writeFileSync(base + '-window.png', Buffer.from((await b.send('Page.captureScreenshot', { format: 'png' })).data, 'base64'));
        fs.writeFileSync(base + '-state.json', JSON.stringify({ probe: p, clip, window: [W, H], dpr: await ev('devicePixelRatio'), fonts: await ev("[...document.fonts].map(f => f.family + ' ' + f.weight + ' ' + f.status)"), pets: await ev(STATE) }, null, 1));
      }
      if (n) bad.push(`t=${(t / 1000).toFixed(2)}s pet ${p.k} (${p.id}): ${n} px at x ${x + x0}-${x + x1}, y ${y + y0}-${y + y1}`);
    }
  };
  const mouse = (type, x, y) => b.send('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1 });
  await script({ push, steps, ses, ev, mouse, fail: m => bad.push(`t=${(t / 1000).toFixed(2)}s ${m}`) });
  if (!seen) bad.push('no effect was shown');
  bad.push(...b.errors.splice(0).map(e => `page error: ${e}`));
  return bad;
}

// The cases bobo's recordings showed broken, at each edge and size: packed families (a parent and 3 or 10 side
// sessions, waiting, working with each tool, finishing, low on context) and separate pets with props walking past.
const fam = (n, top) => async ({ push, steps, ses }) => {
  const P = 'p0', kids = Array.from({ length: n }, (_, i) => `k${i + 1}`), host = { hostSession: 'local_p0' };
  const tools = ['Bash', 'Read', 'Edit', 'Grep'];
  const at = st => [ses(P, { ...host, ...st.p }), ...kids.map((k, i) => ses(k, { parent: P, hostSession: `local_${k}`, ...st.k(i) }))];
  await push(at({ p: { state: 'working', tool: 'Bash' }, k: i => ({ state: 'working', tool: tools[i % 4] }) }));
  await steps(6, 10);
  await push(at({ p: { state: 'waiting' }, k: i => (i === 0 ? { state: 'done', xp: 1 } : i === 1 ? { state: 'waiting' } : i === 2 ? { state: 'working', tool: 'Grep', ctx: 18 } : { state: 'working', tool: tools[i % 4] }) }));
  await steps(5, 6);
};
// Low on context, so each plays its tool card (a pet with context left holds its tool in the costume instead).
const separate = async ({ push, steps, ses }) => {
  const one = (id, tool) => ses(id, { cwd: `/tmp/${id}`, state: 'working', tool, ctx: 18 });
  await push([one('pa', 'Bash'), one('pb', 'Edit'), one('pc', 'Read'), one('pd', 'Grep')]);
  await steps(4, 10);
  await push([one('pa', 'Bash'), one('pc', 'Read'), one('pd', 'Grep'), one('pe', 'Bash')]);
  await steps(3, 4);
  await push([one('pc', 'Read'), one('pd', 'Grep'), one('pe', 'Bash'), one('pf', 'Edit')]);
  await steps(5, 4);
};

// With reduced motion: no effect at all, and each pet that needs you holds its own balloon instead.
async function reducedMotion(b, url) {
  await b.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
  try {
    await b.send('Page.navigate', { url }); await b.once('Page.loadEventFired');
    const ev = async x => (await b.send('Runtime.evaluate', { expression: x, returnByValue: true })).result.value;
    await ev(`__apply(${JSON.stringify({ edge: 'bottom', size: 56, zoneStart: 20, zoneEnd: 60, fadeNearCursor: false, showIdle: true, sleepHide: 0, language: 'en' })})`);
    const n = await ev('__now()');
    await ev(`__push(${JSON.stringify([ses('w', { state: 'waiting', since: n }), ses('d', { cwd: '/tmp/d', state: 'done', since: n }), ses('t', { cwd: '/tmp/t', state: 'working', tool: 'Bash', since: n })])})`);
    for (let i = 0; i < 60; i++) await ev('__step(33.4)');
    return await ev(`({ fxOn: FX_ON, effects: document.querySelectorAll('.fxc,.fxs').length, balloons: [...document.querySelectorAll('.pet:not(.line-off) .bb')].map(e => e.className).sort() })`);
  } finally { await b.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'no-preference' }] }); }
}

// "Show when needed" (pets-ondemand.js), on a 1440×900 work area: only the sessions that need you get a pet, which
// rises under its balloon from below the screen to past the top and never stops on the way, nor touches another pet;
// pointed at, it stops and shows its card; Allow on the card answers and it goes; a finished task crosses once;
// "do not disturb" hides them all and they come back after it. With reduced motion they stand near the bottom edge.
async function onDemand(b, url, rm) {
  const out = [], fail = m => out.push(`show when needed${rm ? ' (reduced motion)' : ''}: ${m}`);
  await b.send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
  if (rm) await b.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
  try {
    await b.send('Page.navigate', { url }); await b.once('Page.loadEventFired');
    const ev = async x => { const r = await b.send('Runtime.evaluate', { expression: x, returnByValue: true }); if (r.exceptionDetails) b.errors.push(r.exceptionDetails.exception?.description || r.exceptionDetails.text); return r.result.value; };
    await ev(`__apply(${JSON.stringify({ edge: 'bottom', size: 56, petMode: 'ondemand', dndUntil: 0, zoneStart: 0, zoneEnd: 100, fadeNearCursor: false, showIdle: true, sleepHide: 0, language: 'en' })}); bobix.answer = (id, c) => { window.__ans = [id, c]; }`);
    const n = await ev('__now()'), one = (id, o) => ses(id, { cwd: `/tmp/${id}`, state: 'working', tool: 'Bash', ctx: 60, since: n, ...o });
    const list = [one('pa', { title: 'A session with a long title for its name tag', state: 'waiting', askTool: 'Bash', ask: 'npm test', answer: { id: 'a-pa', until: null } }), one('pb', { state: 'done' }), one('pc', { ctx: 18 }), one('pd'), one('pe')];
    await ev(`__push(${JSON.stringify(list)})`);
    const state = () => ev(`({ pets: [...OD.pets.values()].map(p => ({ id: p.s.id, y: p.y, box: p.box, hover: p.hover, leaving: !!p.leaving, card: p.el.querySelector('.card')?.offsetParent != null })), edge: document.querySelectorAll('.pet:not(.od)').length, pill: document.querySelector('#odPill span')?.textContent || '' })`);
    const lastY = new Map(); let t = 0;
    const steps = async (sec, check = true) => {
      for (let i = 0; i < Math.round(sec * 30); i++) {
        await ev('__step(33.4)'); t += 33.4;
        if (!check || i % 5) continue;
        const st = await state(), at = `t=${(t / 1000).toFixed(2)}s`;
        if (st.edge) fail(`${at} ${st.edge} pets on the edge`);
        for (const p of st.pets) if (!['pa', 'pb', 'pc'].includes(p.id)) fail(`${at} ${p.id} shows, but needs nothing`);
        const live = st.pets.filter(p => !p.leaving);
        for (let a = 0; a < live.length; a++) for (let c = a + 1; c < live.length; c++) { const [x, y] = [live[a].box, live[c].box]; if (x.l < y.r && y.l < x.r && x.t < y.b && y.t < x.b) fail(`${at} ${live[a].id} and ${live[c].id} overlap`); }
        for (const p of live) {
          const was = lastY.get(p.id);
          if (!rm && was != null && p.y > was + .5 && p.y < 800) fail(`${at} ${p.id} went down (${Math.round(was)} → ${Math.round(p.y)})`); // (or started a new crossing from below)
          if (rm && p.box.b < 900 - 56 - 40) fail(`${at} ${p.id} stands at ${Math.round(p.box.b)}, not near the bottom edge`);
          if (rm && was != null && p.y !== was) fail(`${at} ${p.id} moved with reduced motion`);
          lastY.set(p.id, p.y);
        }
      }
    };
    await steps(5);
    let st = await state();
    if (!/5 sessions · 1 need you/.test(st.pill)) fail(`the pill says "${st.pill}"`);
    const pa = st.pets.find(p => p.id === 'pa');
    if (!pa) fail('no pet for the waiting session');
    else {
      if (!rm && !(pa.box.b < 900 && pa.box.t > 0)) fail(`at 5 s the waiting pet is not on screen (${Math.round(pa.box.t)}-${Math.round(pa.box.b)})`);
      const mx = (pa.box.l + pa.box.r) / 2, my = pa.box.b - 20;
      await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: mx, y: my });
      // While its card is open (3 s, the other pets still rising): the card covers no other pet (its body and balloon)
      // and none of the hovered pet itself (body, balloon, name tag); its name tag covers no other pet either.
      const cardCheck = `(() => { const p = OD.pets.get('pa'), c = p?.el.querySelector('.card'), tag = p?.el.querySelector('.tag'); if (!p?.hover || !c?.offsetParent) return null;
        const R = n => { const b = n.getBoundingClientRect(); return { l: b.left, t: b.top, r: b.right, b: b.bottom }; }, hit = (a, b) => a.l < b.r && b.l < a.r && a.t < b.b && b.t < a.b, k = R(c);
        return { others: [...OD.pets.values()].filter(q => q !== p && !q.leaving && (hit(k, q.box) || (tag && hit(R(tag), q.box)))).map(q => q.s.id), own: hit(k, p.box), tag: !!tag && hit(k, R(tag)), tagShown: !!tag?.offsetParent }; })()`;
      for (let i = 0; i < 6; i++) {
        await steps(.5, false);
        const c = await ev(cardCheck), at = `${(t / 1000).toFixed(2)}s`;
        if (!c) { fail(`${at} the hovered pet shows no card`); break; }
        if (c.others.length) fail(`${at} the open card or its pet's name tag covers ${c.others.join(', ')}`);
        if (c.own || c.tag || !c.tagShown) fail(`${at} the open card covers the hovered pet (${c.own ? 'its body or balloon' : ''}${c.tag ? ' its name tag' : ''}${c.tagShown ? '' : ' its name tag is not shown'})`);
      }
      const a1 = (await state()).pets.find(p => p.id === 'pa'); await steps(1, false); const a2 = (await state()).pets.find(p => p.id === 'pa');
      if (!a1?.hover || !a1.card || a1.y !== a2?.y) fail(`pointed at, the waiting pet does not stop with its card open: ${JSON.stringify([a1, a2].map(p => p && [p.hover, p.card, Math.round(p.y)]))}`);
      const btn = await ev(`(() => { const k = OD.pets.get('pa')?.el.querySelector('[data-answer=allow]')?.getBoundingClientRect(); return k ? [k.left + k.width / 2, k.top + k.height / 2] : null; })()`);
      if (!btn) fail('no Allow button on its card');
      else {
        for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) await b.send('Input.dispatchMouseEvent', { type, x: btn[0], y: btn[1], button: 'left', buttons: type === 'mousePressed' ? 1 : 0, clickCount: 1 });
        await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
        await steps(2, false);
        if (JSON.stringify(await ev('window.__ans')) !== '["a-pa","allow"]') fail(`Allow answered ${JSON.stringify(await ev('window.__ans'))}`);
        if ((await state()).pets.some(p => p.id === 'pa')) fail('the answered pet is still there 2 s later');
      }
    }
    await ev(`__apply({ dndUntil: Date.now() + 30 * 60e3 })`); await steps(.5, false); st = await state();
    if (st.pets.length || !st.pill.includes('☾')) fail(`with "do not disturb" on: ${st.pets.length} pets, the pill says "${st.pill}"`);
    await ev(`__apply({ dndUntil: 0 })`); lastY.clear(); await steps(.5, false); st = await state();
    if (!st.pets.some(p => p.id === 'pc')) fail('after "do not disturb", the low-context pet does not come back');
    if (!rm) { // a finished task crosses once, then goes for good
      await steps(14); st = await state();
      if (st.pets.some(p => p.id === 'pb')) fail(`the finished task still shows 14 s later (y ${Math.round(st.pets.find(p => p.id === 'pb').y)})`);
    }
  } finally {
    await b.send('Emulation.setDeviceMetricsOverride', { width: W, height: H, deviceScaleFactor: 1, mobile: false });
    await b.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'no-preference' }] });
  }
  return out;
}

// One card at a time (edge mode): a family of 3 stands close (side pets overlap their neighbours' boxes a little). The
// pointer on the overlap of two pets, then moved straight from one pet to the next: every frame shows one card, the
// nearest pet's.
async function oneCard(b, url) {
  const out = [];
  await b.send('Page.navigate', { url }); await b.once('Page.loadEventFired');
  const ev = async x => { const r = await b.send('Runtime.evaluate', { expression: x, returnByValue: true }); if (r.exceptionDetails) b.errors.push(r.exceptionDetails.exception?.description || r.exceptionDetails.text); return r.result.value; };
  await ev(`__apply(${JSON.stringify({ edge: 'bottom', size: 56, zoneStart: 30, zoneEnd: 70, fadeNearCursor: false, showIdle: true, sleepHide: 0, language: 'en' })})`);
  const n = await ev('__now()'), host = { hostSession: 'local_p0' };
  await ev(`__push(${JSON.stringify([ses('p0', { ...host, state: 'working', tool: 'Bash', since: n }), ...['k1', 'k2'].map(k => ses(k, { parent: 'p0', hostSession: 'local_' + k, state: 'working', tool: 'Read', since: n }))])})`);
  const step = async k => { for (let i = 0; i < k; i++) await ev('__step(33.4)'); };
  await step(60);
  const at = await ev(`Object.fromEntries(['p0', 'k1', 'k2'].map(id => { const r = pets.get(id).el.getBoundingClientRect(); return [id, [r.left, r.right, r.top, r.bottom]]; }))`);
  // The family's name starts under the parent: the name sits nearer the parent than any of its side sessions.
  { const near = await ev(`(() => { const mid = e => { const r = e.getBoundingClientRect(); return (r.left + r.right) / 2; };
      const n = pets.get('p0').el.querySelector('.tag .nm'), range = document.createRange(); range.selectNodeContents(n);
      const t = range.getBoundingClientRect(), c = (t.left + t.right) / 2, at = id => mid(pets.get(id).el.querySelector('.spr'));
      return { name: Math.round(c), p0: Math.round(at('p0')), k1: Math.round(at('k1')), k2: Math.round(at('k2')) }; })()`);
    if (Math.abs(near.name - near.p0) >= Math.min(Math.abs(near.name - near.k1), Math.abs(near.name - near.k2))) out.push(`the family's name is not under its parent: ${JSON.stringify(near)}`); }
  const cards = () => ev(`[...pets.values()].filter(p => p.el.querySelector('.card')?.offsetParent).map(p => p.s.id)`);
  const move = (x, y) => b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  const [a, c] = [at.p0, at.k1].sort((u, v) => u[0] - v[0]), y = (a[2] + a[3]) / 2; // the left one and the right one
  const ids = Object.keys(at), idOf = r => ids.find(k => at[k] === r);
  // On the overlap (or the seam) of the two, nearer the right one, then nearer the left one.
  for (const [x, want] of [[Math.max(c[0], a[1] - 4) + 2, idOf(c)], [Math.min(a[1], c[0] + 4) - 2 - 6, idOf(a)]]) {
    await move(x, y);
    for (let i = 0; i < 6; i++) { await step(1); const k = await cards(); if (k.length !== 1 || k[0] !== want) out.push(`pointer at x ${Math.round(x)} (pets ${at.p0.map(Math.round).slice(0, 2).join('-')}, ${at.k1.map(Math.round).slice(0, 2).join('-')}), frame ${i + 1}: cards ${JSON.stringify(k)}, want only ${want}`); }
  }
  // Straight from one pet to the next: the first card goes as the next opens (no 250 ms of both).
  await move((at.k2[0] + at.k2[1]) / 2, y); await step(10);
  await move((at.k1[0] + at.k1[1]) / 2, y);
  for (let i = 0; i < 6; i++) { await step(1); const k = await cards(); if (k.length > 1) out.push(`moved from k2 to k1, frame ${i + 1}: cards ${JSON.stringify(k)}`); }
  await move(1, 1);
  return out.map(o => 'one card at a time: ' + o);
}

// A pet dragged onto its neighbour and let go: no overlap after it, and 1.5 s after the drop the line is evenly spaced
// again (every gap the line's own gap, nothing still sliding).
// Every frame right after a drop: no two pets in sight overlap and no two name tags touch, sliding or not.
const NO_TOUCH = `(() => { const out = [], shown = e => +e.closest('.pet').style.opacity > .05;
  const b = [...document.querySelectorAll('.pet:not(.line-off) .spr')].filter(shown).map(e => { const r = e.getBoundingClientRect(), w = e.closest('.pet').classList.contains('side') ? r.width * .1 : 0; return [r.left + w, r.right - w]; });
  for (let i = 0; i < b.length; i++) for (let j = i + 1; j < b.length; j++) if (b[i][1] > b[j][0] + .5 && b[j][1] > b[i][0] + .5) out.push('pets overlap: ' + [b[i], b[j]].map(r => r.map(Math.round).join('-')).join(' / '));
  const t = [...document.querySelectorAll('.pet:not(.line-off) .tag:not(.ghost)')].filter(shown).map(e => e.getBoundingClientRect());
  for (let i = 0; i < t.length; i++) for (let j = i + 1; j < t.length; j++) if (t[i].right + 1 > t[j].left && t[j].right + 1 > t[i].left) out.push('tags touch: ' + [t[i], t[j]].map(r => Math.round(r.left) + '-' + Math.round(r.right)).join(' / '));
  return out; })()`;
const dragged = async ({ push, steps, ses, ev, mouse, fail }) => {
  const one = (id, tool) => ses(id, { cwd: `/tmp/${id}`, state: 'working', tool, ctx: 18 }); // tool cards, as in separate
  await push([one('pa', 'Bash'), one('pb', 'Edit'), one('pc', 'Read'), one('pd', 'Grep')]);
  await steps(2, 10);
  const at = id => ev(`(() => { const b = pets.get('${id}').el.querySelector('.spr').getBoundingClientRect(); return [b.left + b.width / 2, b.top + b.height / 2]; })()`);
  const [from, to] = [await at('pb'), await at('pc')];
  await mouse('mousePressed', ...from);
  for (let i = 1; i <= 10; i++) { await mouse('mouseMoved', from[0] + (to[0] - from[0]) * i / 10, from[1]); await steps(1 / 30); }
  await mouse('mouseReleased', ...to); await mouse('mouseMoved', 1, 1); // and away, so the line is not held by a hover
  for (let i = 1; i <= 30; i++) { await steps(1 / 30); for (const o of await ev(NO_TOUCH)) fail(`frame ${i} after the drop: ${o}`); } // every frame drawn since the drop
  await steps(.5, 6);
  const line = await ev(`(() => { const all = [...pets.values()], W = q => unitWidth(unitOf(q, all)), L = [...LINEUP.vis].sort((a, b) => b.f - a.f), { min, max } = zone();
    return { gaps: L.slice(1).map((e, i) => (L[i].f - W(L[i].p)) - e.f), want: lineGap([...all, ...leaving].filter(q => !attached(q.s)).map(W), max + cfg.size - min), sliding: L.filter(e => Math.abs(e.slide || 0) > .5).length, held: !!LINEUP.held }; })()`);
  if (line.held || line.sliding || line.gaps.some(g => Math.abs(g - line.want) > 2)) fail(`1.5 s after the drop the line is not evenly spaced: ${JSON.stringify(line)}`);
  await steps(2, 6);
};

// Costumes by state (costume.js) on the real overlay: every costume pixel of every pet in sight, as its picture shows
// it now, stays in its own box, off every name tag, off its "z" and off every other pet's box; the XP and the battery
// play on the undressed pet; a held tool takes the tool card's place; the held tools and the finish flag move (4 frames).
const COSTUME_DOM = `(() => { const out = [], R = n => n.getBoundingClientRect(), hit = (a, b) => a.l < b.right && b.left < a.r && a.t < b.bottom && b.top < a.b;
  const shown = [...pets.values()].filter(q => !q.el.classList.contains('line-off') && +q.el.style.opacity > .05), F = window.__fr ||= {};
  const tags = [...document.querySelectorAll('.pet:not(.line-off) .tag:not(.ghost)')].map(R);
  for (const p of shown) {
    const st = look(p.s), art = p.art?.[0] || p.s, kind = costumeOf(art, p.s, st), id = p.s.id; if (!kind) continue;
    if (p.fx && COSTUME.after(p.fx.kind) && p.fxO.dress) out.push(id + ': dressed while its ' + p.fx.kind + ' plays');
    if (p.fx?.kind === 'tool' && COSTUME.holdsTool(kind, p.s.tool)) out.push(id + ': a tool card and a held tool');
    if (p.fx && p.fxO.dress === kind && !F[id]?.includes('fx')) (F[id] ||= []).push('fx'); // worn in its effect (the aura, a tool card)
    const svg = p.el.querySelector('.spr>svg'); if (!svg || getComputedStyle(svg).visibility === 'hidden') continue;
    (F[id] ||= []).includes(p.fr) || F[id].push(p.fr);
    const only = COSTUME.dress(fineSprite(art.project || art.id, st, p.art[1]), kind, { left: rtl(), tool: p.s.tool, frame: Math.max(0, p.fr), only: true });
    const b = R(svg), u = b.width / 36, box = R(p.el.querySelector('.spr')), zz = p.el.querySelector('.zz'), z = zz && getComputedStyle(zz).visibility !== 'hidden' ? R(zz) : null;
    const others = shown.filter(q => q !== p).map(q => R(q.el.querySelector('.spr'))), n = { box: 0, tag: 0, z: 0, pet: 0 };
    for (let y = 0; y < 36; y++) for (let x = 0; x < 36; x++) if (only.data[y * 36 + x] >>> 24) {
      const r = { l: b.left + x * u, t: b.top + y * u, r: b.left + (x + 1) * u, b: b.top + (y + 1) * u };
      if (r.l < box.left - .5 || r.r > box.right + .5 || r.t < box.top - .5 || r.b > box.bottom + .5) n.box++;
      if (tags.some(t => hit(r, t))) n.tag++; if (z && hit(r, z)) n.z++; if (others.some(o => hit(r, o))) n.pet++;
    }
    for (const [k, v] of Object.entries(n)) if (v) out.push(id + ' (' + kind + '): ' + v + ' costume px ' + { box: 'outside its box', tag: 'on a name tag', z: 'on its z', pet: 'on another pet' }[k]);
  } return out; })()`;
const costumes = async ({ push, steps, ses, ev, fail }) => {
  const one = (id, o) => ses(id, { cwd: `/tmp/${id}`, state: 'working', tool: 'Bash', ctx: 60, ...o });
  const all = (o = {}) => [one('edit', { tool: 'Edit' }), one('bash'), one('read', { tool: 'Read' }), one('web', { tool: 'WebSearch' }), one('wait', o.wait), one('done', o.done), one('ctx', o.ctx), one('sleep', o.sleep)];
  await push(all());
  await steps(1, 10);
  await push(all({ wait: { state: 'waiting', askTool: 'Bash' }, done: { state: 'done', xp: 1 }, ctx: { ctx: 18, tool: 'Grep' }, sleep: { state: 'idle' } }));
  for (let i = 0; i < 28; i++) { await steps(.25, 4); for (const o of await ev(COSTUME_DOM)) fail(o); }
  const fr = await ev('window.__fr');
  for (const id of ['edit', 'bash', 'read', 'web', 'done']) if ((fr[id] || []).filter(f => f >= 0).length < 4) fail(`${id}: its costume showed frames ${JSON.stringify(fr[id])}, not all 4`);
  for (const id of ['wait', 'ctx', 'sleep']) if (!fr[id]) fail(`${id}: its costume never showed`);
};

const exe = chrome();
test('no effect pixel lands on another pet, a tag, a balloon or another effect, and no pets overlap (real overlay, both edges, both ways, 32/56/128 px)', { skip: !exe && 'no Chrome', timeout: 600e3 }, async () => {
  const srv = await server(), b = await browser(exe), bad = [];
  try {
    await b.send('Page.enable'); await b.send('Runtime.enable');
    await b.send('Emulation.setDeviceMetricsOverride', { width: W, height: H, deviceScaleFactor: 1, mobile: false });
    await b.send('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } });
    await b.send('Page.addScriptToEvaluateOnNewDocument', { source: PRELUDE });
    const url = `http://127.0.0.1:${srv.address().port}/pets`;
    // What this machine asks for: CI runners (macOS, Windows) report reduced motion, which turns effects off.
    await b.send('Page.navigate', { url: 'about:blank' }); await b.once('Page.loadEventFired');
    const rm = (await b.send('Runtime.evaluate', { expression: "matchMedia('(prefers-reduced-motion: reduce)').matches", returnByValue: true })).result.value;
    console.log(`# this machine reports prefers-reduced-motion: ${rm ? 'reduce' : 'no-preference'}; effects are checked with it set to no-preference, and the reduced-motion look on its own`);
    const r = await reducedMotion(b, url);
    console.log(`# with reduced motion: ${JSON.stringify(r)}`);
    if (r.fxOn || r.effects || r.balloons.join() !== 'bb done,bb waiting') bad.push(`reduced motion: want no effect and a balloon on the waiting and the done pet, got ${JSON.stringify(r)}`);
    await b.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'no-preference' }] });
    if (!process.env.OVERLAY_ONLY || new RegExp(process.env.OVERLAY_ONLY).test(' 56 one card')) bad.push(...await oneCard(b, url), ...b.errors.splice(0).map(e => `one card: page error: ${e}`));
    if (!process.env.OVERLAY_ONLY || new RegExp(process.env.OVERLAY_ONLY).test(' 56 show when needed')) for (const rm of [false, true]) bad.push(...await onDemand(b, url, rm), ...b.errors.splice(0).map(e => `show when needed: page error: ${e}`));
    const cases = [];
    for (const edge of ['bottom', 'top']) for (const size of [32, 56, 128]) {
      cases.push([`${edge} ${size} family of 3, narrow zone`, { edge, size, zoneStart: 20, zoneEnd: 32 }, fam(3)]);
      cases.push([`${edge} ${size} family of 10`, { edge, size, zoneStart: 5, zoneEnd: 55 }, fam(10)]);
      cases.push([`${edge} ${size} separate pets coming and going`, { edge, size, zoneStart: 20, zoneEnd: size > 56 ? 45 : 32 }, separate]);
    }
    for (const edge of ['bottom', 'top']) cases.push([`${edge} 56 a pet dragged onto another and let go`, { edge, size: 56, zoneStart: 20, zoneEnd: 80 }, dragged]);
    for (const edge of ['bottom', 'top']) cases.push([`${edge} 56 family of 10, right to left`, { edge, size: 56, walkDir: 'rtl', zoneStart: 5, zoneEnd: 55 }, fam(10)]);
    for (const edge of ['bottom', 'top']) for (const [size, walkDir] of [[32, 'ltr'], [56, 'ltr'], [56, 'rtl'], [128, 'ltr']]) cases.push([`${edge} ${size} costumes, ${walkDir}`, { edge, size, walkDir, zoneStart: 5, zoneEnd: 95 }, costumes]);
    for (const [name, s, script] of cases.filter(c => !process.env.OVERLAY_ONLY || new RegExp(process.env.OVERLAY_ONLY).test(c[0]))) {
      const found = await run(b, url, { ...s, fadeNearCursor: false, showIdle: true, sleepHide: 0, language: 'en' }, script, name);
      bad.push(...found.slice(0, 6).map(f => `${name}: ${f}`));
    }
  } finally { b.close(); srv.close(); }
  assert.deepEqual(bad, []);
});
