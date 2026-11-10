// cmd-server.mjs: a local queue of page scripts for the evidence build's windows (port 4799, 127.0.0.1 only).
// `node cmd-server.mjs` serves; the pages poll GET /next?w=pets|panel and run what they get, as a person's clicks would.
// `node cmd-server.mjs push pets 'code'` queues one.
import http from 'node:http';
const [, , op, w, code] = process.argv;
if (op === 'push') {
  const r = await fetch(`http://127.0.0.1:4799/push?w=${w}`, { method: 'POST', body: code });
  console.log(r.status === 204 ? 'queued' : r.status); process.exit(0);
}
const q = { pets: [], panel: [] }, out = [];
http.createServer((req, res) => {
  const u = new URL(req.url, 'http://x'), win = u.searchParams.get('w');
  res.setHeader('access-control-allow-origin', '*');
  if (u.pathname === '/push') { let b = ''; req.on('data', c => b += c); req.on('end', () => { q[win]?.push(b); res.writeHead(204).end(); }); return; }
  if (u.pathname === '/next') { const c = q[win]?.shift(); return c ? res.writeHead(200, { 'content-type': 'text/plain' }).end(c) : res.writeHead(204).end(); }
  if (u.pathname === '/log') { let b = ''; req.on('data', c => b += c); req.on('end', () => { console.log(win, b); res.writeHead(204).end(); }); return; }
  res.writeHead(404).end();
}).listen(4799, '127.0.0.1', () => console.log('cmd server on 4799'));
