#!/usr/bin/env node
// Claude Code hook for Bobix: reads the hook event (JSON on stdin) and posts a trimmed copy
// to the local app. No dependencies, never fails the hook. Usage: node hook.js <port>
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
  if (e.tool_input && typeof e.tool_input === 'object') {
    const t = {};
    for (const k of KEEP_INPUT) if (typeof e.tool_input[k] === 'string') t[k] = e.tool_input[k].slice(0, 300);
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
  const req = http.request({
    host: '127.0.0.1', port, path: '/event', method: 'POST', timeout: 1500, agent: false, // no keep-alive: exit right after
    headers: { 'content-type': 'application/json', 'x-bobix': token, 'content-length': Buffer.byteLength(body) },
  }, res => res.resume());
  req.on('error', () => {});
  req.on('timeout', () => req.destroy());
  req.end(body);
});
