// The pets overlay (src/ui/pets.html) in a real headless Chrome, its clock stepped by hand: no pixel any effect draws may
// land on another pet, on any name tag, balloon, bubble or card, or on another pet's effect (a prop, an aura, a bar), and
// no two pets in sight ever stand on each other (also while one is dragged and let go, after which the line must be
// evenly spaced again). Checked on the composited page: for each pet with an effect, a
// screenshot of that effect alone against a screenshot of everything else on the page, pixel by pixel. Needs Chrome (CHROME_PATH, or the usual install place);
// without it the test is skipped.
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

// A PNG writer, for OVERLAY_DUMP (a folder): each failing check saved, the effect over the rest, its overlap in magenta.
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

function server() {
  const types = { '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png' };
  const s = http.createServer((q, r) => {
    const u = new URL(q.url, 'http://x').pathname, f = u === '/pets' ? path.join(UI, 'pets.html') : u.startsWith('/ui/') ? path.join(UI, path.normalize(u.slice(4))) : null;
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
    out.push({ k, id: el.dataset.id || '', x: r.left, y: r.top, w: r.width, h: r.height });
  } return out; })()`;

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
      const clip = { x, y, width: r - x, height: bo - y, scale: 1 }, shot = async () => png(Buffer.from((await b.send('Page.captureScreenshot', { format: 'png', clip })).data, 'base64'));
      // (A loop's sheet can take its canvas's place between two calls: then this pet waits for the next look.)
      const node = `document.querySelector('[data-probe="${p.k}"]:not(.pet)')`, spr = `document.querySelector('.pet[data-probe="${p.k}"] .spr')`;
      if (!await ev(`(() => { const n = ${node}; if (!n) return false; document.body.classList.add('__only'); n.classList.add('__on'); return true; })()`)) continue;
      const fx = await shot();
      await ev(`document.body.classList.remove('__only'); ${node}?.classList.remove('__on'); ${spr}?.classList.add('__off')`);
      const rest = await shot();
      if (!await ev(`(() => { ${spr}?.classList.remove('__off'); return !!${node}; })()`)) continue;
      let n = 0, x0 = 1e9, y0 = 1e9, x1 = -1, y1 = -1;
      for (let j = 0; j < fx.w * fx.h; j++) if (fx.data[j * 4 + 3] > 8 && rest.data[j * 4 + 3] > 8) { n++; const px = j % fx.w, py = (j / fx.w) | 0; x0 = Math.min(x0, px); x1 = Math.max(x1, px); y0 = Math.min(y0, py); y1 = Math.max(y1, py); }
      if (n && process.env.OVERLAY_DUMP) {
        const o = new Uint8Array(fx.w * fx.h * 4);
        for (let j = 0; j < fx.w * fx.h; j++) { const f = fx.data[j * 4 + 3] > 8, q = rest.data[j * 4 + 3] > 8, s = f && q ? [255, 0, 255, 255] : f ? [...fx.data.subarray(j * 4, j * 4 + 3), 255] : q ? [...rest.data.subarray(j * 4, j * 4 + 3), 120] : [0, 0, 0, 0]; o.set(s, j * 4); }
        fs.mkdirSync(process.env.OVERLAY_DUMP, { recursive: true }); fs.writeFileSync(path.join(process.env.OVERLAY_DUMP, `${label}-${(t / 1000).toFixed(2)}-${p.k}.png`.replace(/[^\w.-]+/g, '_')), pngOut(fx.w, fx.h, o));
      }
      if (n) bad.push(`t=${(t / 1000).toFixed(2)}s pet ${p.k}: ${n} px at x ${x + x0}-${x + x1}, y ${y + y0}-${y + y1}`);
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
const separate = async ({ push, steps, ses }) => {
  const one = (id, tool) => ses(id, { cwd: `/tmp/${id}`, state: 'working', tool });
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

// A pet dragged onto its neighbour and let go: no overlap after it, and 1.5 s after the drop the line is evenly spaced
// again (every gap the line's own gap, nothing still sliding).
const dragged = async ({ push, steps, ses, ev, mouse, fail }) => {
  const one = (id, tool) => ses(id, { cwd: `/tmp/${id}`, state: 'working', tool });
  await push([one('pa', 'Bash'), one('pb', 'Edit'), one('pc', 'Read'), one('pd', 'Grep')]);
  await steps(2, 10);
  const at = id => ev(`(() => { const b = pets.get('${id}').el.querySelector('.spr').getBoundingClientRect(); return [b.left + b.width / 2, b.top + b.height / 2]; })()`);
  const [from, to] = [await at('pb'), await at('pc')];
  await mouse('mousePressed', ...from);
  for (let i = 1; i <= 10; i++) { await mouse('mouseMoved', from[0] + (to[0] - from[0]) * i / 10, from[1]); await steps(1 / 30); }
  await mouse('mouseReleased', ...to); await mouse('mouseMoved', 1, 1); // and away, so the line is not held by a hover
  await steps(1.5, 6);
  const line = await ev(`(() => { const all = [...pets.values()], W = q => unitWidth(unitOf(q, all)), L = [...LINEUP.vis].sort((a, b) => b.f - a.f), { min, max } = zone();
    return { gaps: L.slice(1).map((e, i) => (L[i].f - W(L[i].p)) - e.f), want: lineGap([...all, ...leaving].filter(q => !attached(q.s)).map(W), max + cfg.size - min), sliding: L.filter(e => Math.abs(e.slide || 0) > .5).length, held: !!LINEUP.held }; })()`);
  if (line.held || line.sliding || line.gaps.some(g => Math.abs(g - line.want) > 2)) fail(`1.5 s after the drop the line is not evenly spaced: ${JSON.stringify(line)}`);
  await steps(2, 6);
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
    const cases = [];
    for (const edge of ['bottom', 'top']) for (const size of [32, 56, 128]) {
      cases.push([`${edge} ${size} family of 3, narrow zone`, { edge, size, zoneStart: 20, zoneEnd: 32 }, fam(3)]);
      cases.push([`${edge} ${size} family of 10`, { edge, size, zoneStart: 5, zoneEnd: 55 }, fam(10)]);
      cases.push([`${edge} ${size} separate pets coming and going`, { edge, size, zoneStart: 20, zoneEnd: size > 56 ? 45 : 32 }, separate]);
    }
    for (const edge of ['bottom', 'top']) cases.push([`${edge} 56 a pet dragged onto another and let go`, { edge, size: 56, zoneStart: 20, zoneEnd: 80 }, dragged]);
    for (const edge of ['bottom', 'top']) cases.push([`${edge} 56 family of 10, right to left`, { edge, size: 56, walkDir: 'rtl', zoneStart: 5, zoneEnd: 55 }, fam(10)]);
    for (const [name, s, script] of cases.filter(c => !process.env.OVERLAY_ONLY || new RegExp(process.env.OVERLAY_ONLY).test(c[0]))) {
      const found = await run(b, url, { ...s, fadeNearCursor: false, showIdle: true, sleepHide: 0, language: 'en' }, script, name);
      bad.push(...found.slice(0, 6).map(f => `${name}: ${f}`));
    }
  } finally { b.close(); srv.close(); }
  assert.deepEqual(bad, []);
});
