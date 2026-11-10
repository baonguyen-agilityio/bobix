// Run by core.test.js with electron: draws the SVG fixtures (and a WebP) with src/decode.js the way main.js does and
// prints the pixels it needs, plus every request that reached a local server the fixtures point at.
const { app, BrowserWindow } = require('electron');
const fs = require('fs');
const http = require('http');
const path = require('path');
const { decodeInPage, decodeSession } = require('../src/decode');
const { drawSize } = require('../src/core/svg');

app.disableHardwareAcceleration();
app.on('window-all-closed', () => {}); // as main.js does: each decode closes its window, the run goes on
app.whenReady().then(async () => {
  const hits = [];
  const srv = http.createServer((req, res) => { hits.push(req.url); res.end(); }).listen(0, '127.0.0.1');
  await new Promise(r => srv.on('listening', r));
  const origin = `http://127.0.0.1:${srv.address().port}`;
  const fixture = name => Buffer.from(fs.readFileSync(path.join(__dirname, 'fixtures', name), 'utf8').replaceAll('ORIGIN', origin));
  const px = (img, x, y) => [...img.data.subarray((y * img.width + x) * 4, (y * img.width + x) * 4 + 4)];
  const draw = async buf => decodeInPage(buf, 'image/svg+xml', { size: drawSize(buf, 64), waitMs: 10e3 });
  const out = {};
  try {
    const v = await draw(fixture('viewbox.svg'));
    out.viewbox = { width: v.width, height: v.height, left: px(v, 2, 16), right: px(v, 61, 16) };
    const s = await draw(fixture('script.svg'));
    out.script = { width: s.width, height: s.height, middle: px(s, 32, 32) };
    try { await draw(Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><rect')); out.broken = 'drawn'; } catch { out.broken = 'rejected'; }
    // the session itself refuses requests, even ones a page makes on purpose
    const win = new BrowserWindow({ show: false, webPreferences: { sandbox: true, contextIsolation: true, session: decodeSession() } });
    await win.loadURL('about:blank');
    // WebP goes through the same page: a 100x50 one made there, half red, half blue, drawn back at no more than 64
    const webp = Buffer.from(await win.webContents.executeJavaScript(`(async () => {
      const c = new OffscreenCanvas(100, 50), g = c.getContext('2d');
      g.fillStyle = '#f00'; g.fillRect(0, 0, 50, 50); g.fillStyle = '#00f'; g.fillRect(50, 0, 50, 50);
      return [...new Uint8Array(await (await c.convertToBlob({ type: 'image/webp', quality: 1 })).arrayBuffer())];
    })()`));
    const w = await decodeInPage(webp, 'image/webp', { side: 64, waitMs: 10e3 });
    out.webp = { width: w.width, height: w.height, left: px(w, 4, 16).map(n => Math.round(n / 16)), right: px(w, 60, 16).map(n => Math.round(n / 16)) };
    out.fetch = await win.webContents.executeJavaScript(`fetch('${origin}/control').then(() => 'reached', () => 'refused')`);
    win.destroy();
    await new Promise(r => setTimeout(r, 300)); // late requests, if any, still land in `hits`
  } catch (err) { out.error = String(err?.message || err); }
  out.hits = hits;
  process.stdout.write(`RESULT ${JSON.stringify(out)}\n`);
  srv.close();
  app.exit(0);
});
