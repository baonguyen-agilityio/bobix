// rec-pets.mjs OUT.mp4 SECONDS: record the scratch app's real pets page (http://127.0.0.1:4791/pets) in headless Chrome,
// frames taken back to back at the device scale and encoded at 30 fps keeping their times. The token is read from the
// scratch Bobix home and never printed. Events come from real Claude sessions driven meanwhile (world.sh type).
import { spawn, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const W = process.env.W, [out, secs = '60'] = process.argv.slice(2);
const VW = +(process.env.VW || 1280), VH = +(process.env.VH || 260), DPR = +(process.env.DPR || 1.5);
const token = fs.readFileSync(path.join(W, 'home/.claude/bobix/token'), 'utf8').trim();
const prof = path.join(W, 'chrome-prof'), frames = path.join(W, 'frames-' + path.basename(out));
fs.rmSync(frames, { recursive: true, force: true }); fs.mkdirSync(frames, { recursive: true });
const chrome = spawn('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', ['--headless=new', '--remote-debugging-port=9334',
  `--user-data-dir=${prof}`, '--no-first-run', '--hide-scrollbars', '--mute-audio', `--window-size=${VW},${VH}`, 'about:blank'], { stdio: 'ignore' });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let ws;
try {
  let list;
  for (let i = 0; i < 50; i++) { try { list = await (await fetch('http://127.0.0.1:9334/json/list')).json(); if (list.length) break; } catch {} await sleep(200); }
  ws = new WebSocket(list.find(t => t.type === 'page').webSocketDebuggerUrl);
  await new Promise(r => ws.onopen = r);
  let id = 0; const wait = new Map(), shots = [];
  const send = (method, params = {}) => new Promise(r => { const i = ++id; wait.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id && wait.has(d.id)) { wait.get(d.id)(d.result); wait.delete(d.id); } };
  await send('Emulation.setDeviceMetricsOverride', { width: VW, height: VH, deviceScaleFactor: DPR, mobile: false });
  await send('Emulation.setDefaultBackgroundColorOverride', { color: { r: 246, g: 240, b: 230, a: 1 } });
  await send('Page.enable');
  await send('Page.navigate', { url: `http://127.0.0.1:4791/pets?t=${encodeURIComponent(token)}` });
  await sleep(2000);
  // The settings the app gives its own pets window (size, label, pet mode, do not disturb): a plain tab has no bridge to ask.
  const cfg = fs.readFileSync(path.join(W, 'data/settings.json'), 'utf8');
  await send('Runtime.evaluate', { expression: `applySettings(${cfg})` });
  await sleep(1000);
  const t0 = Date.now();
  console.log('rec start', new Date().toISOString());
  while (Date.now() - t0 < +secs * 1000) {
    const r = await send('Page.captureScreenshot', { format: 'jpeg', quality: 85, optimizeForSpeed: true });
    const f = path.join(frames, `f${String(shots.length).padStart(5, '0')}.jpg`); fs.writeFileSync(f, Buffer.from(r.data, 'base64')); shots.push([f, Date.now() / 1000]);
  }
  const lines = ['ffconcat version 1.0'];
  shots.forEach(([f, t], i) => { lines.push(`file '${f}'`); lines.push(`duration ${((shots[i + 1]?.[1] ?? t + 1 / 30) - t).toFixed(4)}`); });
  lines.push(`file '${shots.at(-1)[0]}'`);
  fs.writeFileSync(path.join(frames, 'list.txt'), lines.join('\n'));
  execFileSync('ffmpeg', ['-v', 'error', '-y', '-f', 'concat', '-safe', '0', '-i', path.join(frames, 'list.txt'), '-vf', 'fps=30,format=yuv420p,scale=trunc(iw/2)*2:trunc(ih/2)*2',
    '-c:v', 'libx264', '-crf', '17', '-movflags', '+faststart', out]);
  console.log(`${out}: ${shots.length} frames in ${secs}s`);
} finally {
  ws?.close(); chrome.kill(); await sleep(500);
  fs.rmSync(prof, { recursive: true, force: true }); fs.rmSync(frames, { recursive: true, force: true });
}
