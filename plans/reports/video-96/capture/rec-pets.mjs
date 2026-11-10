// rec-pets.mjs OUT.mp4 SECONDS: record the scratch app's real pets page (http://127.0.0.1:4791/pets) in headless Chrome.
// Frames are taken back to back (Page.captureScreenshot, at the device scale) and kept with their times and encoded at 30 fps keeping those times.
// The token is read from the scratch home and never printed. Events are fired by world.sh on a fixed clock (EVENTS).
import { spawn, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const E = path.dirname(new URL(import.meta.url).pathname);
const [out, secs = '24'] = process.argv.slice(2);
const W = +(process.env.VW || 768), H = +(process.env.VH || 180), DPR = +(process.env.DPR || 2.5);
const EVENTS = JSON.parse(process.env.EVENTS || '[]'); // [[seconds, "world.sh args"], ...]
const token = fs.readFileSync(path.join(E, 'home/.claude/bobix/token'), 'utf8').trim();
const prof = path.join(E, 'chrome-prof'), frames = path.join(E, 'frames');
fs.rmSync(frames, { recursive: true, force: true }); fs.mkdirSync(frames, { recursive: true });

const chrome = spawn('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', ['--headless=new', '--remote-debugging-port=9333',
  `--user-data-dir=${prof}`, '--no-first-run', '--hide-scrollbars', '--mute-audio', `--window-size=${W},${H}`, 'about:blank'], { stdio: 'ignore' });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let ws;
try {
  let list;
  for (let i = 0; i < 50; i++) { try { list = await (await fetch('http://127.0.0.1:9333/json/list')).json(); if (list.length) break; } catch {} await sleep(200); }
  ws = new WebSocket(list.find(t => t.type === 'page').webSocketDebuggerUrl);
  await new Promise(r => ws.onopen = r);
  let id = 0; const wait = new Map(), shots = [];
  const send = (method, params = {}) => new Promise(r => { const i = ++id; wait.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  ws.onmessage = m => {
    const d = JSON.parse(m.data);
    if (d.id && wait.has(d.id)) { wait.get(d.id)(d.result); wait.delete(d.id); }
    if (d.method === 'Page.screencastFrame') {
      const f = path.join(frames, `f${String(shots.length).padStart(5, '0')}.jpg`);
      fs.writeFileSync(f, Buffer.from(d.params.data, 'base64')); shots.push([f, d.params.metadata.timestamp]);
      send('Page.screencastFrameAck', { sessionId: d.params.sessionId });
    }
    if (d.method === 'Runtime.exceptionThrown') console.log('page error:', d.params.exceptionDetails.text);
  };
  await send('Runtime.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: W, height: H, deviceScaleFactor: DPR, mobile: false });
  await send('Emulation.setDefaultBackgroundColorOverride', { color: { r: 246, g: 240, b: 230, a: 1 } });
  await send('Page.enable');
  await send('Page.navigate', { url: `http://127.0.0.1:4791/pets?t=${encodeURIComponent(token)}` });
  await sleep(2500);
  // Screencast frames come at CSS size; screenshots carry the device scale, so frames are taken one after another.
  let rec = true;
  const grab = (async () => { while (rec) { const r = await send('Page.captureScreenshot', { format: 'jpeg', quality: 92, optimizeForSpeed: true });
    const f = path.join(frames, `f${String(shots.length).padStart(5, '0')}.jpg`); fs.writeFileSync(f, Buffer.from(r.data, 'base64')); shots.push([f, Date.now() / 1000]); } })();
  const t0 = Date.now();
  for (const [s, args] of EVENTS) {
    await sleep(Math.max(0, s * 1000 - (Date.now() - t0)));
    execFileSync(path.join(E, 'world.sh'), args.split(' '), { stdio: 'ignore' });
    console.log(`${s}s ${args}`);
  }
  await sleep(Math.max(0, +secs * 1000 - (Date.now() - t0)));
  rec = false; await grab;
  await sleep(300);
  // ffconcat keeping each frame's own duration, then a constant 30 fps H.264.
  const lines = ['ffconcat version 1.0'];
  shots.forEach(([f, t], i) => { lines.push(`file '${f}'`); lines.push(`duration ${((shots[i + 1]?.[1] ?? t + 1 / 30) - t).toFixed(4)}`); });
  lines.push(`file '${shots.at(-1)[0]}'`);
  fs.writeFileSync(path.join(frames, 'list.txt'), lines.join('\n'));
  execFileSync('ffmpeg', ['-v', 'error', '-y', '-f', 'concat', '-safe', '0', '-i', path.join(frames, 'list.txt'), '-vf', 'fps=30,format=yuv420p,scale=trunc(iw/2)*2:trunc(ih/2)*2',
    '-c:v', 'libx264', '-crf', '16', '-movflags', '+faststart', out]);
  console.log(`${out}: ${shots.length} frames`);
} finally {
  ws?.close(); chrome.kill(); await sleep(500);
  fs.rmSync(prof, { recursive: true, force: true }); fs.rmSync(frames, { recursive: true, force: true });
}
