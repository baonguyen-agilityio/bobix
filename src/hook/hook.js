#!/usr/bin/env node
// Claude Code hook for Bobix: reads the hook event (JSON on stdin) and posts a trimmed copy
// to the local app. No dependencies, never fails the hook. Usage: node hook.js <port>
// On PermissionRequest it waits for Bobix's answer (Allow / Deny) and prints it for Claude Code;
// no answer (Bobix not running, turned off, or nobody clicked in time) prints nothing: Claude asks as usual.
const http = require('http');
const fs = require('fs');
const os = require('os');
const path = require('path');

const port = Number(process.argv[2]) || 4747;
const MAX_INPUT = 20000; // bytes of the serialized tool input the app gets; more is cut and marked (bobix_cut)
let token = '';
try { token = fs.readFileSync(path.join(__dirname, 'token'), 'utf8').trim(); } catch {}
if (!token) try { token = fs.readFileSync(path.join(os.homedir(), '.claude', 'bobix', 'token'), 'utf8').trim(); } catch {}

// The tool input as the app should judge it: whole while it serializes to MAX_INPUT bytes or less. Beyond that, keys
// are kept in order, the one that crosses the limit is cut at a character boundary, the rest dropped; cut says so.
function wholeInput(input) {
  const size = v => { try { const j = JSON.stringify(v); return j === undefined ? -1 : Buffer.byteLength(j); } catch { return -1; } };
  const total = size(input);
  if (total >= 0 && total <= MAX_INPUT) return { input, cut: false };
  const out = {};
  let left = MAX_INPUT - 2, cut = true; // 2: the braces
  for (const [k, v] of Object.entries(input)) {
    const head = Buffer.byteLength(JSON.stringify(k)) + 2; // key, ":" and ","
    const n = size(v);
    if (n < 0) continue; // cannot be serialized: dropped, and cut says so
    if (head + n <= left) { out[k] = v; left -= head + n; continue; }
    if (typeof v === 'string') { // as much of it as fits
      let lo = 0, hi = v.length;
      while (lo < hi) { const m = (lo + hi + 1) >> 1; if (head + size(v.slice(0, m)) <= left) lo = m; else hi = m - 1; }
      if (lo && /[\ud800-\udbff]/.test(v[lo - 1])) lo--; // not between the halves of one character
      out[k] = v.slice(0, lo);
    }
    break;
  }
  return { input: out, cut };
}

let raw = '';
process.stdin.on('data', c => { raw += c; });
process.stdin.on('end', () => {
  let e;
  try { e = JSON.parse(raw); } catch { return; }
  // Big payloads (file contents, tool output) are never needed: send only what the app reads.
  delete e.tool_response;
  const input = e.tool_input && typeof e.tool_input === 'object' ? e.tool_input : null;
  const plan = e.tool_name === 'ExitPlanMode'; // a plan Claude asks you to approve: Bobix shows it whole
  if (input) {
    const w = wholeInput(input);
    e.tool_input = w.input;
    if (w.cut) e.bobix_cut = true;
  }
  // Where the session lives, so a click on its pet can open it: Claude desktop's own session id,
  // else the claude process (the app walks up from it to the terminal / IDE window). Sent every time: it's free.
  e.bobix_claude_pid = Number(process.env.CLAUDE_PID) || process.ppid;
  const host = process.env.CLAUDE_CODE_HOST_SESSION_ID || '';
  if (/^local_[A-Za-z0-9-]{1,64}$/.test(host)) e.bobix_host_session = host;
  // For "Copy diagnostics": which CLAUDE* variables exist (names only) and the shape of the host id (letters → x).
  e.bobix_env = Object.keys(process.env).filter(k => /^CLAUDE/.test(k)).sort().slice(0, 40);
  e.bobix_host_shape = host ? host.replace(/[A-Za-z0-9]/g, 'x').slice(0, 80) : null;
  const body = JSON.stringify(e);
  // Headless runs (claude -p, Agent SDK apps) have nobody at Bobix to answer, or their own UI: don't hold them up.
  const ask = e.hook_event_name === 'PermissionRequest' && !/^sdk/.test(process.env.CLAUDE_CODE_ENTRYPOINT || '');
  // Bobix may answer with a title: for a session started from an issue (SessionStart), or by its first prompt, once that
  // names the issue a start in Work waits with (UserPromptSubmit)
  const titled = e.hook_event_name === 'SessionStart' || e.hook_event_name === 'UserPromptSubmit';
  const req = http.request({
    host: '127.0.0.1', port, path: '/event', method: 'POST', agent: false, // no keep-alive: exit right after
    timeout: ask ? 3000 : 1500, // until the app replies; a frozen app can't hold Claude up
    headers: { 'content-type': 'application/json', 'x-bobix': token, 'content-length': Buffer.byteLength(body), ...(ask && { 'x-bobix-wait': '1' }) },
  }, res => {
    // The app replies at once with how long it will wait for an answer ('forever': until answered); never wait longer.
    // A closed request tells the app that Claude moved on, so the wait ends with the hook process.
    const wait = res.headers['x-bobix-wait'], secs = Math.min(300, Number(wait) || 0);
    if (wait === 'forever') req.setTimeout(0);
    else if (secs) { req.setTimeout(0); setTimeout(() => req.destroy(), (secs + 3) * 1000).unref(); } // unref: exit as soon as the answer is in
    let out = '';
    res.setEncoding('utf8');
    res.on('data', c => { out += c; });
    res.on('end', () => {
      if (!(ask || titled) || res.statusCode !== 200) return;
      try {
        const d = JSON.parse(out).hookSpecificOutput;
        if (d?.hookEventName !== e.hook_event_name) return;
        // Claude approves a plan only with its input given back (a bare allow is ignored): the whole one, kept here.
        if (plan && input && d.decision?.behavior === 'allow' && !d.decision.updatedInput) d.decision.updatedInput = input;
        process.stdout.write(JSON.stringify({ hookSpecificOutput: d }));
      } catch {}
    });
  });
  req.on('error', () => {});
  req.on('timeout', () => req.destroy());
  req.end(body);
});
