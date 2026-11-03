// Run: npm test  (node:test, no extra deps)
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const http = require('http');
const { apply, readTranscript, ctxLeft, refresh, setPet } = require('../src/core/state');
const { sanitize, DEFAULTS } = require('../src/core/settings');
const hooks = require('../src/core/hooks');
const { startServer } = require('../src/core/server');

test('session lifecycle', () => {
  const m = new Map(), base = { session_id: 'abc', cwd: '/x/my-app' };
  apply(m, { ...base, hook_event_name: 'SessionStart', bobix_claude_pid: 42, bobix_host_session: 'local_abc-1' });
  assert.equal(m.get('abc').name, 'my-app');
  assert.equal(m.get('abc').claudePid, 42);
  assert.equal(m.get('abc').hostSession, 'local_abc-1');
  apply(m, { ...base, hook_event_name: 'PreToolUse', bobix_host_session: 'local_x; rm -rf /' });
  assert.equal(m.get('abc').hostSession, 'local_abc-1', 'malformed desktop id ignored');
  apply(m, { ...base, hook_event_name: 'UserPromptSubmit', prompt: 'Fix bug' });
  assert.equal(m.get('abc').state, 'working');
  apply(m, { ...base, hook_event_name: 'PreToolUse', tool_name: 'Bash', tool_input: { command: 'ls' } });
  assert.equal(m.get('abc').hits, 1);
  let r = apply(m, { ...base, hook_event_name: 'PermissionRequest', tool_name: 'Bash', tool_input: { command: 'rm x' } });
  assert.equal(r.notice.kind, 'waiting');
  r = apply(m, { ...base, hook_event_name: 'Notification', message: 'Claude needs your permission to use Bash' });
  assert.equal(r.notice, null, 'second permission signal must not notify twice');
  assert.equal(m.get('abc').askTool, 'Bash');
  apply(m, { ...base, hook_event_name: 'PostToolUse', tool_name: 'Read' });
  assert.equal(m.get('abc').state, 'waiting', 'another tool finishing does not clear the pending approval');
  apply(m, { ...base, hook_event_name: 'PostToolUse', tool_name: 'Bash' });
  assert.equal(m.get('abc').state, 'working');
  r = apply(m, { ...base, hook_event_name: 'Stop' });
  assert.equal(r.notice.kind, 'done'); assert.equal(m.get('abc').lv, 2);
  r = apply(m, { ...base, hook_event_name: 'Notification', message: 'Claude is waiting for your input' });
  assert.equal(m.get('abc').state, 'done'); assert.equal(r.notice, null);
  assert.ok(apply(m, { ...base, hook_event_name: 'SessionEnd' }).removed);
  assert.equal(m.size, 0);
  assert.equal(apply(m, { hook_event_name: 'Stop' }), null);
  assert.equal(apply(m, { session_id: '../etc', hook_event_name: 'Stop' }), null);
});

test('transcript: answer, usage, newest titles', () => {
  const f = path.join(os.tmpdir(), `bobix-${process.pid}.jsonl`);
  const lines = [
    { type: 'ai-title', aiTitle: 'Old AI title' },
    { type: 'custom-title', customTitle: 'Old name' },
    { type: 'assistant', message: { content: [{ type: 'text', text: 'Done: fixed it' }], usage: { input_tokens: 10, cache_read_input_tokens: 99990 } } },
    { type: 'custom-title', customTitle: 'Renamed' },
    { type: 'ai-title', aiTitle: 'New AI title' },
  ];
  fs.writeFileSync(f, lines.map(l => JSON.stringify(l)).join('\n'));
  const r = readTranscript(f);
  fs.unlinkSync(f);
  assert.equal(r.text, 'Done: fixed it');
  assert.equal(r.title, 'Renamed', 'custom title wins and the newest one is used');
  assert.equal(ctxLeft(r.usage), 50);
  assert.deepEqual(readTranscript('/nope/missing.jsonl'), { text: null, usage: null, title: null, interrupted: false });
});

test('an interrupted turn (Esc / denied permission) puts the pet to rest', () => {
  const f = path.join(os.tmpdir(), `bobix-int-${process.pid}.jsonl`);
  fs.writeFileSync(f, [
    { type: 'assistant', message: { content: [{ type: 'tool_use' }] } },
    { type: 'user', message: { content: [{ type: 'text', text: '[Request interrupted by user for tool use]' }] } },
  ].map(l => JSON.stringify(l)).join('\n'));
  const m = new Map();
  apply(m, { session_id: 'i1', hook_event_name: 'PermissionRequest', tool_name: 'Bash', transcript_path: f });
  assert.equal(m.get('i1').state, 'waiting');
  assert.equal(refresh(m.get('i1'), false), true);
  assert.equal(m.get('i1').state, 'idle');
  m.get('i1').state = 'working';
  assert.equal(refresh(m.get('i1'), false, Date.now() + 6000), false, 'an unchanged transcript is not read again');
  fs.unlinkSync(f);
});

test('a pet hidden by hand comes back when its session needs you', () => {
  const m = new Map();
  apply(m, { session_id: 'h1', hook_event_name: 'Stop' });
  apply(m, { session_id: 'h2', hook_event_name: 'Stop' });
  assert.equal(setPet(m, 'h1', 'hidden'), true);
  assert.equal(setPet(m, 'h1', 'bogus'), false);
  assert.equal(m.get('h1').pet, 'hidden');
  apply(m, { session_id: 'h1', hook_event_name: 'PreToolUse', tool_name: 'Read' });
  assert.equal(m.get('h1').pet, 'hidden', 'still working on the same turn: stays hidden');
  apply(m, { session_id: 'h1', hook_event_name: 'PermissionRequest', tool_name: 'Bash' });
  assert.equal(m.get('h1').pet, 'auto', 'needs approval: back on screen');
  setPet(m, '*', 'shown');
  assert.deepEqual([...m.values()].map(s => s.pet), ['shown', 'shown']);
  apply(m, { session_id: 'h2', hook_event_name: 'UserPromptSubmit', prompt: 'next' });
  assert.equal(m.get('h2').pet, 'auto');
});

test('the app holding a session is named the way people know it', () => {
  const { hostApp } = require('../src/core/focus');
  assert.equal(hostApp(['claude', 'bash', 'gnome-terminal-server', 'systemd']), 'Terminal');
  assert.equal(hostApp(['claude', 'zsh', 'tmux: server']), 'tmux');
  assert.equal(hostApp(['claude', 'node', 'code']), 'VS Code');
  assert.equal(hostApp(['claude', 'bash']), null);
});

test('process names are full names (Linux cuts ps comm to 15 characters)', async () => {
  const { processNames } = require('../src/core/focus');
  const [name] = await processNames([process.pid]);
  assert.match(name, /^node(\.exe)?$/i);
});

test('settings sanitize', () => {
  const s = sanitize({ size: 9999, edge: 'left', opacity: 0, zoneStart: 90, zoneEnd: 91, evil: 1, sound: 'yes' });
  assert.equal(s.size, 128); assert.equal(s.edge, DEFAULTS.edge); assert.equal(s.opacity, 0.2);
  assert.equal(s.zoneEnd, 95); assert.equal(s.evil, undefined); assert.equal(s.sound, DEFAULTS.sound);
  assert.equal(sanitize({ sleepHide: '60' }).sleepHide, 60); assert.equal(sanitize({ sleepHide: -5 }).sleepHide, 0);
});

test('hooks install keeps foreign hooks and is idempotent', () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-home-'));
  const file = hooks.settingsPath(home);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const foreign = { type: 'command', command: 'echo hi' };
  // Look-alikes that belong to someone else: must survive install AND uninstall.
  const fork = { type: 'command', command: 'node', args: [path.join(home, 'code', 'bobix-fork', 'lint.js')] };
  const otherCurl = { type: 'command', command: 'curl -s --data-binary @- http://127.0.0.1:9999/event' };
  const legacy = { type: 'command', command: 'curl -s -m 1 -o /dev/null -X POST -H "Content-Type: application/json" --data-binary @- http://127.0.0.1:4747/event 2>/dev/null || true' };
  fs.writeFileSync(file, JSON.stringify({ model: 'x', hooks: { Stop: [{ hooks: [foreign, fork, otherCurl] }, { hooks: [legacy] }] } }));
  const src = path.join(__dirname, '..', 'src', 'hook', 'hook.js');
  hooks.install({ port: 5000, nodePath: '/usr/bin/node', hookSource: src, home });
  hooks.install({ port: 5000, nodePath: '/usr/bin/node', hookSource: src, home });
  let s = JSON.parse(fs.readFileSync(file, 'utf8'));
  assert.equal(s.model, 'x');
  assert.deepEqual(s.hooks.Stop[0].hooks, [foreign, fork, otherCurl]);
  assert.equal(s.hooks.Stop.length, 2, 'legacy hook replaced, not duplicated');
  assert.ok(fs.existsSync(hooks.tokenPath(home)));
  assert.ok(fs.existsSync(hooks.hookTarget(home)));
  const st = hooks.status({ home });
  assert.ok(st.complete); assert.equal(st.mode, 'node'); assert.equal(st.port, 5000);
  hooks.uninstall({ home });
  s = JSON.parse(fs.readFileSync(file, 'utf8'));
  assert.deepEqual(s.hooks, { Stop: [{ hooks: [foreign, fork, otherCurl] }] });
  fs.writeFileSync(file, '{broken');
  assert.throws(() => hooks.install({ port: 5000, hookSource: src, home }));
  assert.equal(fs.readFileSync(file, 'utf8'), '{broken', 'unreadable settings are never overwritten');
  // odd shapes never crash status()
  fs.writeFileSync(file, JSON.stringify({ hooks: { Stop: { not: 'a list' }, PreToolUse: [null, { hooks: [null] }] } }));
  assert.doesNotThrow(() => hooks.status({ home }));
  assert.throws(() => hooks.install({ port: 5000, hookSource: src, home }), /not a list/);
  fs.rmSync(home, { recursive: true, force: true });
});

test('hooks install writes through a symlink and keeps file permissions', { skip: process.platform === 'win32' }, () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-home-'));
  const dot = path.join(home, 'dotfiles', 'claude-settings.json');
  fs.mkdirSync(path.dirname(dot), { recursive: true });
  fs.writeFileSync(dot, '{"env":{"SECRET":"x"}}', { mode: 0o600 });
  fs.mkdirSync(path.join(home, '.claude'));
  fs.symlinkSync(dot, hooks.settingsPath(home));
  hooks.install({ port: 5000, hookSource: path.join(__dirname, '..', 'src', 'hook', 'hook.js'), home });
  assert.ok(fs.lstatSync(hooks.settingsPath(home)).isSymbolicLink(), 'symlink kept');
  assert.equal(fs.statSync(dot).mode & 0o777, 0o600, 'permissions kept');
  assert.ok(JSON.parse(fs.readFileSync(dot, 'utf8')).hooks.Stop);
  fs.rmSync(home, { recursive: true, force: true });
});

test('server: header + host guard, events reach onEvent', async () => {
  const got = [];
  const token = 'a'.repeat(48);
  const srv = startServer({ port: 0, token, onEvent: e => got.push(e), snapshot: () => ({ sessions: [] }) });
  await srv.ready;
  const port = srv.server.address().port;
  const get = p => new Promise(res => http.get({ host: '127.0.0.1', port, path: p }, x => { res(x.statusCode); x.destroy(); }));
  assert.equal(await get('/events'), 403, 'state stream needs the token');
  assert.equal(await get('/events?t=' + token), 200);
  const post = headers => new Promise(res => {
    const r = http.request({ host: '127.0.0.1', port, path: '/event', method: 'POST', headers }, x => res(x.statusCode));
    r.end('{"session_id":"s1","hook_event_name":"Stop"}');
  });
  assert.equal(await post({ 'content-type': 'application/json' }), 403, 'no header → rejected');
  assert.equal(await post({ 'x-bobix': '1' }), 403, 'wrong token → rejected');
  assert.equal(await post({ 'x-bobix': token, host: 'evil.example:80' }), 403, 'foreign Host → rejected');
  assert.equal(await post({ 'x-bobix': token }), 204);
  await new Promise(r => setTimeout(r, 20));
  assert.equal(got.length, 1);
  await srv.close();
});

test('hook.js forwards a trimmed event', async () => {
  const got = [];
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-home-'));
  const token = hooks.ensureToken(home);
  const srv = startServer({ port: 0, token, onEvent: e => got.push(e), snapshot: () => ({}) });
  await srv.ready;
  const { execFile } = require('child_process');
  const env = { ...process.env, HOME: home, USERPROFILE: home, CLAUDE_PID: '4242', CLAUDE_CODE_HOST_SESSION_ID: 'local_test-1' };
  const ev = { session_id: 's2', hook_event_name: 'PreToolUse', tool_name: 'Write', tool_input: { file_path: '/a.txt', content: 'x'.repeat(5000) }, tool_response: 'big' };
  // async: a sync child would block this process, which is also the server
  const t0 = Date.now();
  await new Promise(res => execFile(process.execPath, [path.join(__dirname, '..', 'src', 'hook', 'hook.js'), String(srv.server.address().port)], { env }, res).stdin.end(JSON.stringify(ev)));
  assert.ok(Date.now() - t0 < 1000, 'hook exits right after posting');
  await srv.close();
  assert.equal(got.length, 1);
  assert.deepEqual(got[0].tool_input, { file_path: '/a.txt' });
  assert.equal(got[0].tool_response, undefined);
  assert.equal(got[0].bobix_claude_pid, 4242);
  assert.equal(got[0].bobix_host_session, 'local_test-1');
  fs.rmSync(home, { recursive: true, force: true });
});

test('macOS focus picks the outermost app that owns the window', () => {
  const { outerBundle } = require('../src/core/focus');
  // Claude desktop: the CLI is itself bundled as an .app inside Claude.app's support folder
  assert.equal(outerBundle([
    '/Users/x/Library/Application Support/Claude/claude-code/2.1/claude.app/Contents/MacOS/claude',
    '/Applications/Claude.app/Contents/Helpers/disclaimer',
    '/Applications/Claude.app/Contents/MacOS/Claude',
  ]), '/Applications/Claude.app');
  // VS Code terminal: shell under a helper bundle nested in the main app
  assert.equal(outerBundle(['/usr/local/bin/claude', '/bin/zsh', '/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper', '/Applications/Visual Studio Code.app/Contents/MacOS/Electron']), '/Applications/Visual Studio Code.app');
  assert.equal(outerBundle(['/usr/local/bin/claude', '/bin/zsh', '/usr/bin/login', '/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal']), '/System/Applications/Utilities/Terminal.app');
  assert.equal(outerBundle(['/usr/local/bin/claude', '/bin/zsh', '/opt/homebrew/bin/tmux']), null, 'tmux server: no window to bring forward');
});

test('upgrading from the old Session Guild name replaces its hooks and folder', () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-home-'));
  const file = hooks.settingsPath(home);
  const oldDir = path.join(home, '.claude', 'session-guild');
  fs.mkdirSync(oldDir, { recursive: true });
  fs.writeFileSync(path.join(oldDir, 'token'), 'old');
  const foreign = { type: 'command', command: 'echo hi' };
  const oldHook = { type: 'command', command: '/usr/bin/node', args: [path.join(oldDir, 'hook.js'), '4747'], async: true };
  fs.writeFileSync(file, JSON.stringify({ hooks: { Stop: [{ hooks: [foreign] }, { hooks: [oldHook] }], PreToolUse: [{ matcher: '*', hooks: [oldHook] }] } }));
  assert.equal(hooks.status({ home }).outdated, true, 'old install is detected');
  hooks.install({ port: 4747, nodePath: '/usr/bin/node', hookSource: path.join(__dirname, '..', 'src', 'hook', 'hook.js'), home });
  const s = JSON.parse(fs.readFileSync(file, 'utf8'));
  const all = JSON.stringify(s.hooks);
  assert.ok(!all.includes('session-guild'), 'no old hook left');
  assert.equal(s.hooks.PreToolUse.length, 1, 'replaced, not duplicated');
  assert.deepEqual(s.hooks.Stop[0].hooks, [foreign]);
  assert.equal(hooks.status({ home }).outdated, false);
  assert.ok(!fs.existsSync(oldDir), 'old folder cleaned up');
  fs.rmSync(home, { recursive: true, force: true });
});
