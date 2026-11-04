// Local HTTP server: receives hook events, streams session state (SSE) and serves the UI pages.
// Bound to 127.0.0.1. Host is checked (DNS rebinding). Events and the state stream need the per-user token
// (x-bobix header / ?t=): other local users can't feed or read it, and a web page can't send the
// header cross-origin without a CORS preflight we never answer.
const http = require('http');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', 'ui');
const PAGES = { '/': 'panel.html', '/pets': 'pets.html' };
const TYPES = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.woff2': 'font/woff2' };
const ASSETS = new Set(['theme.css', 'creature.js', 'i18n.js']);
const FONT = /^fonts\/[a-z0-9-]+\.(woff2|css)$/;

function startServer({ port, token, onEvent, snapshot }) {
  const clients = new Set();
  const okHost = h => /^(127\.0\.0\.1|localhost)(:\d+)?$/.test(h || '');
  const okToken = t => typeof t === 'string' && t.length === token.length && crypto.timingSafeEqual(Buffer.from(t), Buffer.from(token));

  const server = http.createServer((req, res) => {
    if (!okHost(req.headers.host)) return res.writeHead(403).end();
    const url = req.url.split('?')[0];

    if (req.method === 'POST' && url === '/event') {
      if (!okToken(req.headers['x-bobix'])) return res.writeHead(403).end();
      let body = '', size = 0;
      req.setEncoding('utf8'); // decode across chunk boundaries (non-ASCII prompts)
      req.on('data', c => { size += c.length; if (size > 8e6) req.destroy(); else body += c; });
      // x-bobix-wait: the hook waits for an answer (PermissionRequest). onEvent returns a promise of the
      // hook output, or null for "no answer"; the request closing (Claude moved on) cancels the wait.
      const wait = req.headers['x-bobix-wait'] === '1';
      const gone = new AbortController();
      res.on('close', () => gone.abort());
      req.on('end', async () => {
        let out = null;
        try { out = onEvent(JSON.parse(body), wait ? gone.signal : null); } catch (err) { console.error('[bobix] bad event:', err.message); }
        if (!wait) return res.writeHead(204).end();
        try { out = await out; } catch { out = null; }
        if (res.writableEnded || gone.signal.aborted) return;
        out && typeof out === 'object' ? res.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify(out)) : res.writeHead(204).end();
      });
      return;
    }
    if (req.method !== 'GET') return res.writeHead(405).end();
    if (url === '/events') {
      if (!okToken(new URL(req.url, 'http://x').searchParams.get('t'))) return res.writeHead(403).end();
      res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', Connection: 'keep-alive' });
      res.write(`data: ${JSON.stringify(snapshot())}\n\n`);
      clients.add(res);
      req.on('close', () => clients.delete(res));
      return;
    }
    const rel = url.startsWith('/ui/') ? url.slice(4) : '';
    const file = PAGES[url] || (ASSETS.has(rel) || FONT.test(rel) ? rel : null);
    if (!file) return res.writeHead(404).end();
    const type = TYPES[path.extname(file)];
    res.writeHead(200, { 'Content-Type': type.startsWith('font/') ? type : `${type}; charset=utf-8`, 'Cache-Control': 'no-store' });
    fs.createReadStream(path.join(UI, file)).pipe(res);
  });

  const broadcast = payload => { const d = `data: ${JSON.stringify(payload)}\n\n`; for (const c of clients) c.write(d); };
  // Keep-alive comment so proxies / sleeping laptops don't silently drop the stream.
  const ping = setInterval(() => { for (const c of clients) c.write(': ping\n\n'); }, 25000);
  const ready = new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => resolve());
  });
  const close = () => { clearInterval(ping); for (const c of clients) c.end(); const done = new Promise(r => server.close(r)); server.closeAllConnections?.(); return done; };
  return { ready, broadcast, close, server };
}

module.exports = { startServer };
