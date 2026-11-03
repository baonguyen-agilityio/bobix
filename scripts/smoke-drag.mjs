// CI check (Linux, X11): drag a pet with the real mouse through xdotool and confirm it moved.
// Expects Bobix running with --remote-debugging-port=9333. Usage: node scripts/smoke-drag.mjs
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const hook = fileURLToPath(new URL('../src/hook/hook.js', import.meta.url));
const sleep = ms => new Promise(r => setTimeout(r, ms));
const xdo = (...a) => execFileSync('xdotool', a.map(String));
const send = (id, ev) => execFileSync(process.execPath, [hook, '4747'], { input: JSON.stringify({ session_id: id, hook_event_name: ev, cwd: `/tmp/${id}`, prompt: 'drag check' }) });

let target;
for (let i = 0; i < 60 && !target; i++) {
  try { target = (await (await fetch('http://127.0.0.1:9333/json')).json()).find(x => x.url.includes('/pets')); } catch {}
  if (!target) await sleep(1000);
}
if (!target) { console.log('[drag] no pets page'); process.exit(1); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener('open', r));
let n = 0;
const ev = expression => new Promise(res => {
  const id = ++n;
  const on = m => { const d = JSON.parse(m.data); if (d.id === id) { ws.removeEventListener('message', on); res(d.result?.result?.value); } };
  ws.addEventListener('message', on);
  ws.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, returnByValue: true } }));
});

send('drag-ci', 'UserPromptSubmit'); send('drag-ci', 'Stop');
await sleep(2500);
const at = await ev(`(() => { const p = pets.get('drag-ci'); if (!p) return null; const b = p.el.querySelector('.spr').getBoundingClientRect();
  return { x: Math.round(screenX + b.left + b.width / 2), y: Math.round(screenY + b.top + b.height / 2), petX: Math.round(p.x), w: innerWidth }; })()`);
console.log('[drag] pet at', JSON.stringify(at));
if (!at) process.exit(1);
const dir = at.petX > at.w / 2 ? -1 : 1; // drag toward the middle so the zone never clamps it
xdo('mousemove', at.x, at.y); await sleep(400);
xdo('mousedown', 1); await sleep(100);
for (let i = 1; i <= 15; i++) { xdo('mousemove', at.x + dir * i * 20, at.y); await sleep(40); }
xdo('mouseup', 1); await sleep(800);
const after = await ev(`Math.round(pets.get('drag-ci').x)`);
const moved = (after - at.petX) * dir;
console.log('[drag] pet x', at.petX, '->', after, `(moved ${moved} px)`);
send('drag-ci', 'SessionEnd');
ws.close();
process.exit(moved > 200 ? 0 : 1);
