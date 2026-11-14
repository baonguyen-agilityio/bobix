// Headless Chrome over the DevTools protocol (Node's own WebSocket), shared by overlay.test.js and layout.test.js.
// A CI runner is slow and sometimes stalls, so nothing here waits without a limit and nothing is left behind:
//  - the start is tried up to START_TRIES times (each try waits START_WAIT_MS for DevToolsActivePort); a try that fails
//    kills its Chrome and removes its profile folder, and the final error says what each try saw (exit code, Chrome's stderr);
//  - every call has CALL_MS; one that gets no answer is sent once more (a stalled renderer often wakes up), then fails
//    with its name; every event wait has EVENT_MS;
//  - close() always ends Chrome and removes the profile, so a test's finally leaves no process holding the job open.
const fs = require('fs'), path = require('path'), os = require('os');
const { spawn } = require('child_process');

const START_TRIES = 3, START_WAIT_MS = 20e3, CALL_MS = 30e3, EVENT_MS = 60e3;
const sleep = ms => new Promise(r => setTimeout(r, ms));

function chrome() {
  const c = [process.env.CHROME_PATH,
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/google-chrome', '/usr/bin/google-chrome-stable', '/usr/bin/chromium', '/usr/bin/chromium-browser',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe', 'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe'];
  return c.find(p => p && fs.existsSync(p));
}

const rm = dir => { try { fs.rmSync(dir, { recursive: true, force: true }); } catch {} };
const within = (p, ms, msg) => { let t; return Promise.race([p, new Promise((_, no) => { t = setTimeout(() => no(new Error(msg)), ms); })]).finally(() => clearTimeout(t)); };

// One start: Chrome running and its page's DevTools address, or an Error saying why not (after cleaning up).
async function startOnce(exe, prefix, args) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix)), logFile = path.join(os.tmpdir(), path.basename(dir) + '.log'), log = fs.openSync(logFile, 'w');
  // These flags keep a background or covered window drawing and answering (Windows and Linux runners otherwise stall it).
  const proc = spawn(exe, ['--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-gpu', '--mute-audio',
    '--disable-background-timer-throttling', '--disable-renderer-backgrounding', '--disable-backgrounding-occluded-windows',
    '--disable-features=CalculateNativeWinOcclusion', '--disable-dev-shm-usage',
    '--remote-debugging-port=0', `--user-data-dir=${dir}`, '--force-device-scale-factor=1', ...args, 'about:blank'], { stdio: ['ignore', 'ignore', log] });
  let exited = null; proc.on('exit', (code, sig) => { exited = `exit code ${code ?? sig}`; }); proc.on('error', e => { exited = String(e); });
  const fail = why => {
    let err = ''; try { err = fs.readFileSync(logFile, 'utf8').trim().split('\n').slice(-5).join(' | '); } catch {}
    try { proc.kill(); } catch {} try { fs.closeSync(log); } catch {} rm(dir); rm(logFile);
    return new Error(`${why}${exited ? ` (${exited})` : ''}${err ? `, Chrome said: ${err.slice(0, 400)}` : ''}`);
  };
  let port;
  for (let t = Date.now(); Date.now() - t < START_WAIT_MS && !port && !exited; await sleep(50)) { try { port = fs.readFileSync(path.join(dir, 'DevToolsActivePort'), 'utf8').split('\n')[0]; } catch {} }
  if (!port) throw fail(`no DevToolsActivePort in ${START_WAIT_MS / 1000} s`);
  let page;
  for (let t = Date.now(); Date.now() - t < 10e3 && !page && !exited; await sleep(50)) {
    try { page = (await (await within(fetch(`http://127.0.0.1:${port}/json/list`), 3e3, 'json/list')).json()).find(t => t.type === 'page'); } catch {}
  }
  if (!page) throw fail('Chrome started but offers no page');
  return { proc, dir, log, logFile, page };
}

// Starts Chrome (args: window size, scrollbars ... of the test) and connects to its page.
async function launch(exe, prefix, args = []) {
  const errs = []; let s;
  for (let k = 1; k <= START_TRIES && !s; k++) {
    try { s = await startOnce(exe, prefix, args); } catch (e) { errs.push(`try ${k}: ${e.message}`); }
  }
  if (!s) throw new Error(`Chrome did not start in ${START_TRIES} tries. ${errs.join('. ')}`);
  const { proc, dir, log, logFile } = s;
  let done = false;
  const close = async () => {
    if (done) return; done = true;
    try { ws.close(); } catch {} try { proc.kill(); } catch {} try { fs.closeSync(log); } catch {}
    await sleep(200); rm(dir); rm(logFile); // the profile folder is released a moment after Chrome ends (Windows)
  };
  const ws = new WebSocket(s.page.webSocketDebuggerUrl), calls = new Map(), on = new Map(); let id = 0;
  try { await within(new Promise((ok, no) => { ws.onopen = ok; ws.onerror = () => no(new Error('DevTools socket error')); }), CALL_MS, 'Chrome: the DevTools socket did not open'); } catch (e) { await close(); throw e; }
  ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id) { const c = calls.get(d.id); calls.delete(d.id); if (c) d.error ? c[1](new Error(d.error.message)) : c[0](d.result); } else on.get(d.method)?.(d.params); };
  const call = (method, params) => new Promise((ok, no) => { const n = ++id; calls.set(n, [ok, no]); ws.send(JSON.stringify({ id: n, method, params })); });
  const send = async (method, params = {}) => {
    for (let k = 0; ; k++) {
      try { return await within(call(method, params), CALL_MS, `no answer to ${method} in ${CALL_MS / 1000} s`); }
      catch (e) { if (!/^no answer/.test(e.message) || k === 1) throw new Error(`Chrome: ${e.message}${k ? ' (after one retry)' : ''}`); }
    }
  };
  const once = method => within(new Promise(ok => on.set(method, p => { on.delete(method); ok(p); })), EVENT_MS, `Chrome: no ${method} event in ${EVENT_MS / 1000} s`);
  const errors = []; on.set('Runtime.exceptionThrown', p => errors.push(p.exceptionDetails.exception?.description || p.exceptionDetails.text));
  return { send, once, close, errors };
}

module.exports = { chrome, launch, sleep };
