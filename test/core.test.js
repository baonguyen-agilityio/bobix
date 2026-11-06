// Run: npm test  (node:test, no extra deps)
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const http = require('http');
const { apply, readTranscript, ctxLeft, refresh, setPet, plain } = require('../src/core/state');
const { sanitize, DEFAULTS } = require('../src/core/settings');
const hooks = require('../src/core/hooks');
const { startServer } = require('../src/core/server');
const markdown = require('../src/ui/markdown');
const LOG = require('../src/ui/log');

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

test('messages Claude Code sends through UserPromptSubmit are not new quests', () => {
  const m = new Map(), p = prompt => apply(m, { session_id: 'q1', hook_event_name: 'UserPromptSubmit', prompt });
  p('Fix the login bug');
  apply(m, { session_id: 'q1', hook_event_name: 'Stop' });
  const s = m.get('q1');
  s.pet = 'hidden';
  p('<task-notification>\n<task-id>abc</task-id>\n<tool-use-id>x</tool-use-id>\n<summary>Background command "Run tests" completed</summary>\n</task-notification>');
  assert.equal(s.quest, 'Fix the login bug', 'the quest stays');
  assert.equal(s.pet, 'hidden', 'a hidden pet stays hidden');
  assert.equal(s.state, 'working');
  assert.deepEqual([s.log.at(-1).kind, s.log.at(-1).text], ['system', 'Background command "Run tests" completed']);
  p('<system-reminder>\nsomething long\n</system-reminder>');
  assert.deepEqual([s.log.at(-1).kind, s.log.at(-1).text], ['system', 'system reminder']);
  p('<div>why is this HTML broken?</div>');
  assert.equal(s.quest, '<div>why is this HTML broken?</div>', 'a prompt that starts with other tags is still the user\'s');
  assert.equal(s.pet, 'auto');
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

test('the final report keeps its lines; the one-line summary stays short', () => {
  const f = path.join(os.tmpdir(), `bobix-rep-${process.pid}.jsonl`);
  const text = `## Done\n\n- one\n- two\n\n\n\n${'x'.repeat(40000)}`;
  fs.writeFileSync(f, JSON.stringify({ type: 'assistant', message: { content: [{ type: 'text', text }] } }));
  const m = new Map();
  apply(m, { session_id: 'r1', hook_event_name: 'UserPromptSubmit', prompt: 'go', transcript_path: f });
  refresh(m.get('r1'), true);
  const s = m.get('r1');
  assert.ok(s.report.startsWith('## Done\n\n- one\n- two\n\nxxx'), 'lines kept, blank-line runs cut to one');
  assert.ok(s.report.length <= 32000 && s.report.length > 31000);
  assert.ok(s.report.endsWith('x\n\n[…]'), 'the cut is marked on its own line');
  assert.equal(s.summary.length, 600);
  assert.ok(!s.summary.includes('\n'));
  assert.ok(s.summary.startsWith('Done one two xxx'), 'previews carry no Markdown');
  apply(m, { session_id: 'r1', hook_event_name: 'UserPromptSubmit', prompt: 'again' });
  assert.equal(s.report, '', 'a new prompt clears the old report');
  fs.unlinkSync(f);
});

test('plain: Markdown markers go, the words stay', () => {
  assert.equal(plain('## Done\n- **Fix** `a | b` in [docs](https://x.y)\n1. one\n---\n| A | B |\n|---|---|\n| 1 | 2 |\n```bash\nnpm test\n```').replace(/\s+/g, ' ').trim(),
    'Done Fix a | b in docs one A · B 1 · 2 npm test');
  assert.equal(plain('![logo](a.png) *one* _two_ __three__ ~~four~~ snake_case_name 2 * 3 * 4'), 'logo one two three four snake_case_name 2 * 3 * 4');
});

test('markdown: the report as safe HTML', () => {
  assert.equal(markdown('Hi <b>"x"</b> & `<i>` **bold** [docs](https://x.y)'),
    '<p>Hi &lt;b&gt;&quot;x&quot;&lt;/b&gt; &amp; <code>&lt;i&gt;</code> <strong>bold</strong> docs</p>');
  assert.equal(markdown('## Title\nline one\nline two'), '<h4>Title</h4><p>line one<br>line two</p>');
  assert.equal(markdown('- a\n  more\n- b\n\n3. c\n4. d'), '<ul><li>a more</li><li>b</li></ul><ol start="3"><li>c</li><li>d</li></ol>');
  assert.equal(markdown('| A | B |\n|---|---|\n| `1` | 2 |'),
    '<table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td><code>1</code></td><td>2</td></tr></tbody></table>');
  assert.equal(markdown('```bash\nnpm test && echo "<ok>"\n```\n---'), '<pre><code>npm test &amp;&amp; echo &quot;&lt;ok&gt;&quot;</code></pre><hr>');
  assert.equal(markdown('```\ncut off'), '<pre><code>cut off</code></pre>', 'an unclosed fence (report cut) still ends');
  assert.equal(markdown(''), '');
});

test('the whole prompt is kept for the details; quest and log lines stay short', () => {
  const m = new Map(), prompt = `Line one\n\nLine two ${'p'.repeat(40000)}`;
  apply(m, { session_id: 'p1', hook_event_name: 'UserPromptSubmit', prompt });
  const s = m.get('p1');
  assert.ok(s.prompt.startsWith('Line one\n\nLine two'), 'lines kept');
  assert.ok(s.prompt.length <= 32000 && s.prompt.endsWith('p\n\n[…]'));
  assert.equal(s.quest.length, 240);
  assert.ok(!s.quest.includes('\n'));
  assert.equal(s.log.at(-1).text.length, 240);
  apply(m, { session_id: 'p1', hook_event_name: 'UserPromptSubmit', prompt: '<task-notification> done' });
  assert.ok(s.prompt.startsWith('Line one'), 'a turn Claude Code starts itself keeps the prompt too');
});

test('a pasted report of 20k characters is the whole quest; a longer one is cut at a line, its code block closed', () => {
  const m = new Map(), row = i => `| ${i} | \`npm test\` | Mở file số ${i} |`;
  const prompt = ['## Báo cáo', '', '| # | Lệnh | Việc |', '|---|---|---|', ...Array.from({ length: 600 }, (_, i) => row(i)), '', '1. Mở file cuối'].join('\n');
  assert.ok(prompt.length > 20000);
  apply(m, { session_id: 'q1', hook_event_name: 'UserPromptSubmit', prompt });
  assert.equal(m.get('q1').prompt, prompt);
  apply(m, { session_id: 'q1', hook_event_name: 'UserPromptSubmit', prompt: `Run:\n\`\`\`bash\n${'echo hi\n'.repeat(5000)}\`\`\`` });
  const cut = m.get('q1').prompt;
  assert.ok(cut.length <= 32000);
  assert.ok(cut.endsWith('echo hi\n```\n\n[…]'), 'whole lines, the fence closed, then the mark');
  assert.ok(markdown(cut).endsWith('echo hi</code></pre><p>[…]</p>'), 'the mark shows below the code, not inside it');
});

test('the quest line and the log show a Markdown prompt as plain words', () => {
  const m = new Map();
  apply(m, { session_id: 'q2', hook_event_name: 'UserPromptSubmit', prompt: '## Việc cần làm\n\n| A | B |\n|:--|--:|\n| **x** | `y \\| z` |\n\n1. Mở [docs](https://x.y)\n   - *nhanh*' });
  const s = m.get('q2');
  assert.equal(s.quest, 'Việc cần làm A · B x · y | z Mở docs nhanh');
  assert.equal(s.log.at(-1).text, s.quest);
});

test('markdown: nested lists, tables with pipes in cells, quotes, italic', () => {
  assert.equal(markdown('1. a\n   - x\n     - deep\n   - y\n2. b\n\n   more b\n3. c'),
    '<ol><li>a<ul><li>x<ul><li>deep</li></ul></li><li>y</li></ul></li><li>b more b</li><li>c</li></ol>', 'a blank line keeps the list');
  assert.equal(markdown('- [ ] todo\n- [x] done'), '<ul><li>☐ todo</li><li>☑ done</li></ul>');
  assert.equal(markdown('| Cmd | Note |\n|:--|--:|\n| `a | b` | x \\| y |'),
    '<table><thead><tr><th>Cmd</th><th>Note</th></tr></thead><tbody><tr><td><code>a | b</code></td><td>x | y</td></tr></tbody></table>');
  assert.equal(markdown('> **Note**\n> > nested\n>>>> deep'), '<blockquote><p><strong>Note</strong><br>nested<br>deep</p></blockquote>');
  assert.equal(markdown('*one* _two_ 2 * 3 * 4 snake_case_name __init__ ![logo](a.png) [w](https://x.y/a_(b))'),
    '<p><em>one</em> <em>two</em> 2 * 3 * 4 snake_case_name __init__ logo w</p>');
  assert.equal(markdown('## Plan ##\n  ~~~js\n  a();\n    b();\n  ~~~'), '<h4>Plan</h4><pre><code>a();\n  b();</code></pre>');
});

test('markdown: HTML and links in a prompt stay inert text', () => {
  const tags = /^<\/?(p|br|h4|ul|ol|li|table|thead|tbody|tr|th|td|pre|code|strong|em|blockquote|hr)( start="\d+")?>$/;
  const cases = {
    '<script>alert(1)</script>': '<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>',
    '<img src=x onerror="alert(1)">': '<p>&lt;img src=x onerror=&quot;alert(1)&quot;&gt;</p>',
    '[click](javascript:alert(1)) ![x](javascript:alert(1))': '<p>click x</p>',
    '| a" onmouseover="alert(1) | `<b>` |\n|---|---|\n| \'><svg onload=alert(1)> | x |': '<table><thead><tr><th>a&quot; onmouseover=&quot;alert(1)</th><th><code>&lt;b&gt;</code></th></tr></thead>'
      + '<tbody><tr><td>&#39;&gt;&lt;svg onload=alert(1)&gt;</td><td>x</td></tr></tbody></table>',
    '# <i>"x"</i>\n> <a href="javascript:x">y</a>\n1) **<u>**': '<h4>&lt;i&gt;&quot;x&quot;&lt;/i&gt;</h4><blockquote><p>&lt;a href=&quot;javascript:x&quot;&gt;y&lt;/a&gt;</p></blockquote><ol><li><strong>&lt;u&gt;</strong></li></ol>',
    '```\n</code></pre><script>x</script>\n```': '<pre><code>&lt;/code&gt;&lt;/pre&gt;&lt;script&gt;x&lt;/script&gt;</code></pre>',
  };
  for (const [md, html] of Object.entries(cases)) {
    const out = markdown(md);
    assert.equal(out, html, md);
    assert.ok(out.match(/<[^>]*>/g).every(t => tags.test(t)), `only plain tags: ${md}`);
  }
});

test('log: newest first, repeats folded, long arguments keep their end', () => {
  const m = new Map(), long = `node /tmp/${'d'.repeat(200)}/run.js`;
  apply(m, { session_id: 'l1', hook_event_name: 'UserPromptSubmit', prompt: 'go' });
  for (const tool_name of ['mcp__b__batch', 'mcp__b__batch', 'mcp__b__batch', 'Read']) apply(m, { session_id: 'l1', hook_event_name: 'PreToolUse', tool_name });
  apply(m, { session_id: 'l1', hook_event_name: 'PreToolUse', tool_name: 'Bash', tool_input: { command: long } });
  const s = m.get('l1');
  assert.equal(s.log.at(-1).text, long, 'the log keeps the whole argument');
  assert.equal(s.toolArg.length, 60, 'the bubble keeps a short one');
  assert.deepEqual(LOG.fold(s.log).map(l => [l.kind, l.tool, l.n]),
    [['tool', 'Bash', 1], ['tool', 'Read', 1], ['tool', 'mcp__b__batch', 3], ['prompt', '', 1]]);
  assert.deepEqual(LOG.fold([{ kind: 'tool', tool: 'Read', text: 'a' }, { kind: 'tool', tool: 'Read', text: 'b' }]).map(l => l.n), [1, 1], 'other files are other lines');
  const run = [1, 2, 3].map(t => ({ t, kind: 'tool', tool: 'Bash', text: 'npm test' }));
  assert.deepEqual(LOG.fold(run).map(l => [l.t, l.from, l.n]), [[3, 1, 3]], 'shown at the newest repeat, keyed by the first');
  const short = LOG.mid(long);
  assert.equal(short.length, 60);
  assert.ok(short.startsWith('node /tmp/') && short.endsWith('/run.js') && short.includes('…'));
  assert.equal(LOG.mid('ls'), 'ls');
  assert.equal(LOG.cap('running a command'), 'Running a command'); assert.equal(LOG.cap('đọc file', 'vi'), 'Đọc file'); assert.equal(LOG.cap(''), '');
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
  assert.equal(hostApp(['claude', 'bash', 'claude-desktop', 'claude-desktop']), 'Claude', 'the Claude app\'s own terminal');
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

test('a permission request waits for the answer from Bobix and prints it for Claude Code', async () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-home-'));
  const token = hooks.ensureToken(home);
  let reply = null;
  const srv = startServer({ port: 0, token, onEvent: (e, signal) => (signal ? reply : null), snapshot: () => ({}) });
  await srv.ready;
  const { execFile } = require('child_process');
  const env = { ...process.env, HOME: home, USERPROFILE: home };
  const run = ev => new Promise(res => execFile(process.execPath, [path.join(__dirname, '..', 'src', 'hook', 'hook.js'), String(srv.server.address().port)], { env }, (err, out) => res(out)).stdin.end(JSON.stringify(ev)));
  const ask = { session_id: 's3', hook_event_name: 'PermissionRequest', tool_name: 'Bash', tool_input: { command: 'ls' } };
  const decision = { hookSpecificOutput: { hookEventName: 'PermissionRequest', decision: { behavior: 'allow' } } };
  reply = { secs: 5, answer: new Promise(r => setTimeout(() => r(decision), 300)) };
  let t0 = Date.now();
  assert.deepEqual(JSON.parse(await run(ask)), decision, 'the answer reaches Claude Code');
  assert.ok(Date.now() - t0 < 2000, 'the hook exits as soon as the answer is in, not at the end of the wait');
  reply = { secs: 1, answer: new Promise(() => {}) }; // nobody answers and the app never ends the reply
  t0 = Date.now();
  assert.equal(await run(ask), '', 'no answer: nothing printed, Claude asks as usual');
  assert.ok(Date.now() - t0 < 6000, 'the hook gives up soon after the wait time');
  reply = null;
  t0 = Date.now();
  assert.equal(await run(ask), '', 'the app does not wait (turned off): nothing printed');
  assert.ok(Date.now() - t0 < 1000);
  assert.equal(await run({ ...ask, hook_event_name: 'PreToolUse' }), '', 'other events never print');
  await srv.close();
  // A frozen app (accepts the connection, never replies) holds the hook only briefly.
  const net = require('net');
  const frozen = net.createServer(() => {}).listen(0, '127.0.0.1');
  await new Promise(r => frozen.once('listening', r));
  t0 = Date.now();
  const out = await new Promise(res => execFile(process.execPath, [path.join(__dirname, '..', 'src', 'hook', 'hook.js'), String(frozen.address().port)], { env }, (err, o) => res(o)).stdin.end(JSON.stringify(ask)));
  assert.equal(out, ''); assert.ok(Date.now() - t0 < 5000, 'frozen app: the hook gives up after a few seconds');
  frozen.close();
  fs.rmSync(home, { recursive: true, force: true });
});

test('headless runs (claude -p, Agent SDK) are never held for an answer', async () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-home-'));
  const token = hooks.ensureToken(home);
  const waited = [];
  const srv = startServer({ port: 0, token, onEvent: (e, signal) => { waited.push(!!signal); return null; }, snapshot: () => ({}) });
  await srv.ready;
  const { execFile } = require('child_process');
  const env = { ...process.env, HOME: home, USERPROFILE: home, CLAUDE_CODE_ENTRYPOINT: 'sdk-cli' };
  await new Promise(res => execFile(process.execPath, [path.join(__dirname, '..', 'src', 'hook', 'hook.js'), String(srv.server.address().port)], { env }, res)
    .stdin.end(JSON.stringify({ session_id: 's4', hook_event_name: 'PermissionRequest', tool_name: 'Bash' })));
  await srv.close();
  assert.deepEqual(waited, [false], 'the event still arrives, without asking the app to wait');
  fs.rmSync(home, { recursive: true, force: true });
});

test('the permission hook runs in the foreground; an old background one counts as outdated', () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-home-'));
  const src = path.join(__dirname, '..', 'src', 'hook', 'hook.js');
  hooks.install({ port: 4747, nodePath: process.execPath, hookSource: src, home });
  const file = hooks.settingsPath(home), cfg = JSON.parse(fs.readFileSync(file, 'utf8'));
  const ask = cfg.hooks.PermissionRequest[0].hooks[0], tool = cfg.hooks.PreToolUse[0].hooks[0];
  assert.equal(ask.async, undefined); assert.ok(ask.timeout >= 300);
  assert.equal(tool.async, true);
  assert.equal(hooks.status({ home }).outdated, false);
  ask.async = true; fs.writeFileSync(file, JSON.stringify(cfg));
  assert.equal(hooks.status({ home }).outdated, true, 'installed before answers: reinstall');
  fs.rmSync(home, { recursive: true, force: true });
});

test('new version check: only a newer, non-pre-release version is offered', async () => {
  const { isNewer, checkLatest } = require('../src/core/update');
  assert.equal(isNewer('v0.1.2', '0.1.1'), true);
  assert.equal(isNewer('v0.1.1', '0.1.1'), false);
  assert.equal(isNewer('v0.1.0', '0.1.1'), false);
  assert.equal(isNewer('v0.2.0', '0.1.9'), true);
  assert.equal(isNewer('v0.1.1', '0.1.1-rc.3'), true, 'the release beats its own release candidates');
  assert.equal(isNewer('garbage', '0.1.1'), false);
  const reply = body => async (url, opts) => { assert.match(opts.headers['User-Agent'], /^Bobix\//); return { ok: true, json: async () => body }; };
  const repo = 'baonguyen-agilityio/bobix';
  assert.deepEqual(await checkLatest({ repo, current: '0.1.1', fetchImpl: reply({ tag_name: 'v0.1.2', html_url: `https://github.com/${repo}/releases/tag/v0.1.2` }) }),
    { version: '0.1.2', url: `https://github.com/${repo}/releases/tag/v0.1.2` });
  assert.equal((await checkLatest({ repo, current: '0.1.1', fetchImpl: reply({ tag_name: 'v0.1.2', html_url: 'https://evil.example/x' }) })).url,
    `https://github.com/${repo}/releases/latest`, 'only opens the project\'s own release page');
  assert.equal(await checkLatest({ repo, current: '0.1.1', fetchImpl: reply({ tag_name: 'v0.1.2', prerelease: true }) }), null);
  assert.equal(await checkLatest({ repo, current: '0.1.1', fetchImpl: async () => { throw new Error('offline'); } }), null, 'offline: quiet');
  assert.equal(await checkLatest({ repo, current: '0.1.1', fetchImpl: async () => ({ ok: false }) }), null);
});

test('a Claude desktop side session knows the session it was started from', () => {
  const { parentOf } = require('../src/core/links');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-ccd-'));
  const dir = path.join(root, 'org-1', 'user-1');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'local_child-1.json'), JSON.stringify({ sessionId: 'local_child-1', title: 'secret', spawnedFrom: { sessionId: 'local_parent-1', sideSession: true } }));
  fs.writeFileSync(path.join(dir, 'local_parent-1.json'), JSON.stringify({ sessionId: 'local_parent-1' }));
  fs.writeFileSync(path.join(dir, 'local_weird.json'), JSON.stringify({ spawnedFrom: { sessionId: '../../etc/passwd' } }));
  assert.equal(parentOf('local_child-1', root), 'local_parent-1');
  assert.equal(parentOf('local_parent-1', root), null, 'a root session has no parent');
  assert.equal(parentOf('local_weird', root), null, 'only a real session id counts');
  assert.equal(parentOf('local_missing', root), null);
  assert.equal(parentOf('../org-1/user-1/local_child-1', root), null, 'ids are never paths');
  assert.equal(parentOf('local_child-1', path.join(root, 'nope')), null, 'no Claude desktop: no parent');
  fs.rmSync(root, { recursive: true, force: true });
});

test('every session of a project, git worktrees included, belongs to the same project', () => {
  const { projectOf } = require('../src/core/project');
  const G = '.' + 'git';
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-proj-')));
  const repo = path.join(root, 'app'), wt = path.join(root, 'wt', 'feature-x');
  fs.mkdirSync(path.join(repo, G, 'worktrees', 'feature-x'), { recursive: true });
  fs.mkdirSync(path.join(repo, 'src', 'deep'), { recursive: true });
  fs.writeFileSync(path.join(repo, G, 'worktrees', 'feature-x', 'commondir'), '../..\n');
  fs.mkdirSync(path.join(wt, 'src'), { recursive: true });
  fs.writeFileSync(path.join(wt, G), `gitdir: ${path.join(repo, G, 'worktrees', 'feature-x')}\n`);
  assert.equal(projectOf(repo), repo);
  assert.equal(projectOf(path.join(repo, 'src', 'deep')), repo, 'a subfolder is the same project');
  assert.equal(projectOf(path.join(wt, 'src')), repo, 'a worktree is its main repository');
  const plain = path.join(root, 'notes');
  fs.mkdirSync(plain);
  assert.equal(projectOf(plain), plain, 'outside a repository: the folder itself');
  fs.rmSync(root, { recursive: true, force: true });
});

test('pet pictures: only valid file names are kept, and they are served with the token only', async () => {
  assert.deepEqual(sanitize({ petImages: { '/a': '0123456789abcdef01234567.png', '/b': '../../etc/passwd', '/c': 'x.png' } }).petImages, { '/a': '0123456789abcdef01234567.png' });
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-pets-'));
  fs.writeFileSync(path.join(dir, '0123456789abcdef01234567.png'), 'png!');
  const token = 'b'.repeat(48);
  const srv = startServer({ port: 0, token, onEvent: () => null, snapshot: () => ({}), petDir: dir });
  await srv.ready;
  const port = srv.server.address().port;
  const get = p => new Promise(res => http.get({ host: '127.0.0.1', port, path: p }, x => { let b = ''; x.on('data', c => { b += c; }); x.on('end', () => res([x.statusCode, b])); }));
  assert.deepEqual(await get('/pet-image/0123456789abcdef01234567.png?t=' + token), [200, 'png!']);
  assert.equal((await get('/pet-image/0123456789abcdef01234567.png'))[0], 403, 'no token');
  assert.equal((await get('/pet-image/..%2F..%2Fsecret.png?t=' + token))[0], 404, 'no paths');
  await srv.close();
  fs.rmSync(dir, { recursive: true, force: true });
});

test('an uploaded picture becomes the big head of the same project\'s creature', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const faced = ctx.creature('some project', 'working', '/pet-image/a.png?t=x');
  assert.doesNotMatch(faced, /class="eye"/, 'the picture is the face');
  assert.match(faced, /<image href="\/pet-image\/a\.png\?t=x" x="4\.5" y="4\.5" width="13" height="13"/, 'it takes 13 of 22 units');
  assert.ok(faced.indexOf('<image') < faced.lastIndexOf('<rect '), 'the outline trims the corners of the picture');
  assert.equal(faced, ctx.creature('some project', 'done', '/pet-image/a.png?t=x'), 'the same project keeps its shape');
  const shapes = new Set(['a', 'b', 'c', 'd', 'e', 'f'].map(id => ctx.creature(id, 'working', '/x.png').replace(/var\(--[a-z]+\)/g, '')));
  assert.ok(shapes.size > 1, 'projects differ in shape');
});

test('feedback carries no folders, tokens or session names, and fits in a link', () => {
  const { redact, summarize, issueUrl, MAX_URL } = require('../src/core/feedback');
  const home = '/Users/Jo Doe';
  assert.equal(redact('failed /Users/Jo Doe/work/acme-secret/a.js', home), 'failed .../a.js');
  assert.equal(redact('failed /home/sam/acme/a.js'), 'failed .../a.js');
  assert.equal(redact('failed C:\\Users\\sam\\acme\\a.js'), 'failed .../a.js');
  assert.equal(redact('http://x/y?t=abc&z=1'), 'http:/.../y?t=[redacted]&z=1');
  assert.equal(redact('key ' + 'a'.repeat(48)), 'key [redacted]');
  const d = { app: '1.0', platform: 'linux', release: '6', arch: 'x64', settings: {}, hooks: { mode: 'node', error: 'bad /Users/Jo Doe/.claude/x.json' }, displays: [{ id: 9, bounds: { x: 1 }, scale: 2, primary: true }],
    overlay: { ready: true, visible: true, alwaysOnTop: true, page: { pets: 1, labels: [3] }, errors: ['secret in overlay'] },
    sessions: [{ state: 'working', sinceMin: 2, title: true, chain: ['/Users/Jo Doe/acme/run'], env: ['ACME_SECRET'] }], somethingNew: 'never listed' };
  const info = summarize(d, ['boom /Users/Jo Doe/acme/a.js'], home), text = [...info.lines, ...info.errs].join('\n');
  for (const leak of ['Jo Doe', 'acme', 'ACME_SECRET', 'never listed', 'secret in overlay']) assert.ok(!text.includes(leak), leak);
  assert.match(text, /Sessions: working 2m/);
  const short = issueUrl({ repo: 'o/r', kind: 'bug', title: 'Pets & co', description: 'hi', info });
  assert.ok(short.url.startsWith('https://github.com/o/r/issues/new?title=%5BBug%5D%20Pets%20%26%20co&body='));
  const idea = decodeURIComponent(issueUrl({ repo: 'o/r', kind: 'idea', title: 'Themes', description: 'nice', info: null }).url);
  assert.match(idea, /title=\[Idea\] Themes&body=## The idea\nnice$/, 'an idea carries no technical details');
  assert.equal(short.truncated, false);
  assert.match(decodeURIComponent(short.url), /## Recent errors\n- boom \.\.\.\/a\.js/);
  const long = issueUrl({ repo: 'o/r', kind: 'bug', title: 't', description: 'đ'.repeat(5000), info: { lines: ['- l'], errs: ['- e'.repeat(90)] } });
  assert.ok(long.url.length <= MAX_URL && long.truncated, 'cut to fit');
  assert.match(decodeURIComponent(long.url), /## Technical details/, 'the details survive');
});

test('sessions of one project share a shape; colours tell them apart', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const fill = svg => [...new Set((svg.match(/fill="var\(--[a-z]+\)"/g) || []))].join();
  const shape = svg => svg.replace(/fill="[^"]*"/g, '');
  const list = [{ id: 'a', project: '/p' }, { id: 'b', project: '/p' }, { id: 'c', project: '/q' },
    ...['k1', 'k2', 'k3'].map(id => ({ id, project: '/p', parent: 'a' }))];
  const art = s => ctx.creature((list.find(x => x.id === s.parent) || s).project, 'working', null, ctx.petColor(s, list));
  const [A, B, K1, K2] = ['a', 'b', 'k1', 'k2'].map(id => art(list.find(x => x.id === id)));
  assert.equal(shape(A), shape(B), 'same project, same shape');
  assert.notEqual(fill(A), fill(B), 'two sessions of a project differ in colour');
  assert.equal(shape(K1), shape(A), 'a side session has its parent\'s shape');
  assert.notEqual(fill(K1), fill(A), 'never its parent\'s colour');
  assert.notEqual(fill(K1), fill(K2), 'neighbours differ');
  assert.notEqual(fill(K1), fill(B), 'a side session never looks like another session of the project');
  assert.equal(ctx.petColor(list[2], list), ctx.petColor({ id: 'c', project: '/q' }, [{ id: 'c', project: '/q' }]), 'alone in its project: the project\'s colour');
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

