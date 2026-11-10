// shot-url.mjs URL OUT: headless Chrome screenshot of a public page; scrolls to the release's Assets list first.
import { spawn } from 'node:child_process'; import fs from 'node:fs'; import path from 'node:path';
const [url, out] = process.argv.slice(2), E = path.dirname(new URL(import.meta.url).pathname), prof = path.join(E, 'chrome-prof2');
const chrome = spawn('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', ['--headless=new', '--remote-debugging-port=9334', `--user-data-dir=${prof}`, '--no-first-run', '--hide-scrollbars', 'about:blank'], { stdio: 'ignore' });
const sleep = ms => new Promise(r => setTimeout(r, ms)); let ws;
try {
  let list; for (let i = 0; i < 50; i++) { try { list = await (await fetch('http://127.0.0.1:9334/json/list')).json(); if (list.length) break; } catch {} await sleep(200); }
  ws = new WebSocket(list.find(t => t.type === 'page').webSocketDebuggerUrl); await new Promise(r => ws.onopen = r);
  let id = 0; const wait = new Map(); ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id && wait.has(d.id)) { wait.get(d.id)(d.result); wait.delete(d.id); } };
  const send = (method, params = {}) => new Promise(r => { const i = ++id; wait.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 860, deviceScaleFactor: 1.5, mobile: false });
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
  await send('Page.enable'); await send('Page.navigate', { url }); await sleep(6000);
  await send('Runtime.evaluate', { expression: `(() => { const h = [...document.querySelectorAll('h2, summary, span')].find(e => e.textContent.trim().startsWith('Assets')); h?.scrollIntoView({ block: 'start' }); scrollBy(0, -330); })()` });
  await sleep(2500);
  const r = await send('Page.captureScreenshot', { format: 'png' }); fs.writeFileSync(out, Buffer.from(r.data, 'base64')); console.log(out);
} finally { ws?.close(); chrome.kill(); await sleep(500); fs.rmSync(prof, { recursive: true, force: true }); }
