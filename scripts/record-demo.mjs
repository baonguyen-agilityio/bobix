// Records docs/media/demo.mp4 + demo.gif from the real app with a scripted set of pretend sessions.
// Usage: node scripts/record-demo.mjs   (needs ffmpeg and the hooks token, i.e. the app ran once)
// It starts its own app instance (temporary data folder, port 4848), so your real sessions and settings
// never appear in the recording. Frames: the control panel, plus the pets strip along the bottom.
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';
import { execFileSync, spawn } from 'child_process';

const ROOT = path.join(path.dirname(new URL(import.meta.url).pathname), '..');
const OUT = path.join(ROOT, 'docs', 'media');
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-demo-'));
const FPS = 8, SECONDS = 16, W = 1600, H = 900, STRIP = 330;
const token = fs.readFileSync(path.join(os.homedir(), '.claude', 'bobix', 'token'), 'utf8').trim();
const PORT = 4848, DEBUG = 9334;
const DATA = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-demo-data-'));
fs.writeFileSync(path.join(DATA, 'settings.json'), JSON.stringify({
  port: PORT, language: 'en', size: 64, opacity: 1, edge: 'bottom', zoneStart: 0, zoneEnd: 100,
  showIdle: true, onlyAttention: false, paused: true, label: 'folder', fadeNearCursor: false,
}));
const electron = path.join(ROOT, 'node_modules', '.bin', 'electron');
const launch = args => spawn(electron, ['.', `--user-data-dir=${DATA}`, ...args], { cwd: ROOT, stdio: 'ignore' });
const app = launch([`--remote-debugging-port=${DEBUG}`]);
const sleep = ms => new Promise(r => setTimeout(r, ms));
await sleep(5000);
launch([]); // second instance: asks the first one to open its panel
await sleep(3000);

async function target(part) {
  const list = await (await fetch(`http://127.0.0.1:${DEBUG}/json`)).json();
  const t = list.find(x => x.type === 'page' && x.url.includes(part));
  if (!t) throw new Error(`no page matching ${part}`);
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  let id = 0; const wait = new Map();
  ws.onmessage = m => { const d = JSON.parse(m.data); wait.get(d.id)?.(d.result); };
  await new Promise(r => (ws.onopen = r));
  const call = (method, params = {}) => new Promise(r => { wait.set(++id, r); ws.send(JSON.stringify({ id, method, params })); });
  const run = async expr => (await call('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true }))?.result?.value;
  return { call, run, close: () => ws.close() };
}

// A permission request waits for an answer, like the real hook, so the pet shows Allow / Deny.
const post = ev => new Promise(res => {
  const body = JSON.stringify(ev);
  const wait = ev.hook_event_name === 'PermissionRequest' ? { 'x-bobix-wait': '1' } : {};
  const r = http.request({ host: '127.0.0.1', port: PORT, path: '/event', method: 'POST', headers: { 'content-type': 'application/json', 'x-bobix': token, ...wait } }, x => { x.resume(); res(); });
  r.on('error', () => {});
  r.end(body);
});

// Pretend sessions. Each gets a tiny transcript so the context meter shows a real reading.
const CTX = { billing: 64000, 'search-service': 118000, 'ios-app': 34000, infra: 156000, 'mobile-app': 12000 };
const transcript = name => {
  const f = path.join(DATA, `${name}.jsonl`);
  fs.writeFileSync(f, JSON.stringify({ type: 'assistant', message: { content: [{ type: 'text', text: 'Updated the guide and checked every link.' }], usage: { input_tokens: 10, cache_read_input_tokens: CTX[name] } } }) + '\n');
  return f;
};
// cwd is made up: the recording must never show the recorder's real home folder.
const S = name => ({ session_id: `demo-${name}`, cwd: `/Users/you/code/${name}`, transcript_path: transcript(name) });
const ev = (name, hook, extra = {}) => post({ ...S(name), hook_event_name: hook, ...extra });
const tool = (name, t, arg) => ev(name, 'PreToolUse', { tool_name: t, tool_input: arg });
const NAMES = Object.keys(CTX);

// The story, in seconds.
const SCRIPT = [
  [0.0, () => NAMES.forEach(n => ev(n, 'SessionStart', { source: 'startup' }))],
  [0.3, () => ev('billing', 'UserPromptSubmit', { prompt: 'Add retries to the Stripe webhook handler' })],
  [0.5, () => ev('search-service', 'UserPromptSubmit', { prompt: 'Speed up the autocomplete query' })],
  [0.7, () => ev('ios-app', 'UserPromptSubmit', { prompt: 'Fix dark mode colors on the settings screen' })],
  [0.9, () => ev('infra', 'UserPromptSubmit', { prompt: 'Document the new Terraform modules' })],
  [1.2, () => tool('billing', 'Read', { file_path: 'src/webhooks/stripe.ts' })],
  [1.6, () => tool('search-service', 'Grep', { pattern: 'autocomplete' })],
  [2.0, () => tool('ios-app', 'Edit', { file_path: 'Settings/Theme.swift' })],
  [2.4, () => tool('infra', 'Write', { file_path: 'docs/modules.md' })],
  [3.4, () => tool('billing', 'Bash', { command: 'npm test' })],
  [4.0, () => ev('billing', 'PermissionRequest', { tool_name: 'Bash', tool_input: { command: 'npm install p-retry' } })],
  [4.6, () => tool('search-service', 'Bash', { command: 'npm run bench' })],
  [6.0, () => tool('ios-app', 'Bash', { command: 'xcodebuild test' })],
  [8.0, () => ev('infra', 'Stop')],
  [10.0, () => ev('billing', 'PostToolUse', { tool_name: 'Bash' })],
  [10.3, () => tool('billing', 'Edit', { file_path: 'src/webhooks/retry.ts' })],
];

const pets = await target('/pets');
const panel = await target(`127.0.0.1:${PORT}/?`);
try {
  await panel.run(`showTab('sessions')`);
  await pets.call('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } });
  const dims = await pets.run('({ w: innerWidth, h: innerHeight })');
  const t0 = Date.now();
  let next = 0, frame = 0;
  while (Date.now() - t0 < SECONDS * 1000) {
    const t = (Date.now() - t0) / 1000;
    while (next < SCRIPT.length && SCRIPT[next][0] <= t) SCRIPT[next++][1]();
    // Show one pet's hover card near the end (normally the app feeds the real cursor position).
    if (t > 12.2 && t < 15.5) await pets.run(`(() => { const p = [...pets.values()].find(p => p.s.id === 'demo-ios-app'); if (p) p.hoverUntil = Date.now() + 400; })()`);
    const [strip, pane] = await Promise.all([
      pets.call('Page.captureScreenshot', { format: 'png', clip: { x: 0, y: dims.h - STRIP, width: dims.w, height: STRIP, scale: 1 } }),
      panel.call('Page.captureScreenshot', { format: 'png' }),
    ]);
    const n = String(frame++).padStart(4, '0');
    fs.writeFileSync(path.join(TMP, `strip-${n}.png`), Buffer.from(strip.data, 'base64'));
    fs.writeFileSync(path.join(TMP, `panel-${n}.png`), Buffer.from(pane.data, 'base64'));
    const wait = t0 + frame * 1000 / FPS - Date.now();
    if (wait > 0) await new Promise(r => setTimeout(r, wait));
  }
  console.log(`captured ${frame} frames`);

  // Stills for the README, from the same pretend sessions.
  fs.mkdirSync(OUT, { recursive: true });
  fs.copyFileSync(path.join(TMP, `panel-${String(Math.round(FPS * 5)).padStart(4, '0')}.png`), path.join(OUT, 'sessions.png'));
  const shot = async (name, js) => {
    await panel.run(js);
    await sleep(700);
    const r = await panel.call('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(path.join(OUT, name), Buffer.from(r.data, 'base64'));
  };
  await shot('settings.png', `showTab('settings'); scrollTo(0, 0)`);
  // Setup shows the settings file path: show the portable form, not this machine's home folder.
  await shot('setup.png', `showTab('setup'); new Promise(r => setTimeout(r, 400)).then(() => document.querySelectorAll('#setup code').forEach(c => c.textContent = '~/.claude/settings.json'))`);
} finally {
  pets.close(); panel.close();
  app.kill();
  await sleep(1000);
  fs.rmSync(DATA, { recursive: true, force: true });
}

// Compose: soft desktop, panel window centered above, pets strip at the bottom edge.
fs.mkdirSync(OUT, { recursive: true });
const filter = [
  `color=c=0xdfe3ee:s=${W}x${H}:r=${FPS}[bg]`,
  `[1:v]scale=${Math.round(W * 0.66)}:-1,pad=iw+2:ih+2:1:1:color=0xc9cedb[pn]`,
  `[0:v]scale=${W}:-1[st]`,
  `[bg][pn]overlay=(W-w)/2:28:shortest=1[a]`,
  `[a][st]overlay=0:H-h:shortest=1,format=yuv420p[v]`,
].join(';');
const inputs = ['-framerate', String(FPS), '-i', path.join(TMP, 'strip-%04d.png'), '-framerate', String(FPS), '-i', path.join(TMP, 'panel-%04d.png')];
execFileSync('ffmpeg', ['-y', '-loglevel', 'error', ...inputs, '-filter_complex', filter, '-map', '[v]', '-c:v', 'libx264', '-crf', '20', '-movflags', '+faststart', path.join(OUT, 'demo.mp4')]);
execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-i', path.join(OUT, 'demo.mp4'), '-vf', `fps=${FPS},scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128[p];[b][p]paletteuse=dither=none`, path.join(OUT, 'demo.gif')]);
fs.rmSync(TMP, { recursive: true, force: true });
console.log('wrote docs/media/demo.mp4 and docs/media/demo.gif');
