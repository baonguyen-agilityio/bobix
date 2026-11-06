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
const strict = require('node:assert/strict');
const zlib = require('zlib');
const P = require('../src/core/pixelate');
const petstore = require('../src/core/petstore');

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

// ---------- pet pictures as pixel art: core/pixelate.js ----------

const fixture = name => fs.readFileSync(path.join(__dirname, 'fixtures', name));

// A small PNG reader for the tests (8-bit, colour type 6 or 3, not interlaced): the app decodes PNGs with Electron.
function readPng(buf) {
  const chunks = {}; let idat = [];
  for (let p = 8; p < buf.length;) {
    const len = buf.readUInt32BE(p), type = buf.toString('latin1', p + 4, p + 8), data = buf.subarray(p + 8, p + 8 + len);
    strict.equal(buf.readUInt32BE(p + 8 + len), zlib.crc32(buf.subarray(p + 4, p + 8 + len)), `${type} crc`);
    if (type === 'IDAT') idat.push(data); else chunks[type] = data;
    p += 12 + len;
  }
  const w = chunks.IHDR.readUInt32BE(0), h = chunks.IHDR.readUInt32BE(4), type = chunks.IHDR[9], bpp = type === 6 ? 4 : 1;
  strict.equal(chunks.IHDR[8], 8); strict.equal(chunks.IHDR[12], 0);
  const raw = zlib.inflateSync(Buffer.concat(idat)), px = Buffer.alloc(w * h * bpp), row = w * bpp;
  for (let y = 0; y < h; y++) {
    const f = raw[y * (row + 1)], line = raw.subarray(y * (row + 1) + 1, (y + 1) * (row + 1));
    for (let x = 0; x < row; x++) {
      const a = x >= bpp ? px[y * row + x - bpp] : 0, b = y ? px[(y - 1) * row + x] : 0, c = x >= bpp && y ? px[(y - 1) * row + x - bpp] : 0;
      const pa = Math.abs(b - c), pb = Math.abs(a - c), pc = Math.abs(a + b - 2 * c);
      const pred = [0, a, b, (a + b) >> 1, pa <= pb && pa <= pc ? a : pb <= pc ? b : c][f];
      px[y * row + x] = line[x] + pred;
    }
  }
  return { w, h, type, px, plte: chunks.PLTE, trns: chunks.tRNS };
}

const at = (rgba, w, x, y) => Array.from(rgba.subarray((y * w + x) * 4, (y * w + x) * 4 + 4));

// fixtures/dispose.gif, made with ImageMagick 7.1.2:
//   magick \( -size 8x8 xc:red -set dispose None -set delay 7 \) \( -size 4x4 xc:blue -set page 8x8+2+2 -set dispose Background
//     -set delay 20 \) \( -size 3x3 xc:lime -alpha set -fill none -draw 'color 1,1 point' -set page 8x8+0+0 -set dispose Previous
//     -set delay 30 \) \( -size 2x2 xc:yellow -set page 8x8+5+5 -set dispose None -set delay 40 \) -loop 3 dispose.gif
// `-loop 3` is written as NETSCAPE value 2 (repeats after the first play; gifenc's `repeat` takes the same value).
//  frame 0: full red, dispose None, 7 cs        frame 1: 4×4 blue at +2+2, dispose Background, 20 cs
//  frame 2: 3×3 lime at +0+0 with a transparent centre, dispose Previous, 30 cs   frame 3: 2×2 yellow at +5+5, 40 cs
// The expected pictures are `magick dispose.gif -coalesce` (R red, B blue, G lime, Y yellow, . transparent).
// fixtures/alpha.png: magick -size 16x16 xc:none -fill '#f08030' -draw 'circle 7.5,7.5 7.5,1.5' -fill '#203060'
//   -draw 'rectangle 5,6 6,7' -draw 'rectangle 9,6 10,7' -define png:color-type=6 alpha.png
const COALESCED = [
  ['RRRRRRRR', 'RRRRRRRR', 'RRRRRRRR', 'RRRRRRRR', 'RRRRRRRR', 'RRRRRRRR', 'RRRRRRRR', 'RRRRRRRR'],
  ['RRRRRRRR', 'RRRRRRRR', 'RRBBBBRR', 'RRBBBBRR', 'RRBBBBRR', 'RRBBBBRR', 'RRRRRRRR', 'RRRRRRRR'],
  ['GGGRRRRR', 'GRGRRRRR', 'GGG...RR', 'RR....RR', 'RR....RR', 'RR....RR', 'RRRRRRRR', 'RRRRRRRR'],
  ['RRRRRRRR', 'RRRRRRRR', 'RR....RR', 'RR....RR', 'RR....RR', 'RR...YYR', 'RRRRRYYR', 'RRRRRRRR'],
];
const NAMES = { '255,0,0,255': 'R', '0,0,255,255': 'B', '0,255,0,255': 'G', '255,255,0,255': 'Y' };
const picture = (rgba, w, h) => Array.from({ length: h }, (_, y) => Array.from({ length: w }, (_, x) => {
  const c = at(rgba, w, x, y); return c[3] === 0 ? '.' : NAMES[c.join()] || '?';
}).join(''));

test('decodeGif composites delta frames like ImageMagick -coalesce', () => {
  const buf = fixture('dispose.gif');
  // the fixture really is made of partial frames with disposals 1, 2, 3, 1
  const raw = P.parseGif(buf).images;
  strict.deepEqual(raw.map(f => [f.width, f.height]), [[8, 8], [4, 4], [3, 3], [2, 2]]);
  strict.deepEqual(raw.map(f => f.gce.disposal), [1, 2, 3, 1]);
  // a Buffer that is a view into a larger pool must decode the same (byteOffset respected)
  const pooled = Buffer.concat([Buffer.alloc(13, 7), buf]).subarray(13);
  for (const b of [buf, pooled]) {
    const s = P.decodeGif(b);
    strict.equal(s.width, 8); strict.equal(s.height, 8);
    strict.deepEqual(s.frames.map(f => picture(f.rgba, 8, 8)), COALESCED);
    strict.deepEqual(s.frames.map(f => f.delay), [70, 200, 300, 400]);
    strict.equal(s.loop, 2); strict.equal(s.opaque, false); strict.equal(s.truncated, false);
  }
  strict.throws(() => P.decodeGif(Buffer.from('not a gif at all')));
  strict.throws(() => P.decodeGif(buf.subarray(0, 10)));
});

// More GIF shapes, each compared pixel by pixel with ImageMagick's reading of it (`magick <f>.gif -coalesce -depth 8
// rgba:<f>.rgba`, every frame in a row):
//   interlaced.gif  magick -size 16x16 gradient:'#e04020-#2040e0' -colors 16 -interlace GIF interlaced.gif
//   nogce.gif       magick -size 12x10 xc:'#3080c0' -fill '#f0d020' -draw 'rectangle 2,2 6,5' -fill '#c02040'
//                     -draw 'point 9,7' GIF87:nogce.gif   (GIF87a: no graphic control extension at all)
//   lct.gif         gifenc: an 8×8 red/lime checkerboard (120 ms), then blue over yellow halves (50 ms) given its own
//                     palette, which gifenc writes as a local colour table
test('the GIF parser reads interlaced images, local colour tables and GIFs without a control block', () => {
  const gif = name => { const b = fixture(name + '.gif'); return [b, P.parseGif(b), fixture(name + '.rgba')]; };
  const same = (s, ref) => strict.deepEqual(Buffer.concat(s.frames.map(f => Buffer.from(f.rgba))), ref);
  let [b, g, ref] = gif('interlaced');
  strict.deepEqual(g.images.map(i => i.interlaced), [true]);
  const s = P.decodeGif(b);
  same(s, ref);
  strict.equal(new Set(Array.from({ length: 16 }, (_, y) => at(s.frames[0].rgba, 16, 0, y).join())).size, 16, 'every row differs, so a row out of place shows');
  [b, g, ref] = gif('nogce');
  strict.equal(b.toString('latin1', 0, 6), 'GIF87a'); strict.deepEqual(g.images.map(i => i.gce), [null]);
  same(P.decodeGif(b), ref);
  strict.deepEqual(P.decodeGif(b).frames.map(f => f.delay), [100], 'no control block: 100 ms');
  [b, g, ref] = gif('lct');
  strict.deepEqual(g.images.map(i => i.table === g.gct), [true, false], 'the second frame brings its own colours');
  const l = P.decodeGif(b);
  same(l, ref);
  strict.deepEqual(l.frames.map(f => f.delay), [120, 50]);
});

// fixtures/big-delta.gif: a 600×300 screen of delta frames, so each frame is shrunk to 256 from only what changed:
//   magick -size 600x300 xc:red -set dispose None \( -size 200x100 xc:blue -set page 600x300+100+50 -set dispose
//     Background \) \( -size 50x50 xc:lime -set page 600x300+400+200 -set dispose Previous \) \( -size 30x30 xc:yellow
//     -set page 600x300+10+10 -set dispose None \) -loop 0 big-delta.gif
test('a big delta GIF shrinks each frame exactly as shrinking the whole coalesced frame would', () => {
  const b = fixture('big-delta.gif');
  strict.deepEqual(P.parseGif(b).images.map(i => [i.left, i.top, i.width, i.height, i.gce.disposal]), [[0, 0, 600, 300, 1], [100, 50, 200, 100, 2], [400, 200, 50, 50, 3], [10, 10, 30, 30, 1]]);
  const W = 600, H = 300, screen = new Uint8ClampedArray(W * H * 4);
  const rect = (x, y, w, h, c) => { for (let j = y; j < y + h; j++) for (let i = x; i < x + w; i++) screen.set(c, (j * W + i) * 4); };
  const snap = () => P.fromRGBA(W, H, screen).frames[0].rgba; // the whole frame shrunk in one go
  rect(0, 0, W, H, [255, 0, 0, 255]); const f0 = snap();
  rect(100, 50, 200, 100, [0, 0, 255, 255]); const f1 = snap();
  rect(100, 50, 200, 100, [0, 0, 0, 0]); rect(400, 200, 50, 50, [0, 255, 0, 255]); const f2 = snap();
  rect(400, 200, 50, 50, [255, 0, 0, 255]); rect(10, 10, 30, 30, [255, 255, 0, 255]); const f3 = snap();
  const s = P.decodeGif(b);
  strict.equal(s.width, 256); strict.equal(s.height, 128);
  s.frames.forEach((f, i) => strict.ok(Buffer.from(f.rgba).equals(Buffer.from([f0, f1, f2, f3][i])), `frame ${i}`));
});

test('a damaged GIF keeps its whole images, and one without any throws', () => {
  const b = fixture('dispose.gif'), g = P.parseGif(b);
  const cut = n => P.decodeGif(b.subarray(0, n));
  // cut inside the third image's data: the first two images stay, as they were
  strict.equal(cut(g.images[2].end - 2).frames.length, 2);
  strict.deepEqual(cut(g.images[2].end - 2).frames.map(f => picture(f.rgba, 8, 8)), COALESCED.slice(0, 2));
  // no trailer, or garbage where the next block should start: everything before it is kept
  strict.equal(cut(b.length - 1).frames.length, 4);
  // (the junk byte is followed by a well-formed 1×1 image, which must not be read: nothing after damage is trusted)
  const junk = Buffer.from([0x99, 0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 1, 0x44, 0]);
  strict.equal(P.decodeGif(Buffer.concat([b.subarray(0, g.images[1].end), junk])).frames.length, 2);
  const bad = Buffer.from(b); bad[g.images[0].data] = 250;
  strict.throws(() => P.decodeGif(bad), /not a GIF/, 'the first data sub-block claims to run past the end');
  for (const x of [Buffer.alloc(0), Buffer.from('GIF89a'), b.subarray(0, g.images[0].end - 1), Buffer.from('PNG89a').fill(0, 6), Buffer.concat([Buffer.from('GIF89a'), Buffer.alloc(7)])])
    strict.throws(() => P.decodeGif(x), /not a GIF/);
});

test('decodeGif shrinks to 256 and caps the frame count', () => {
  // a 600×300 GIF with MAX_FRAMES + 5 one-pixel frames, written with gifenc
  const { GIFEncoder } = require('../src/core/gif/gifenc'), g = GIFEncoder();
  for (let i = 0; i < P.MAX_FRAMES + 5; i++) g.writeFrame(new Uint8Array(i ? 1 : 600 * 300), i ? 1 : 600, i ? 1 : 300, { palette: i ? undefined : [[10, 20, 30], [0, 0, 0]], delay: 50 });
  g.finish();
  const s = P.decodeGif(Buffer.from(g.bytes()));
  strict.equal(s.width, 256); strict.equal(s.height, 128);
  strict.equal(s.frames.length, P.MAX_FRAMES); strict.equal(s.truncated, true); strict.equal(s.opaque, true);
  strict.deepEqual(at(s.frames[0].rgba, 256, 100, 100), [10, 20, 30, 255]);
});

test('normalizeOpts clamps bad input', () => {
  strict.deepEqual(P.normalizeOpts(undefined), P.DEFAULTS);
  strict.deepEqual(P.normalizeOpts({}, { plainBg: true }), { ...P.DEFAULTS, removeBg: true });
  strict.equal(P.normalizeOpts({ removeBg: false }, { plainBg: true }).removeBg, false);
  const o = P.normalizeOpts({ size: 1000, colors: 99, accents: -3, clean: 'yes', outline: 0, removeBg: 'x', whole: 'yes' });
  strict.deepEqual(o, { size: 64, colors: 32, accents: 0, clean: true, outline: true, removeBg: false, whole: false });
  strict.equal(P.normalizeOpts({ whole: true }).whole, true, 'the picture can be the whole pet');
  strict.equal(P.normalizeOpts({ size: 41 }).size, 48, 'the nearest size');
  strict.equal(P.normalizeOpts({ size: '64' }).size, 48, 'strings are not numbers');
  strict.deepEqual(P.normalizeOpts({ colors: 4, accents: 8 }), { ...P.DEFAULTS, colors: 4, accents: 1 }, 'accents leave room for 2 + outline');
  strict.equal(P.normalizeOpts({ colors: 4, accents: 8, outline: false }).accents, 2);
  strict.equal(P.normalizeOpts({ colors: NaN, accents: Infinity }).colors, 16);
  const again = P.normalizeOpts(o);
  strict.deepEqual(again, o, 'normalizing twice changes nothing');
});

test('pixelate: palette within budget, alpha only 0/255, outline counted', () => {
  const png = readPng(fixture('alpha.png'));
  strict.equal(png.type, 6);
  const src = P.fromRGBA(png.w, png.h, new Uint8ClampedArray(png.px));
  strict.equal(src.opaque, false);
  strict.ok(Array.from(src.frames[0].rgba).some((v, i) => i % 4 === 3 && v > 0 && v < 255), 'the fixture has soft edges');
  for (const colors of [4, 6, 16]) {
    const art = P.pixelate(src, { colors, accents: 1 });
    strict.ok(art.palette.length <= colors, `${art.palette.length} ≤ ${colors}`);
    strict.equal(art.width, 16 + 2, 'never upscaled; 1 px border for the outline');
    for (const f of art.frames) for (const v of f.index) strict.ok(v <= art.palette.length);
    // PNG out: the only alpha values are 255 (every colour) and 0 (the one transparent entry)
    const out = P.encode(art);
    strict.equal(out.ext, 'png');
    const back = readPng(out.data);
    strict.equal(back.type, 3);
    strict.deepEqual(Array.from(back.trns), [...art.palette.map(() => 255), 0]);
    strict.deepEqual(Array.from(back.px), Array.from(art.frames[0].index));
    strict.deepEqual(Array.from(back.plte.subarray(0, art.palette.length * 3)), art.palette.flat());
  }
  // hard alpha at 128: with nothing else changing the shape, it is exactly the source pixels with alpha ≥ 128
  const bare = P.pixelate(src, { clean: false, outline: false }), solid = bare.frames[0].index.filter(v => v < bare.palette.length).length;
  strict.equal(solid, Array.from(src.frames[0].rgba).filter((v, i) => i % 4 === 3 && v >= 128).length);
  // a picture with more colours than the budget fills it exactly, outline and accents included
  const grad = new Uint8ClampedArray(40 * 40 * 4);
  for (let i = 0; i < 1600; i++) grad.set([i % 40 * 6, Math.floor(i / 40) * 6, (i * 7) % 256, 255], i * 4);
  const rich = P.fromRGBA(40, 40, grad);
  for (const o of [{ colors: 4, accents: 1 }, { colors: 16, accents: 3 }, { colors: 32, accents: 8, outline: false }]) {
    strict.equal(P.pixelate(rich, { ...o, removeBg: false }).palette.length, o.colors, JSON.stringify(o));
  }
  // a 64 px picture becomes 46 + 2 at size 48
  const big = P.fromRGBA(64, 32, new Uint8ClampedArray(64 * 32 * 4).fill(200));
  const art = P.pixelate(big, { size: 48, outline: false });
  strict.deepEqual([art.width, art.height], [48, 25]);
});

test('frame count, delays and loop survive decode → pixelate → encode → decode', () => {
  const s = P.decodeGif(fixture('dispose.gif'));
  const art = P.pixelate(s, { size: 32 });
  strict.equal(art.frames.length, 4);
  const out = P.encode(art);
  strict.equal(out.ext, 'gif');
  const back = P.decodeGif(out.data);
  strict.equal(back.frames.length, 4);
  strict.deepEqual(back.frames.map(f => f.delay), [70, 200, 300, 400]);
  strict.equal(back.loop, 2);
  // every frame is the art frame (dispose 2 + full-size frames: nothing leaks from the frame before)
  const raw = P.parseGif(out.data);
  strict.ok(raw.images.every(f => f.gce.disposal === 2 && f.gce.transparent === art.palette.length));
  strict.ok(raw.images.every(f => f.table === raw.gct), 'one global palette');
  back.frames.forEach((f, n) => {
    for (let i = 0; i < art.width * art.height; i++) {
      const k = art.frames[n].index[i], c = at(f.rgba, art.width, i % art.width, Math.floor(i / art.width));
      strict.deepEqual(c, k === art.palette.length ? [0, 0, 0, 0] : [...art.palette[k], 255]);
    }
  });
});

// A JPG-like picture: opaque, a slightly noisy off-white background, a dark blue disc with a white "eye" inside it.
// (A real JPG needs Electron's nativeImage to decode, which plain node tests cannot load; main.js hands its pixels over
// as RGBA, which is what this builds.)
function jpgLike(n = 40) {
  const rgba = new Uint8ClampedArray(n * n * 4);
  let seed = 1; const noise = () => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) % 13 - 6;
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
    const p = (y * n + x) * 4, r = Math.hypot(x - n / 2 + 0.5, y - n / 2 + 0.5), eye = Math.hypot(x - n / 2 + 0.5, y - n / 2 + 4.5) < 3;
    const c = r < n / 3 && !eye ? [30, 40, 120] : [245 + noise(), 245 + noise(), 240 + noise()];
    rgba.set([...c, 255], p);
  }
  return rgba;
}

// A photo: a sky fading from blue-grey to light at the top, a dark sea below it, and a black coat standing on the
// bottom edge. The bottom corners are the sea's colour, and the coat is within BG_THRESHOLD of it.
function photoLike(n = 40) {
  const rgba = new Uint8ClampedArray(n * n * 4);
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
    const sky = [70 + y * 4, 100 + y * 4, 140 + y * 3], sea = [20, 30, 55], coat = [10, 12, 20];
    rgba.set([...(Math.abs(x - n / 2) < n / 5 && y > n / 3 ? coat : y < n / 2 ? sky : sea), 255], (y * n + x) * 4);
  }
  return rgba;
}

test('background removal is on by default only when the picture has a plain background', () => {
  strict.equal(P.fromRGBA(40, 40, jpgLike()).plainBg, true, 'a subject on a plain, slightly noisy background');
  const photo = P.fromRGBA(40, 40, photoLike());
  strict.equal(photo.opaque, true); strict.equal(photo.plainBg, false, 'a photo: corners of different colours');
  strict.deepEqual(P.pixelate(photo, {}).frames[0].index, P.pixelate(photo, { removeBg: false }).frames[0].index, 'nothing is cut out of a photo by default');
  strict.notDeepEqual(P.pixelate(photo, { removeBg: true }).frames[0].index, P.pixelate(photo, { removeBg: false }).frames[0].index, 'asked for, it still removes');
  const framed = jpgLike(); for (let i = 0; i < 40 * 40; i += 7) framed.set([30, 40, 120, 255], (i % 40 === 0 || i < 40 ? i + 1 : i) * 4);
  strict.equal(P.fromRGBA(40, 40, framed).plainBg, false, 'a busy border is not a plain background');
  const alpha = new Uint8ClampedArray(16 * 16 * 4); alpha.set([200, 0, 0, 255], (8 * 16 + 8) * 4);
  strict.equal(P.fromRGBA(16, 16, alpha).plainBg, false, 'transparent corners: nothing to remove');
  strict.equal(P.decodeGif(fixture('nogce.gif')).plainBg, true, 'a GIF on one flat colour');
});

test('background removal takes the edge-connected background and keeps the subject', () => {
  const src = P.fromRGBA(40, 40, jpgLike());
  strict.equal(src.opaque, true);
  const keep = P.pixelate(src, { size: 48, removeBg: false, outline: false });
  strict.ok(keep.frames[0].index.every((v, i) => {
    const x = i % keep.width, y = Math.floor(i / keep.width);
    return (x === 0 || y === 0 || x === keep.width - 1 || y === keep.height - 1) === (v === keep.palette.length);
  }), 'without removal only the border is transparent');
  const art = P.pixelate(src, { size: 48, removeBg: true, outline: false }), f = art.frames[0], w = art.width, T = art.palette.length;
  const k = (x, y) => f.index[(y + 1) * w + x + 1]; // picture coordinates inside the 1 px border
  for (const [x, y] of [[0, 0], [39, 0], [0, 39], [39, 39], [20, 2], [2, 20]]) strict.equal(k(x, y), T, `bg ${x},${y}`);
  strict.notEqual(k(20, 20), T, 'the disc stays');
  const eye = k(20, 16);
  strict.notEqual(eye, T, 'the white eye inside the disc is not connected to the edge, so it stays');
  strict.ok(art.palette[eye].every(v => v > 200), 'and it is still white');
  strict.ok(P.BG_THRESHOLD > 6 * Math.sqrt(3), 'the threshold covers the noise of the test background');
});

test('accents keep a small saturated spot that median cut alone loses', () => {
  // a 40×40 grey-blue gradient under a flat band (the most common colour) with a 3×3 red spot
  const n = 40, rgba = new Uint8ClampedArray(n * n * 4);
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) rgba.set(y < 10 ? [100, 110, 130, 255] : [60 + x * 4, 70 + x * 4, 90 + y * 3, 255], (y * n + x) * 4);
  for (let y = 18; y < 21; y++) for (let x = 18; x < 21; x++) rgba.set([230, 20, 30, 255], (y * n + x) * 4);
  const src = P.fromRGBA(n, n, rgba), red = c => c[0] > 180 && c[1] < 80 && c[2] < 80;
  const plain = P.pixelate(src, { size: 48, colors: 8, accents: 0, outline: false, clean: false, removeBg: false });
  strict.ok(!plain.palette.some(red), `median cut alone has no red: ${JSON.stringify(plain.palette)}`);
  const art = P.pixelate(src, { size: 48, colors: 8, accents: 1, outline: false, clean: false, removeBg: false });
  strict.equal(art.palette.length, 8);
  const r = art.palette.findIndex(red);
  strict.ok(r >= 0, 'the accent is the red');
  strict.equal(art.frames[0].index.filter(v => v === r).length, 9, 'and the spot is drawn with it');
});

test('outline: 1 px, darker, touches the shape only edge to edge', () => {
  // a diamond: its diagonal sides would get diagonal-only outline pixels with an 8-neighbour outline
  const n = 21, rgba = new Uint8ClampedArray(n * n * 4);
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (Math.abs(x - 10) + Math.abs(y - 10) <= 8) rgba.set([240, 180, 60, 255], (y * n + x) * 4);
  const art = P.pixelate(P.fromRGBA(n, n, rgba), { size: 32 }), f = art.frames[0], w = art.width, h = art.height;
  const o = art.palette.length - 1, T = art.palette.length;
  strict.deepEqual(art.palette, [[240, 180, 60], [139, 96, 11]], 'the shape colour and the outline (lightness halved)');
  const get = (x, y) => x < 0 || y < 0 || x >= w || y >= h ? T : f.index[y * w + x];
  let outlined = 0;
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const v = get(x, y), four = [get(x - 1, y), get(x + 1, y), get(x, y - 1), get(x, y + 1)];
    const touches = four.some(u => u !== T && u !== o);
    if (v === o) { outlined++; strict.ok(touches, `outline ${x},${y} touches the shape by an edge`); }
    if (v === T) strict.ok(!touches, `transparent ${x},${y} next to the shape should be outline`);
  }
  strict.ok(outlined > 0);
  // corners of the border stay free: the outline never needed the pad's corner and never clips
  for (const i of [0, w - 1, (h - 1) * w, h * w - 1]) strict.equal(f.index[i], T);
  const none = P.pixelate(P.fromRGBA(n, n, rgba), { size: 32, outline: false });
  strict.deepEqual(none.palette, [[240, 180, 60]]);
});

test('clean removes specks and noise but keeps a 1 px detail', () => {
  // 12×12 orange square; a lone opaque pixel away from it; a slightly different orange pixel inside; a dark eye inside
  const n = 20, rgba = new Uint8ClampedArray(n * n * 4);
  for (let y = 4; y < 16; y++) for (let x = 4; x < 16; x++) rgba.set([240, 140, 40, 255], (y * n + x) * 4);
  rgba.set([20, 20, 20, 255], (8 * n + 8) * 4);      // eye
  rgba.set([225, 132, 38, 255], (12 * n + 12) * 4);  // shading noise
  rgba.set([240, 140, 40, 255], (1 * n + 1) * 4);    // speck
  rgba.set([0, 0, 0, 0], (10 * n + 13) * 4);         // pinhole
  const src = P.fromRGBA(n, n, rgba), opts = { size: 32, colors: 5, accents: 0, outline: false };
  const dirty = P.pixelate(src, { ...opts, clean: false }), art = P.pixelate(src, { ...opts, clean: true });
  const k = (a, x, y) => a.frames[0].index[(y + 1) * a.width + x + 1], T = a => a.palette.length;
  strict.notEqual(k(dirty, 1, 1), T(dirty)); strict.equal(k(art, 1, 1), T(art), 'speck gone');
  strict.equal(k(dirty, 13, 10), T(dirty)); strict.equal(k(art, 13, 10), k(art, 12, 10), 'pinhole filled');
  strict.notEqual(k(dirty, 12, 12), k(dirty, 11, 12), 'the noise pixel has its own colour before');
  strict.equal(k(art, 12, 12), k(art, 11, 12), 'noise takes the colour around it');
  strict.notEqual(k(art, 8, 8), k(art, 7, 8), 'the eye stays');
});

test('a pixel-art face is drawn without smoothing, any other picture as before', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const pixel = ctx.creature('p', 'idle', '/a.png', undefined, true), plain = ctx.creature('p', 'idle', '/a.png');
  strict.match(pixel, /<image href="\/a\.png" x="4\.5" y="4\.5" width="13" height="13" style="image-rendering:pixelated" /);
  strict.doesNotMatch(plain, /image-rendering/);
  strict.equal(pixel.replace(' style="image-rendering:pixelated"', ''), plain, 'only the smoothing differs');
});

test('a pixel-art picture is the whole pet when the person chose so, else the face in the creature', () => {
  const ctx = { URLSearchParams, location: { search: '?t=tok' } };
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const s = { id: 's1', project: '/p', image: '0123456789abcdef01234567.gif', pixel: true };
  const url = '/pet-image/0123456789abcdef01234567.gif?t=tok';
  const whole = ctx.petArt({ ...s, whole: true }, 'working', 2);
  strict.equal(whole, ctx.picturePet(url));
  strict.match(whole, /^<svg class="pic" viewBox="0 0 1 1"[^>]*><image href="\/pet-image\/0123456789abcdef01234567\.gif\?t=tok" width="1" height="1" preserveAspectRatio="xMidYMax meet" style="image-rendering:pixelated"\/><\/svg>$/);
  strict.doesNotMatch(whole, /<rect /, 'no creature body, ears or outline around it');
  strict.equal(ctx.petArt(s, 'working', 2), ctx.creature('/p', 'working', url, 2, true), 'not chosen: the face in the body');
  strict.equal(ctx.petArt({ ...s, pixel: false, whole: true }, 'working', 2), ctx.creature('/p', 'working', url, 2, false), 'a picture from before pixel art stays a face');
});

// ---------- pet pictures: storage (core/petstore.js) and settings ----------

const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-petstore-'));
const OPTS = { size: 48, colors: 16, accents: 3, clean: true, outline: true, removeBg: false, whole: false };
const png = tail => Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 13, 10, 26, 10]), Buffer.from(tail)]);
const list = d => fs.readdirSync(d).filter(f => f !== 'originals').sort().concat(fs.existsSync(path.join(d, 'originals')) ? fs.readdirSync(path.join(d, 'originals')).map(f => `originals/${f}`).sort() : []);

test('pet pictures: settings keep legacy file names and valid { src, file, opts } entries, drop the rest', () => {
  const A = '0123456789abcdef01234567', B = 'fedcba9876543210fedcba98';
  const good = { src: `${A}.webp`, file: `${B}.gif`, opts: { ...OPTS, size: 64 } };
  const out = sanitize({ petImages: {
    '/legacy': `${A}.png`, '/new': good,
    '/badname': '../../etc/passwd', '/badfile': { src: `${A}.png`, file: '../x.png', opts: OPTS }, '/badsrc': { src: `${A}.bmp`, file: `${B}.png`, opts: OPTS },
    '/traversal': { src: `../${A}.png`, file: `${B}.png`, opts: OPTS }, '/nums': 5, '/null': null,
    '/gifresult-as-jpg': { src: `${A}.png`, file: `${B}.jpg`, opts: OPTS },
  } }).petImages;
  strict.deepEqual(Object.keys(out).sort(), ['/legacy', '/new']);
  strict.equal(out['/legacy'], `${A}.png`);
  strict.deepEqual(out['/new'], good);
});

test('pet pictures: options of an entry are clamped, missing ones fall back to the defaults', () => {
  const A = '0123456789abcdef01234567';
  const e = sanitize({ petImages: { '/p': { src: `${A}.png`, file: `${A}.png`, opts: { size: 9999, colors: 1000, accents: -4, clean: 'yes', outline: 0, removeBg: 1, whole: 1 } } } }).petImages['/p'].opts;
  strict.deepEqual(e, { size: 64, colors: 32, accents: 0, clean: true, outline: true, removeBg: false, whole: false });
  strict.deepEqual(sanitize({ petImages: { '/p': { src: `${A}.png`, file: `${A}.png` } } }).petImages['/p'].opts, OPTS, 'no opts at all: the defaults');
});

test('petstore: kindOf reads the bytes, not the name', () => {
  strict.equal(petstore.kindOf(png('xxxxxxxx')), 'png');
  strict.equal(petstore.kindOf(Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0, 0, 0, 0, 0])), 'jpg');
  strict.equal(petstore.kindOf(Buffer.from('GIF89a......')), 'gif');
  strict.equal(petstore.kindOf(Buffer.from('RIFF\0\0\0\0WEBPVP8 ')), 'webp');
  strict.equal(petstore.kindOf(Buffer.from('RIFF\0\0\0\0WAVEfmt ')), null);
  strict.equal(petstore.kindOf(Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"/>')), null);
});

test('petstore: fromBitmap turns premultiplied BGRA into straight RGBA', () => {
  // rgba(255,0,0,.5) as nativeImage gives it: [b, g, r, a] = [0, 0, 128, 128]; opaque green; fully transparent
  const out = petstore.fromBitmap(Buffer.from([0, 0, 128, 128, 0, 255, 0, 255, 0, 0, 0, 0]));
  strict.deepEqual([...out], [255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 0, 0]);
});

test('petstore: an import is stored as original + result, and can be read back for editing', () => {
  const dir = tmp(), original = png('the original'), result = { data: Buffer.from('GIF89a result'), ext: 'gif' };
  const images = petstore.saveImport(dir, {}, '/p', { original, kind: 'png', result, opts: { ...OPTS, size: 32 } });
  const e = images['/p'];
  strict.match(e.src, /^[a-f0-9]{24}\.png$/);
  strict.match(e.file, /^[a-f0-9]{24}\.gif$/);
  strict.deepEqual(e.opts, { ...OPTS, size: 32 });
  strict.deepEqual(fs.readFileSync(path.join(dir, e.file)), result.data, 'the result sits where the route serves it');
  strict.deepEqual(fs.readFileSync(path.join(dir, 'originals', e.src)), original, 'the original sits in originals/');
  strict.equal(petstore.served(e), e.file);
  strict.deepEqual(petstore.readOriginal(dir, e), { original, opts: { ...OPTS, size: 32 } });
  strict.deepEqual(sanitize({ petImages: images }).petImages, images, 'what is stored survives the settings check');
  fs.rmSync(path.join(dir, 'originals', e.src));
  strict.equal(petstore.readOriginal(dir, e), null, 'a lost original is null, not a throw');
  strict.equal(petstore.readOriginal(dir, e.file), null, 'a legacy entry has no original');
  strict.equal(petstore.served('0123456789abcdef01234567.png'), '0123456789abcdef01234567.png');
  fs.rmSync(dir, { recursive: true, force: true });
});

test('petstore: only the originals folder is out of the route\'s reach', async () => {
  const dir = tmp(), token = 'c'.repeat(48);
  const e = petstore.saveImport(dir, {}, '/p', { original: png('o'), kind: 'png', result: { data: png('r'), ext: 'png' }, opts: OPTS })['/p'];
  const srv = startServer({ port: 0, token, onEvent: () => null, snapshot: () => ({}), petDir: dir });
  try {
    await srv.ready;
    const port = srv.server.address().port;
    const get = p => new Promise(res => http.get({ host: '127.0.0.1', port, path: p }, x => { x.resume(); x.on('end', () => res(x.statusCode)); }));
    strict.equal(await get(`/pet-image/${e.file}?t=${token}`), 200);
    strict.equal(await get(`/pet-image/${e.src}?t=${token}`), 404, 'the original is not served by its name');
    strict.equal(await get(`/pet-image/originals/${e.src}?t=${token}`), 404);
    strict.equal(await get(`/pet-image/originals%2F${e.src}?t=${token}`), 404);
  } finally {
    await srv.close();
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('petstore: replacing or resetting deletes only the files no entry uses any more', () => {
  const dir = tmp(), r = (s, ext = 'png') => ({ data: Buffer.from(s), ext });
  let images = petstore.saveImport(dir, {}, '/a', { original: png('one'), kind: 'png', result: r('shared result'), opts: OPTS });
  images = petstore.saveImport(dir, images, '/b', { original: png('one'), kind: 'png', result: r('shared result'), opts: { ...OPTS, size: 32 } });
  strict.equal(images['/a'].file, images['/b'].file, 'the same bytes are the same file');
  strict.equal(list(dir).length, 2, 'two projects, one original and one result on disk');
  // /a gets another picture: nothing is deleted, /b still uses both files
  const old = images['/a'];
  images = petstore.saveImport(dir, images, '/a', { original: png('two'), kind: 'png', result: r('second result'), opts: OPTS });
  strict.deepEqual(petstore.release(dir, images, old), [], 'a file /b still uses stays');
  strict.equal(list(dir).length, 4);
  // /b is reset: the shared files go, /a's own stay
  const { ['/b']: oldB, ...rest } = images;
  strict.equal(petstore.release(dir, rest, oldB).length, 2);
  strict.deepEqual(list(dir), [images['/a'].file, `originals/${images['/a'].src}`].sort());
  // a legacy entry: its one file goes, unless another entry names it
  fs.writeFileSync(path.join(dir, '0123456789abcdef01234567.png'), 'legacy');
  petstore.release(dir, { ...rest, '/other': '0123456789abcdef01234567.png' }, '0123456789abcdef01234567.png');
  strict.ok(fs.existsSync(path.join(dir, '0123456789abcdef01234567.png')), 'a legacy file another project uses stays');
  petstore.release(dir, rest, '0123456789abcdef01234567.png');
  strict.ok(!fs.existsSync(path.join(dir, '0123456789abcdef01234567.png')), 'an unused legacy file goes');
  petstore.release(dir, rest, undefined);
  petstore.release(dir, rest, { src: 'nope.png', file: 'nope.png' }); // nothing to delete is not an error
  fs.rmSync(dir, { recursive: true, force: true });
});

// ---------- pet pictures: an outline colour of the person's own ----------

test('normalizeOpts reads an outline colour typed in any of the usual forms', () => {
  const oc = v => P.normalizeOpts({ outlineColor: v }).outlineColor;
  const good = {
    '#f0a': '#ff00aa', '#F0A8': '#ff00aa', '#ff00AA': '#ff00aa', '#ff00aa80': '#ff00aa', '  #AbC  ': '#aabbcc',
    'rgb(255, 0, 128)': '#ff0080', 'rgb(255 0 128)': '#ff0080', 'RGBA(255,0,128,0.5)': '#ff0080', 'rgb(255 0 128 / 50%)': '#ff0080',
    'rgb(100% 0% 50%)': '#ff0080', 'rgb(300 -5 12.6)': '#ff000d',
    'hsl(200, 80%, 40%)': '#1481b8', 'hsl(200 80% 40%)': '#1481b8', 'hsla(200deg, 80%, 40%, .3)': '#1481b8', 'hsl(200 80 40)': '#1481b8',
    'hsl(0.5turn 100% 50%)': '#00ffff', 'hsl(-120 100% 50%)': '#0000ff', 'hsl(400grad 100% 50% / 0)': '#ff0000', 'hsl(0 0% 100%)': '#ffffff',
  };
  for (const [v, want] of Object.entries(good)) strict.equal(oc(v), want, v);
  const bad = ['red', '#ff', '#ggg', 'f0a', '#f0a8c', 'rgb(1 2)', 'rgb(1 2 3 4 5)', 'rgb(1,,2,3)', 'rgb(1 2 3x)', 'rgb(10deg 0 0)',
    'hsl(10% 50% 50%)', 'rgb(1e999 0 0)', 'rgb((1 2 3))', 'rgb(1 2 3) x', 'cmyk(1 2 3)', '#' + 'a'.repeat(70), '', null, 5, {}, ['#fff']];
  for (const v of bad) strict.equal(oc(v), undefined, JSON.stringify(v));
  strict.ok(!('outlineColor' in P.normalizeOpts({})), 'left out by default: the automatic outline, as in entries saved before it');
  strict.ok(!('outlineColor' in P.normalizeOpts({ outlineColor: null })));
  const off = P.normalizeOpts({ outline: false, outlineColor: 'rgb(255 0 128)' });
  strict.equal(off.outlineColor, '#ff0080', 'kept while the outline is off, so it comes back with it');
  strict.deepEqual(P.normalizeOpts(off), off, 'normalizing twice changes nothing');
  strict.equal(sanitize({ petImages: { '/p': { src: `${'a'.repeat(24)}.png`, file: `${'b'.repeat(24)}.png`, opts: { outlineColor: '#F0A' } } } }).petImages['/p'].opts.outlineColor, '#ff00aa', 'settings.json keeps it');
});

test('pixelate draws the outline in exactly the chosen colour, in PNG and GIF', () => {
  const n = 21, rgba = new Uint8ClampedArray(n * n * 4);
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (Math.abs(x - 10) + Math.abs(y - 10) <= 8) rgba.set([240, 180, 60, 255], (y * n + x) * 4);
  const src = P.fromRGBA(n, n, rgba), auto = P.pixelate(src, { size: 32 }), ring = Array.from(auto.frames[0].index, v => +(v === 1));
  const png = (art, hex) => {
    const back = readPng(P.encode(art).data), want = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16));
    const o = art.palette.findIndex(c => c.join() === want.join());
    strict.ok(o >= 0, `${hex} is in the palette`);
    strict.deepEqual(Array.from(back.plte.subarray(o * 3, o * 3 + 3)), want, `${hex} is written as it is`);
    strict.deepEqual(Array.from(back.px, v => +(v === o)), ring, `${hex}: the same ring as the automatic outline, in that colour`);
    return art.palette;
  };
  strict.deepEqual(png(P.pixelate(src, { size: 32, outlineColor: '#f0a' }), '#ff00aa'), [[240, 180, 60], [255, 0, 170]]);
  // one step from the shape's colour is still its own entry, never folded into the shape's; an equal one is shared
  strict.deepEqual(png(P.pixelate(src, { size: 32, outlineColor: '#f0b43d' }), '#f0b43d'), [[240, 180, 60], [240, 180, 61]]);
  const same = P.pixelate(src, { size: 32, outlineColor: '#f0b43c' });
  strict.deepEqual(same.palette, [[240, 180, 60]]);
  strict.deepEqual(Array.from(same.frames[0].index, v => +(v === 0)), Array.from(auto.frames[0].index, v => +(v < 2)), 'the ring is drawn with the shared entry');
  // no colour, null or one that is not a colour: the automatic outline, byte for byte; outline off ignores the colour
  const bytes = o => P.encode(P.pixelate(src, { size: 32, ...o })).data;
  for (const o of [{ outlineColor: null }, { outlineColor: 'bogus' }]) strict.ok(bytes(o).equals(bytes({})), JSON.stringify(o));
  strict.ok(bytes({ outline: false, outlineColor: '#f0a' }).equals(bytes({ outline: false })));
  // an animated GIF: every outline pixel of every frame decodes to the chosen colour
  const gifSrc = P.decodeGif(fixture('dispose.gif')), art = P.pixelate(gifSrc, { size: 32, outlineColor: 'hsl(200 80% 40%)' });
  const o = art.palette.findIndex(c => c.join() === '20,129,184'), back = P.decodeGif(P.encode(art).data);
  strict.ok(o >= 0, 'the outline colour is in the palette');
  strict.equal(back.frames.length, 4);
  back.frames.forEach((f, k) => {
    let ringed = 0;
    for (let i = 0; i < art.width * art.height; i++) if (art.frames[k].index[i] === o) { ringed++; strict.deepEqual(at(f.rgba, art.width, i % art.width, Math.floor(i / art.width)), [20, 129, 184, 255]); }
    strict.ok(ringed > 0, `frame ${k} has an outline`);
  });
  strict.ok(P.encode(P.pixelate(gifSrc, { size: 32, outlineColor: null })).data.equals(P.encode(P.pixelate(gifSrc, { size: 32 })).data), 'GIF: null is the automatic outline');
});

test('a session keeps its project while Claude works in a folder outside every git repository', () => {
  const { pinProject } = require('../src/core/state');
  const G = '.' + 'git';
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-pin-')));
  const repo = path.join(root, 'app'), other = path.join(root, 'lib'), scratch = path.join(root, 'scratchpad'), plain = path.join(root, 'notes');
  for (const d of [path.join(repo, G), path.join(repo, 'src'), path.join(other, G), path.join(scratch, 'probe'), path.join(plain, 'dist')]) fs.mkdirSync(d, { recursive: true });
  const s = { cwd: path.join(repo, 'src') }, at = cwd => { s.cwd = cwd; return pinProject(s); };
  assert.equal(pinProject(s), repo);
  assert.equal(at(scratch), repo, 'a scratch folder outside git: still the repository (its picture stays)');
  assert.equal(at(path.join(scratch, 'probe')), repo);
  assert.equal(at(other), other, 'another repository: that project');
  assert.equal(at(repo), repo);
  // a project outside git: its first folder, through its own subfolders
  const n = { cwd: plain };
  assert.equal(pinProject(n), plain);
  n.cwd = path.join(plain, 'dist');
  assert.equal(pinProject(n), plain);
  // first seen outside git (Bobix started while Claude was there): that folder, until a repository
  const f = { cwd: scratch };
  assert.equal(pinProject(f), scratch);
  f.cwd = repo;
  assert.equal(pinProject(f), repo);
  fs.rmSync(root, { recursive: true, force: true });
});

test('"until I answer": the setting is validated, is the default, and the hook cap outlasts any wait', () => {
  const S = require('../src/core/settings');
  strict.equal(DEFAULTS.approveWait, -1);
  strict.equal(sanitize({}).approveWait, -1, 'a settings file without the key gets the default');
  strict.equal(sanitize({ approveWait: 45 }).approveWait, 45, 'a value set earlier is kept');
  strict.equal(sanitize({ approveWait: -1 }).approveWait, S.UNTIL_ANSWER);
  strict.equal(sanitize({ approveWait: '-1' }).approveWait, S.UNTIL_ANSWER, 'the select sends a string');
  strict.equal(sanitize({ approveWait: -5 }).approveWait, 0, 'any other negative number means off, never "until I answer"');
  strict.equal(sanitize({ approveWait: 9999 }).approveWait, 300);
  strict.equal(sanitize({ approveWait: 'x' }, { ...DEFAULTS, approveWait: 20 }).approveWait, 20, 'junk keeps the current value');
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-wait-')), src = path.join(home, 'hook.js');
  fs.writeFileSync(src, '// hook');
  hooks.install({ port: 5000, nodePath: process.execPath, hookSource: src, home });
  const file = hooks.settingsPath(home), written = JSON.parse(fs.readFileSync(file, 'utf8')).hooks;
  strict.equal(written.PermissionRequest[0].hooks[0].timeout, 86400, 'a day, in seconds');
  strict.equal(written.PermissionRequest[0].hooks[0].async, undefined, 'it must block until answered');
  strict.equal(written.Stop[0].hooks[0].timeout, 5, 'the other events stay quick');
  strict.equal(hooks.status({ home }).outdated, false);
  const old = JSON.parse(fs.readFileSync(file, 'utf8'));
  old.hooks.PermissionRequest[0].hooks[0].timeout = 330; // installed by an earlier build
  fs.writeFileSync(file, JSON.stringify(old));
  strict.equal(hooks.status({ home }).outdated, true, 'a cap shorter than a day is reinstalled at start');
  fs.rmSync(home, { recursive: true, force: true });
});

test('a permission request is held only where the person can see it', () => {
  const { holdSecs } = require('../src/core/settings');
  const on = { ...DEFAULTS, approveWait: 60 }, pet = { pet: 'auto' };
  strict.equal(holdSecs(on, pet), 60);
  strict.equal(holdSecs({ ...on, approveWait: -1 }, pet), Infinity);
  strict.equal(holdSecs({ ...on, approveWait: 0 }, pet), 0, 'off');
  strict.equal(holdSecs({ ...on, approveWait: -1, paused: true }, pet), 0, 'paused');
  strict.equal(holdSecs({ ...on, approveWait: -1, petsVisible: false }, pet), 0, 'pets hidden');
  strict.equal(holdSecs({ ...on, approveWait: -1 }, { pet: 'hidden' }), 0, 'this pet is hidden');
  strict.equal(holdSecs({ ...on, approveWait: -1 }, { pet: 'shown' }), Infinity);
  strict.equal(holdSecs({ ...on, approveWait: 60, petsVisible: false }, pet), 0, 'a timed wait too');
});

test('a session whose claude process is gone is forgotten; a shell or an unchecked pid never is', () => {
  const { gone, CLAUDE_PROC, publicView } = require('../src/core/state');
  const m = new Map();
  for (const [id, pid] of [['dead', 101], ['live', 102], ['shell', 103], ['unchecked', 104], ['moved', 105]])
    apply(m, { session_id: id, hook_event_name: 'UserPromptSubmit', prompt: 'x', bobix_claude_pid: pid });
  m.get('dead').pidIsClaude = 101; m.get('live').pidIsClaude = 102;
  m.get('moved').pidIsClaude = 99; // checked an older pid; a new one came in since
  const alive = new Set([102]);
  assert.deepEqual(gone(m, pid => alive.has(pid)), ['dead'], 'only a checked claude pid that is no longer running');
  assert.ok(['claude', 'node', 'Claude', 'node'].every(n => CLAUDE_PROC.test(n)));
  assert.ok(['sh', 'bash', 'zsh', 'cmd', 'powershell', 'Bobix', 'Electron', '?', 'claude-desktop'].every(n => !CLAUDE_PROC.test(n)), 'shells and others are not claude');
  assert.ok(!('pidIsClaude' in publicView(m.get('dead'))) && !('pidChecked' in publicView(m.get('dead'))), 'bookkeeping stays in the main process');
});
