// Run: npm test  (node:test, no extra deps)
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const markdown = require('../src/ui/markdown');
const LOG = require('../src/ui/log');
const strict = require('node:assert/strict');
const zlib = require('zlib');
const P = require('../src/ui/pixelate');







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
  const long = `node /tmp/${'d'.repeat(200)}/run.js`;
  const log = [{ kind: 'prompt', tool: '', text: 'go' }, ...['mcp__b__batch', 'mcp__b__batch', 'mcp__b__batch', 'Read', 'Bash'].map(tool => ({ kind: 'tool', tool, text: tool === 'Bash' ? long : '' }))];
  assert.deepEqual(LOG.fold(log).map(l => [l.kind, l.tool, l.n]),
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


test('the pet is the fine 36×36 version of its creature: same shape and colours in every state, a face per state', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const theme = fs.readFileSync(path.join(__dirname, '../src/ui/theme.css'), 'utf8'), TOKENS = require('vm').runInContext('TOKENS', ctx);
  for (const [name, [L, C, h]] of Object.entries(TOKENS)) {
    const m = theme.match(new RegExp(`${name}:oklch\\(([\\d.]+)% ([\\d.]+) ([\\d.]+)\\)`));
    strict.deepEqual([+m[1] / 100, +m[2], +m[3]], [L, C, h], `${name} as in theme.css`);
  }
  const states = ['working', 'waiting', 'done', 'idle'], sp = states.map(st => ctx.fineSprite('/p', st, 1));
  for (const s of sp) strict.deepEqual([s.w, s.h, s.data.length], [36, 36, 1296]);
  const silhouette = s => [...s.data].map(c => (c ? 1 : 0)).join('');
  strict.ok(sp.every(s => silhouette(s) === silhouette(sp[0])), 'the face changes, never the shape');
  strict.equal(new Set(sp.map(s => [...s.data].join())).size, 4, 'each state has its own face');
  const d = sp[0].data, at = (x, y) => d[y * 36 + x], line = ctx.oklch(ctx.mixOklch(TOKENS['--mint'], TOKENS['--n-900'], 0.45));
  strict.ok([...Array(36).keys()].every(y => [...Array(36).keys()].every(x => !at(x, y) || at(x, y) === line || [[1, 0], [-1, 0], [0, 1], [0, -1]].every(([a, b]) => at(x + a, y + b))))
    , 'a 1-pixel outline closes the body: only outline pixels touch the outside');
  strict.ok(d.includes(ctx.oklch(TOKENS['--mint'])) && d.includes(0xffffffff), 'its colour, and a white glint in the working eyes');
  strict.equal(ctx.fineSprite('/p', 'idle', 1), ctx.fineSprite('/p', 'idle', 1), 'built once, then cached');
  strict.match(ctx.creature('/p', 'done', null, 1), /^<svg viewBox="0 0 36 36" width="80%" height="80%" shape-rendering="crispEdges"[^>]*>(<path fill="#[0-9a-f]{6}" d="[M0-9 hvz-]+"\/>)+<\/svg>$/);
});

test('pixel effects start on the right changes, and nothing plays while nothing happens', () => {
  const ctx = {};
  require('vm').runInNewContext(['creature.js', 'pet-fx.js'].map(f => fs.readFileSync(path.join(__dirname, '../src/ui', f), 'utf8')).join('\n'), ctx);
  const run = (code) => require('vm').runInContext(code, ctx);
  ctx.EV = (prev, s) => JSON.parse(JSON.stringify(ctx.fxEvents(prev, s)));
  const s = (o) => ({ id: 'a', state: 'working', lv: 1, xp: 0, ctx: 80, parent: null, tool: '', ...o });
  const kinds = (prev, now) => ctx.EV(prev, now).map(e => e[0]);
  strict.deepEqual(kinds(s(), s()), [], 'no change, no effect');
  strict.deepEqual(kinds(s(), s({ state: 'waiting' })), [], 'waiting is a loop, not a one-shot');
  strict.deepEqual(ctx.EV(s({ xp: 1 }), s({ state: 'done', xp: 2 })), [['xp', { f0: 1 / 3, f1: 2 / 3, lv: 1 }]], 'a finished task: +1 XP');
  strict.deepEqual(ctx.EV(s({ xp: 2 }), s({ state: 'done', lv: 2, xp: 0 })), [['xp', { f0: 2 / 3, f1: 1, lv: 2 }]], 'the third one: a level');
  strict.deepEqual(kinds(s({ state: 'done' }), s({ state: 'done', since: 2 })), [], 'still done: no new XP');
  strict.deepEqual(kinds(s({ ctx: 30 }), s({ ctx: 24 })), ['ctx'], 'context drops under 25%');
  strict.deepEqual(kinds(s({ ctx: 20 }), s({ ctx: 10 })), [], 'already low: once');
  strict.deepEqual(kinds(s({ ctx: null }), s({ ctx: null })), [], 'unknown context is not low');
  strict.deepEqual(kinds(null, s({ ctx: 12 })), ['ctx'], 'a pet that shows up low');
  strict.deepEqual(kinds(null, s({ parent: 'p' })), [], 'a new side session just walks in behind its parent');
  const loop = (st, tool) => JSON.parse(JSON.stringify(ctx.fxLoop(s({ tool }), st)));
  strict.deepEqual(loop('waiting'), ['aura']);
  strict.deepEqual([loop('working', 'Edit'), loop('working', 'Bash'), loop('working', 'Read'), loop('working', 'Grep')], [['tool', 0], ['tool', 1], ['tool', 2], ['tool', 3]]);
  strict.equal(loop('working', 'TodoWrite'), null, 'a tool without a prop: nothing');
  strict.equal(loop('done'), null); strict.equal(loop('idle'), null);
  // Every effect runs and draws: one-shots end on time, loops keep going; the pet is drawn from its fine pixels.
  run(`globalThis.out = Object.keys(FX).map(kind => {
    const pet = st => ({ ...fineSprite('/p', st, 0), k: 1 }), r = fxRun(kind, { pet, f0: 2 / 3, f1: 1, lv: 7, seg: 2 }), b = new Buf(), img = { data: new Uint8ClampedArray(GW * GH * 16) };
    let painted = 0;
    for (let i = 0; i < 400 && !r.done; i++) { r.step(1 / 60); r.draw(b); b.toImage(img); painted = Math.max(painted, new Uint32Array(img.data.buffer).filter(Boolean).length); }
    return [kind, r.done, painted > 0];
  })`);
  strict.deepEqual(JSON.parse(JSON.stringify(ctx.out)), [['aura', false, true], ['xp', true, true], ['tool', false, true], ['ctx', true, true], ['end', true, true]]);
  // From its second loop on, a loop repeats exactly every cycle, so the overlay draws one cycle once and replays it.
  run(`globalThis.rep = [['aura', {}], ...[0, 1, 2, 3].map(seg => ['tool', { seg, first: true }])].map(([kind, o]) => {
    const pet = st => ({ ...fineSprite('/p', st, 0), k: 1 }), r = fxRun(kind, { pet, ...o }), b = new Buf(), img = { data: new Uint8ClampedArray(GW * GH * 16) };
    const lead = FX[kind].D, n = Math.round(FX[kind].cycle * 30), shots = [];
    for (let f = 0, sim = 0; f < 2 * n; f++) { for (const until = Math.round((lead + f / 30) / SIM); sim < until; sim++) r.step(SIM); r.draw(b); b.toImage(img); shots.push(img.data.join() + b.shake); }
    return shots.slice(0, n).every((x, i) => x === shots[i + n]);
  })`);
  strict.deepEqual([...ctx.rep], [true, true, true, true, true]);
  // One of each kind waits: a second XP keeps the first one's starting bar.
  const Q = (q, playing, kind, o) => JSON.parse(JSON.stringify(ctx.fxQueue(q, playing, kind, o)));
  strict.deepEqual(Q([['xp', { f0: 0, f1: 1 / 3, lv: 1 }]], null, 'xp', { f0: 1 / 3, f1: 2 / 3, lv: 1 }), [['xp', { f0: 0, f1: 2 / 3, lv: 1 }]]);
  strict.deepEqual(Q([['ctx']], null, 'xp', { f0: 0, f1: 1 / 3, lv: 1 }).map(e => e[0]), ['ctx', 'xp']);
  // Mirrored for the top edge (or a prop on the other side): the pet and the text come out upright, not upside down.
  run(`globalThis.flip = (() => {
    const spr = { ...fineSprite('/p', 'working', 0), k: 1 }, W2 = GW * 2, H2 = GH * 2;
    const draw = (fx, fy) => { const b = new Buf(); b.fx = fx; b.fy = fy; b.clear(); blit(b, spr, CX, FEET); text(b, 'LV 2', CX, 10, WARM[3]); const im = { data: new Uint8ClampedArray(W2 * H2 * 4) }; b.toImage(im); return new Uint32Array(im.data.buffer); };
    const a = draw(false, false), m = draw(false, true), h = draw(true, false);
    const rows = (img, r0, n, x0 = 0, x1 = W2) => { const o = []; for (let j = 0; j < n; j++) o.push(img.slice((r0 + j) * W2 + x0, (r0 + j) * W2 + x1).join()); return o.join('|'); };
    return [rows(a, 2 * (FEET - 18), 36) === rows(m, H2 - 2 * FEET, 36), rows(a, 18, 14) === rows(m, H2 - 32, 14),
      rows(a, 2 * (FEET - 18), 36, 2 * CX - 18, 2 * CX + 18) === rows(h, 2 * (FEET - 18), 36, W2 - 2 * CX - 18, W2 - 2 * CX + 18), rows(a, 2 * (FEET - 18), 36) !== rows(a, H2 - 2 * FEET, 36)];
  })()`);
  strict.deepEqual([...ctx.flip], [true, true, true, true], 'pet upright top-down, text upright top-down, pet upright left-right');
});

// The overlay itself is checked pixel by pixel in a browser (overlay.test.js); here the drawing side: an effect drawn for
// a room never draws outside it, and a tight room shrinks it instead of cutting it.
test('an effect drawn for its room stays inside it; the aura hugs the pet and the prop stays in its hands', () => {
  const ctx = {};
  require('vm').runInNewContext(['creature.js', 'pet-fx.js'].map(f => fs.readFileSync(path.join(__dirname, '../src/ui', f), 'utf8')).join('\n'), ctx);
  const out = require('vm').runInContext(`(() => {
    const pet = st => ({ ...fineSprite('/p', st, 0), k: 1 }), res = {};
    // A packed row: 10 cells either side of the pet's column, 20 above its feet (the top edge's name tag), 3 below.
    for (const [kind, o] of [['aura', {}], ['xp', { f0: 2 / 3, f1: 1, lv: 9 }], ['ctx', {}], ['end', {}], ['tool', { seg: 1, mini: true }], ['tool', { seg: 1, plain: true }]]) {
      const r = fxRun(kind, { pet, ...o }), room = [r.ox - 10, FEET - 20, r.ox + 10, FEET + 3], b = new Buf();
      let outside = 0, drawn = 0;
      for (let f = 0; f < 140 && !r.done; f++) {
        r.step(1 / 30); r.draw(b, room);
        for (let y = 0, i = 0; y < GH; y++) for (let x = 0; x < GW; x++, i++) if (b.c[i] || b.l[i] > .08) { drawn++; if (x < room[0] || x >= room[2] || y < room[1] || y >= room[3]) outside++; }
      }
      res[kind + (o.plain ? '-plain' : '')] = { outside, drawn };
    }
    // With all the room it wants, the aura and the held prop still stay within 3 cells of the pet (18 cells wide).
    const span = (kind, o) => { const r = fxRun(kind, { pet, ...o }), b = new Buf(); let x0 = GW, x1 = -1; for (let f = 0; f < 80; f++) { r.step(1 / 30); r.draw(b, null); for (let y = 0, i = 0; y < GH; y++) for (let x = 0; x < GW; x++, i++) if (b.c[i]) { x0 = Math.min(x0, x); x1 = Math.max(x1, x); } } return [r.ox - x0, x1 + 1 - r.ox]; };
    res.span = [span('aura', {}), ...[0, 1, 2, 3].map(seg => span('tool', { seg, mini: true }))];
    return JSON.stringify(res);
  })()`, ctx);
  const r = JSON.parse(out);
  for (const k of ['aura', 'xp', 'ctx', 'end', 'tool', 'tool-plain']) { assert.equal(r[k].outside, 0, `${k} drew outside its room`); assert.ok(r[k].drawn > 0, `${k} drew nothing`); }
  for (const [l, rr] of r.span) assert.ok(l <= 12 && rr <= 12, `drawn within 3 cells of the pet: ${JSON.stringify(r.span)}`);
  // With effects on, pets show no status balloon (the effect tells the state); without them, as before.
  assert.equal(require('vm').runInContext("[statusBalloon(true, 'waiting'), statusBalloon(true, 'done'), statusBalloon(false, 'waiting')]", ctx).join(), ',,waiting');
  // The rect maths the clip is made of.
  assert.deepEqual(JSON.parse(JSON.stringify(ctx.rectMinus([0, 0, 10, 10], [3, 3, 6, 6]))), [[0, 0, 10, 3], [0, 6, 10, 10], [0, 3, 3, 6], [6, 3, 10, 6]]);
  const hs = JSON.parse(JSON.stringify(ctx.fxHoles([0, 0, 100, 100], [40, 40, 60, 100], [[30, 50, 70, 70], [50, 60, 90, 80], [200, 0, 300, 10]])));
  const area = rs => rs.reduce((a, r) => a + (r[2] - r[0]) * (r[3] - r[1]), 0);
  assert.equal(area(hs), 20 * 10 + 10 * 20 + 30 * 20 - 10 * 10, 'the union of both minus the own pet, nothing counted twice, nothing outside the box');
  assert.equal(area(JSON.parse(JSON.stringify(ctx.fxHoles([0, 0, 100, 100], [40, 40, 60, 100], [], [[30, 50, 70, 70]])))), 40 * 20, 'a pet in front is cut out over its own pet too');
});

test('sessions of one project share a shape; colours tell them apart', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const fill = svg => [...new Set((svg.match(/fill="#[0-9a-f]{6}"/g) || []))].join();
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
  // A side session that arrives later (whatever its id) leaves the others' colours be (a burst of them never recolours).
  const later = [...list, { id: '0-new', project: '/p', parent: 'a' }];
  for (const x of list) assert.equal(ctx.petColor(x, later), ctx.petColor(x, list), `${x.id} keeps its colour`);
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
  const { GIFEncoder } = require('../src/ui/gif/gifenc'), g = GIFEncoder();
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


















// ---------- the hook (src/hook/hook.js) against a stand-in for the app's /event (the app's side is tested in Rust) ----------
test('hook.js forwards a trimmed event, with the token, and prints a session title it is answered with', async () => {
  const http = require('http'), { execFile } = require('child_process');
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bobix-home-')), token = 'a'.repeat(48);
  fs.mkdirSync(path.join(home, '.claude', 'bobix'), { recursive: true });
  fs.writeFileSync(path.join(home, '.claude', 'bobix', 'token'), token);
  const got = [];
  const titled = { hookSpecificOutput: { hookEventName: 'SessionStart', sessionTitle: '[SHA-27] Fix it' } };
  const srv = http.createServer((req, res) => {
    let b = '';
    req.on('data', c => { b += c; });
    req.on('end', () => {
      const e = JSON.parse(b);
      got.push({ e, token: req.headers['x-bobix'], wait: req.headers['x-bobix-wait'] });
      if (e.hook_event_name === 'SessionStart') res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(titled));
      else res.writeHead(204).end();
    });
  }).listen(0, '127.0.0.1');
  await new Promise(r => srv.once('listening', r));
  const env = { ...process.env, HOME: home, USERPROFILE: home, CLAUDE_PID: '4242', CLAUDE_CODE_HOST_SESSION_ID: 'local_test-1' };
  const run = ev => new Promise(res => execFile(process.execPath, [path.join(__dirname, '..', 'src', 'hook', 'hook.js'), String(srv.address().port)], { env }, (err, out) => res(out)).stdin.end(JSON.stringify(ev)));
  const t0 = Date.now();
  assert.equal(await run({ session_id: 's2', hook_event_name: 'PreToolUse', tool_name: 'Write', tool_input: { file_path: '/a.txt', content: 'x'.repeat(5000) }, tool_response: 'big' }), '');
  assert.ok(Date.now() - t0 < 1500, 'hook exits right after posting');
  const { e, token: sent, wait } = got[0];
  assert.equal(sent, token);
  assert.equal(wait, undefined, 'only a permission request asks the app to wait');
  assert.deepEqual(e.tool_input, { file_path: '/a.txt' });
  assert.equal(e.tool_response, undefined);
  assert.equal(e.bobix_claude_pid, 4242);
  assert.equal(e.bobix_host_session, 'local_test-1');
  assert.deepEqual(JSON.parse(await run({ session_id: 's5', hook_event_name: 'SessionStart', source: 'startup' })), titled, 'a session title reaches Claude Code');
  srv.close();
  fs.rmSync(home, { recursive: true, force: true });
});

test('board: each issue in one stage of the day, and the end-of-day report', () => {
  const B = require('../src/ui/board');
  const now = new Date(2026, 10, 8, 17, 0).getTime(), yesterday = new Date(2026, 10, 7, 23, 0).toISOString();
  const mk = (iid, extra = {}) => ({ iid, title: 't' + iid, url: 'u' + iid, sessions: [], labels: [], ...extra });
  const work = { status: 'ok',
    local: [{ path: 'g/app', dir: '/src/app', issues: [mk(1, { sessions: ['a', 'gone'], stage: 'doing' }), mk(2, { review: 'https://h/g/app/-/merge_requests/9', sessions: ['b'], stage: 'verify' }), mk(3, { stage: 'todo' }), mk(4, { sessions: ['c'], stage: 'doing' })] }],
    away: [{ path: 'g/web', issues: [mk(5, { stage: 'todo' })] }],
    closed: [{ iid: 7, title: 't7', project: 'g/app', closedAt: new Date(now - 3600e3).toISOString() }, { iid: 8, title: 't8', project: 'g/web', closedAt: yesterday },
      { iid: 7, title: 't7', project: 'g/app', closedAt: new Date(now - 60e3).toISOString() }, { iid: 9, title: 't9', project: 'g/new', closedAt: new Date(now).toISOString() }] };
  const ss = [{ id: 'a', st: 'working', since: 5 }, { id: 'b', st: 'done', since: 6 }, { id: 'c', st: 'waiting', since: 1 }, { id: 'z', st: 'idle', since: 1 }];
  const st = B.stages(work, ss, now);
  const ids = k => st[k].map(([i]) => i.iid);
  assert.deepEqual([ids('todo'), ids('doing'), ids('verify'), ids('done')], [[3, 5], [4, 1], [2], [7, 9]], 'each in the stage the work model gave it; most urgent first; closed today only, once');
  assert.deepEqual(st.doing[1][2].map(s => s.id), ['a'], 'a session that is gone does not count');
  assert.equal(st.done[1][1].path, 'g/new', 'a project with no open issue still names its group');
  assert.deepEqual(B.loose(st, ss).map(s => s.id), ['z']);
  assert.deepEqual(B.stages({ status: 'off' }, ss), { todo: [], doing: [], verify: [], done: [] });
  const I18N = require('../src/ui/i18n');
  const T = (k, v) => I18N.t('vi', k, v);
  assert.equal(B.report(st, T, now, 'vi-VN'), 'Báo cáo ngày 8/11/2026\n\n[app]\nĐã xong:\n- #7 t7\nChờ kiểm tra:\n- #2 t2\n\n[new]\nĐã xong:\n- #9 t9\n');
  assert.equal(B.report({ done: [], verify: [] }, T, now, 'vi-VN'), 'Báo cáo ngày 8/11/2026\n\nHôm nay chưa có việc xong hay chờ kiểm tra.\n');
  assert.equal(B.day(new Date(2026, 0, 2, 0, 5).getTime()), '2026-01-02');
});

test('board: the issue to start next, and what Claude gets', () => {
  const B = require('../src/ui/board');
  const mk = (iid, extra = {}) => [{ iid, title: 't' + iid, url: 'u' + iid, ...extra }, { path: extra.p || 'g/app', dir: extra.away ? undefined : '/src/' + (extra.p || 'g/app') }];
  const todo = [mk(9), mk(4), mk(2, { resume: 'old' }), mk(3, { away: true }), mk(5, { due: '2026-12-01' }), mk(6, { p: 'g/web' })];
  const st = { todo };
  assert.equal(B.next(st)[0].iid, 5, 'one with a due date first');
  assert.equal(B.next({ todo: todo.filter(x => x[0].iid !== 5) })[0].iid, 4, 'then the oldest; one named after an old session and one not on this machine are left out');
  assert.equal(B.next(st, [{ dir: '/src/g/app', iid: 5 }])[0].iid, 4, 'not one a new session is already starting');
  assert.equal(B.next(st, [], 'g/web')[0].iid, 6, 'in the project picked');
  assert.equal(B.next({ todo: [] }), null);
  assert.equal(B.brief({ iid: 54, title: 'Pick next', url: 'https://h/i/54' }, '  Steps\n- a  '), '#54 Pick next\n\nSteps\n- a\n\nhttps://h/i/54');
  assert.equal(B.brief({ iid: 1, title: 'T', url: 'u' }, null), '#1 T\n\nu', 'no description');
});

test('board: the skill to suggest for an issue, and the prompt with it', () => {
  const B = require('../src/ui/board');
  const sk = (name, source = 'personal') => ({ name, source, description: '' });
  const skills = [sk('ak:cook'), sk('ak:fix'), sk('fix-ticket', 'project'), sk('ba-qa-kit:write-stories', 'plugin'), sk('ba-qa-kit:write-test-cases', 'plugin'), sk('docx')];
  assert.deepEqual(B.skillFor(skills, ['bug'], ''), { name: 'fix-ticket', why: 'label', label: 'bug' }, "a label's skill; the repo's own wins a tie");
  assert.deepEqual(B.skillFor(skills, ['bug'], 'qa'), { name: 'fix-ticket', why: 'label', label: 'bug' });
  assert.deepEqual(B.skillFor(skills, [], 'qa'), { name: 'ba-qa-kit:write-test-cases', why: 'role', label: undefined }, 'the role alone');
  assert.deepEqual(B.skillFor(skills, ['enhancement'], 'ba'), { name: 'ba-qa-kit:write-stories', why: 'label', label: 'enhancement' }, 'label and role together score most');
  assert.equal(B.skillFor(skills, ['design'], ''), null, 'a label with no matching words: no skill');
  assert.equal(B.skillFor(skills, [], ''), null);
  assert.equal(B.skillFor([sk('docx')], ['bug'], 'dev'), null, 'only words in the name count');
  const text = '#53 T\n\nbody\n\nhttps://h/i/53';
  const one = B.withSkill(text, 'fix-ticket', 'https://h/i/53');
  assert.equal(one, '/fix-ticket https://h/i/53\n\n#53 T\n\nbody\n\nhttps://h/i/53');
  assert.equal(B.withSkill(one, 'ak:cook', 'https://h/i/53'), '/ak:cook https://h/i/53\n\n' + text, 'another skill replaces the first line');
  assert.equal(B.withSkill(one, '', 'https://h/i/53'), text, 'no skill');
});

test("every script and stylesheet the pages load is one the app's server serves", () => {
  const ui = path.join(__dirname, '../src/ui');
  const listed = JSON.parse(fs.readFileSync(path.join(__dirname, '../src-tauri/src/server.rs'), 'utf8').match(/const ASSETS: \[&str; \d+\] = (\[[^\]]*\])/)[1]);
  const fonts = rel => /^fonts\/[a-z0-9-]+\.(woff2|css)$/.test(rel);
  for (const page of ['pets.html', 'panel.html']) {
    const refs = [...fs.readFileSync(path.join(ui, page), 'utf8').matchAll(/(?:src|href)="\/ui\/([^"?#]+)"/g)].map(m => m[1]);
    assert.ok(refs.length > 2, page);
    for (const rel of refs) {
      assert.ok(listed.includes(rel) || fonts(rel), `${page} loads /ui/${rel}, which src-tauri/src/server.rs (ASSETS) does not serve`);
      assert.ok(fs.existsSync(path.join(ui, rel)), `${page} loads /ui/${rel}, which does not exist`);
    }
  }
});
