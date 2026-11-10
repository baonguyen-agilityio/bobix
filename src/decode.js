// Pictures nativeImage cannot read (WebP, SVG) are drawn by a hidden page instead. The page loads the picture as an
// image, the way a browser treats an <img>: an SVG then runs no script and loads nothing outside the file. The page
// also sits in its own in-memory session that refuses every request, permission, new window and navigation.
const { BrowserWindow, session } = require('electron');

let ses = null;
function decodeSession() {
  if (ses) return ses;
  ses = session.fromPartition('bobix-decode'); // no "persist:": nothing is written to disk
  ses.webRequest.onBeforeRequest((d, cb) => cb({ cancel: !/^(data|about):/.test(d.url) }));
  ses.setPermissionRequestHandler((wc, perm, cb) => cb(false));
  ses.setPermissionCheckHandler(() => false);
  return ses;
}

// Draws `buf` (a `mime` picture) and returns its pixels as straight RGBA: at `size` ({ width, height }) when given,
// else at its own size shrunk to no more than `side` on the long side. Rejects a picture the page cannot draw, or
// one still drawing after `waitMs`.
async function decodeInPage(buf, mime, { side, size, waitMs }) {
  const win = new BrowserWindow({ show: false, width: 1, height: 1, webPreferences: { sandbox: true, contextIsolation: true, session: decodeSession() } });
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  let timer;
  try {
    await win.loadURL('about:blank');
    win.webContents.on('will-navigate', e => e.preventDefault()); // after the blank page: before, it stops that load too
    const r = await Promise.race([win.webContents.executeJavaScript(`(async () => {
      const img = new Image();
      img.src = 'data:${mime};base64,${buf.toString('base64')}';
      await img.decode();
      const size = ${JSON.stringify(size || null)}, k = Math.min(1, ${side} / Math.max(img.naturalWidth, img.naturalHeight));
      const w = size ? size.width : Math.max(1, Math.round(img.naturalWidth * k)), h = size ? size.height : Math.max(1, Math.round(img.naturalHeight * k));
      const c = new OffscreenCanvas(w, h), g = c.getContext('2d');
      g.drawImage(img, 0, 0, w, h);
      const d = g.getImageData(0, 0, w, h).data;
      let s = '';
      for (let i = 0; i < d.length; i += 0x8000) s += String.fromCharCode(...d.subarray(i, i + 0x8000));
      return [w, h, btoa(s)];
    })()`), new Promise((_, no) => { timer = setTimeout(() => no(new Error(`${mime} decode timed out`)), waitMs); })]);
    return { kind: 'rgba', width: r[0], height: r[1], data: Buffer.from(r[2], 'base64') };
  } finally { clearTimeout(timer); win.destroy(); }
}

module.exports = { decodeInPage, decodeSession };
