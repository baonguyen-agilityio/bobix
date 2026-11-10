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
const KEEP_INPUT = ['file_path', 'command', 'pattern', 'url', 'query', 'description'];
let token = '';
try { token = fs.readFileSync(path.join(__dirname, 'token'), 'utf8').trim(); } catch {}
if (!token) try { token = fs.readFileSync(path.join(os.homedir(), '.claude', 'bobix', 'token'), 'utf8').trim(); } catch {}

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
    const t = {};
    for (const k of KEEP_INPUT) if (typeof input[k] === 'string') t[k] = input[k].slice(0, 300);
    if (plan && typeof input.plan === 'string') t.plan = input.plan.slice(0, 20000);
    e.tool_input = t;
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
  const titled = e.hook_event_name === 'SessionStart'; // Bobix may answer with a title for a session started from an issue
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
