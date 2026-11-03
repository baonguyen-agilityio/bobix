// Session state machine fed by Claude Code hook events. apply() is pure so it can be unit-tested.
const fs = require('fs');
const path = require('path');

const ID_RE = /^[\w-]{1,128}$/;
const short = (s, n = 80) => { s = String(s ?? '').replace(/\s+/g, ' ').trim(); return s.length > n ? s.slice(0, n - 1) + '…' : s; };
const toolArg = (input = {}) => short(input.file_path || input.command || input.pattern || input.url || input.query || input.description || '', 60);

function newSession(id, now) {
  return {
    id, name: '?', cwd: '', title: '', transcript: '', state: 'idle', since: now, updated: now,
    lv: 1, quest: '', tool: '', toolArg: '', askTool: '', ask: '', summary: '', hits: 0, ctx: null, claudePid: null, hostSession: null, log: [],
  };
}

// Returns { s, notice } | { removed } | null. notice = { kind: 'waiting' | 'done', id } for things the user should hear about.
function apply(sessions, e, now = Date.now()) {
  const id = e && e.session_id;
  if (typeof id !== 'string' || !ID_RE.test(id)) return null;
  if (e.hook_event_name === 'SessionEnd') return sessions.delete(id) ? { removed: id } : null;

  let s = sessions.get(id);
  if (!s) { s = newSession(id, now); sessions.set(id, s); }
  s.updated = now;
  if (typeof e.cwd === 'string' && e.cwd) { s.cwd = e.cwd; s.name = path.basename(e.cwd) || e.cwd; }
  if (typeof e.transcript_path === 'string' && e.transcript_path.endsWith('.jsonl')) s.transcript = e.transcript_path;
  if (Number.isInteger(e.bobix_claude_pid) && e.bobix_claude_pid > 1) s.claudePid = e.bobix_claude_pid;
  if (typeof e.bobix_host_session === 'string' && /^local_[A-Za-z0-9-]{1,64}$/.test(e.bobix_host_session)) s.hostSession = e.bobix_host_session;

  const log = (kind, text = '', tool = '') => { s.log.push({ t: now, kind, text, tool }); if (s.log.length > 40) s.log.shift(); };
  const set = st => { if (s.state !== st) { s.state = st; s.since = now; } };
  const askFor = (tool, text) => {
    const first = s.state !== 'waiting';
    s.askTool = tool || s.askTool; s.ask = short(text, 160);
    log('ask', s.ask, s.askTool);
    set('waiting');
    return first ? { kind: 'waiting', id } : null; // Notification + PermissionRequest both fire: notify once
  };
  let notice = null;

  switch (e.hook_event_name) {
    case 'SessionStart':
      log('start', e.source || 'startup');
      break;
    case 'UserPromptSubmit':
      s.quest = short(e.prompt, 240); s.hits = 0; s.summary = ''; s.ask = ''; s.askTool = ''; s.tool = ''; s.toolArg = '';
      log('prompt', short(e.prompt, 120));
      set('working');
      break;
    case 'PreToolUse':
      s.tool = String(e.tool_name || ''); s.toolArg = toolArg(e.tool_input); s.hits++;
      if (s.state === 'waiting') { s.ask = ''; s.askTool = ''; }
      log('tool', s.toolArg, s.tool);
      set('working');
      break;
    case 'PostToolUse':
      // Only the tool that asked clears "waiting": parallel tools / subagents finish while it still waits.
      if (s.state === 'waiting' && (!s.askTool || e.tool_name === s.askTool)) { s.ask = ''; s.askTool = ''; set('working'); }
      break;
    case 'PermissionRequest':
      notice = askFor(String(e.tool_name || ''), `${e.tool_name || ''} ${toolArg(e.tool_input)}`.trim());
      break;
    case 'Notification': {
      // "Claude needs your permission to use X" → waiting. "waiting for your input" (after Stop) → ignore.
      const m = String(e.message || '');
      if (/permission/i.test(m)) notice = askFor((m.match(/use (\S+)/i) || [])[1], m);
      break;
    }
    case 'Stop':
      s.lv++; s.ask = ''; s.askTool = '';
      log('done');
      set('done');
      notice = { kind: 'done', id };
      break;
  }
  return { s, notice };
}

// Reads the transcript's tail (last answer + token usage) and titles. Titles can sit far from the end, so the head is scanned too.
// interrupted: the latest turn was cut short (Esc, or a denied permission). Claude runs no Stop hook then.
function readTranscript(file) {
  const out = { text: null, usage: null, title: null, interrupted: false };
  let fd;
  try {
    if (!fs.statSync(file).isFile()) return out; // never open a FIFO / device: that would block
    fd = fs.openSync(file, 'r');
    const size = fs.fstatSync(fd).size;
    const chunk = (pos, len) => { const b = Buffer.alloc(len); fs.readSync(fd, b, 0, len, pos); return b.toString('utf8').split('\n'); };
    let custom = null, ai = null;
    // keepFirst: scanning backwards, the first title met is the newest one.
    const titles = (j, keepFirst) => {
      if (j.type === 'custom-title' && j.customTitle && !(keepFirst && custom)) custom = j.customTitle;
      if (j.type === 'ai-title' && j.aiTitle && !(keepFirst && ai)) ai = j.aiTitle;
    };
    const tail = chunk(Math.max(0, size - 512 * 1024), Math.min(size, 512 * 1024));
    let lastTurnSeen = false;
    for (let i = tail.length - 1; i >= 0; i--) {
      let j; try { j = JSON.parse(tail[i]); } catch { continue; }
      titles(j, true);
      if (!lastTurnSeen && (j.type === 'user' || j.type === 'assistant') && !j.isSidechain) {
        lastTurnSeen = true;
        const c = j.message?.content, txt = typeof c === 'string' ? c : Array.isArray(c) ? c.map(x => x.text || '').join(' ') : '';
        out.interrupted = j.type === 'user' && txt.includes('[Request interrupted by user');
      }
      if (j.type !== 'assistant' || !j.message || j.isSidechain) continue;
      out.usage ??= j.message.usage || null;
      if (!out.text && Array.isArray(j.message.content)) {
        const t = j.message.content.filter(c => c.type === 'text').map(c => c.text).join('\n').trim();
        if (t) out.text = t;
      }
    }
    if (!custom && !ai && size > 512 * 1024) for (const l of chunk(0, Math.min(size, 256 * 1024))) { try { titles(JSON.parse(l)); } catch {} }
    out.title = custom || ai;
  } catch { /* transcript not there yet */ } finally { if (fd !== undefined) fs.closeSync(fd); }
  return out;
}

function ctxLeft(u, forcedWindow) {
  if (!u) return null;
  const used = (u.input_tokens || 0) + (u.cache_read_input_tokens || 0) + (u.cache_creation_input_tokens || 0) + (u.output_tokens || 0);
  // ponytail: window guessed from usage (200k, else 1M); pass forcedWindow when known.
  const win = forcedWindow || (used > 200000 ? 1000000 : 200000);
  return Math.max(0, Math.round(100 - used / win * 100));
}

// Refresh title/context (throttled) and the final answer (on Stop) from the transcript.
function refresh(s, withSummary, now = Date.now(), forcedWindow) {
  if (!s.transcript || (!withSummary && now - (s.readAt || 0) < 5000)) return;
  s.readAt = now;
  const r = readTranscript(s.transcript);
  if (r.usage) s.ctx = ctxLeft(r.usage, forcedWindow);
  if (r.title) s.title = short(r.title, 80);
  if (withSummary && r.text) s.summary = short(r.text, 600);
  if (r.interrupted && (s.state === 'working' || s.state === 'waiting')) {
    s.ask = ''; s.askTool = ''; s.state = 'idle'; s.since = now;
    return true; // state changed
  }
  return false;
}

// What the outside world sees (no internal bookkeeping fields).
const publicView = s => { const { readAt, ...rest } = s; return rest; };

module.exports = { apply, readTranscript, ctxLeft, refresh, short, publicView, ID_RE };
