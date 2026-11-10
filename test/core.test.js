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

















test('an uploaded picture becomes the face of a flat pet, in a window on its body', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const faced = ctx.creature('some project', 'working', '/pet-image/a.png?t=x'), plain = x => x.replace(/bx\d+/g, '');
  const id = faced.match(/<linearGradient id="(bx\d+)"/)[1];
  strict.match(faced, /^<svg class="flat" /, 'a flat pet');
  strict.ok(faced.includes(`<clipPath id="${id}c"><rect x="9.5" y="10" width="17" height="16" rx="5"/></clipPath>`), 'a rounded window');
  strict.ok(faced.includes(`<image href="/pet-image/a.png?t=x" x="9.5" y="10" width="17" height="16" clip-path="url(#${id}c)"`), 'the picture fills it');
  strict.doesNotMatch(faced, /crispEdges|var\(--/, 'no pixel steps, colours written out');
  strict.match(faced, /r="4\.7" fill="#5b8def"/, 'its status badge, like every pet');
  strict.notEqual(plain(ctx.creature('p', 'working', '/x.png')), plain(ctx.creature('p', 'working', '/x.png', undefined, false, ['typing', 1])), 'it acts its state');
});


test('flat-icon pets: a design per project, its session colour, a pose and a badge per state', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const theme = fs.readFileSync(path.join(__dirname, '../src/ui/theme.css'), 'utf8'), TOKENS = require('vm').runInContext('TOKENS', ctx);
  for (const [name, [L, C, h]] of Object.entries(TOKENS)) {
    const m = theme.match(new RegExp(`${name}:oklch\\(([\\d.]+)% ([\\d.]+) ([\\d.]+)\\)`));
    strict.deepEqual([+m[1] / 100, +m[2], +m[3]], [L, C, h], `${name} as in theme.css`);
  }
  const V = x => JSON.parse(JSON.stringify(x));
  const designOf = require('vm').runInContext('designOf', ctx);
  strict.equal(designOf('bobix'), 'default', "Bobix's own mascot is the default design");
  const designs = new Set(Array.from({ length: 60 }, (_, i) => designOf(`/code/p${i}`)));
  strict.deepEqual([...designs].sort(), ['default', 'miso', 'nota', 'ship', 'tty'], 'every design is picked by some project');
  // Every state is acted: its own pose loop, done waves then jumps, a working pet walks while the line moves.
  strict.deepEqual(['working', 'waiting', 'idle', 'failed'].map(st => ctx.poseAt(st)[0]), ['typing', 'waiting', 'asleep', 'failed']);
  strict.deepEqual([ctx.poseAt('working', 0, 'R')[0], ctx.poseAt('working', 0, 'L')[0], ctx.poseAt('waiting', 0, 'R')[0]], ['walkR', 'walkL', 'waiting'], 'only a working pet walks');
  strict.deepEqual([0, 150, 450, 600, 1050, 1200].map(t => ctx.poseAt('done', t).join()), ['wave,0', 'wave,1', 'wave,3', 'jump,0', 'jump,3', 'wave,0']);
  strict.deepEqual([0, 220, 880].map(t => ctx.poseAt('waiting', t)[1]), [0, 1, 0], '4 frames of 220 ms');
  const R = V(ctx.flatPose('walkR', 0)), L = V(ctx.flatPose('walkL', 0));
  strict.deepEqual([L.rot, L.look, L.hands[0][0], L.feet], [-R.rot, -R.look, 36 - R.hands[1][0], [R.feet[1], R.feet[0]]], 'walking left mirrors walking right');
  // The pictures: whole SVGs with every colour written out (they also draw into a canvas), a gradient of their own (an
  // id per picture), a badge per status, the coral ring only while it waits for you.
  const svg = (st, pose) => ctx.creature('/code/web', st, null, 1, false, pose), plain = x => x.replace(/bx\d+/g, '');
  for (const st of ['working', 'waiting', 'done', 'idle', 'failed']) {
    const x = svg(st), id = x.match(/<linearGradient id="(bx\d+)"/)[1];
    strict.match(x, /^<svg class="flat" xmlns="http:\/\/www\.w3\.org\/2000\/svg" viewBox="0 0 36 36"[^>]*>.*<\/svg>$/);
    strict.doesNotMatch(x, /var\(--|crispEdges/, `${st}: colours written out`);
    strict.ok(x.includes(`url(#${id})`), `${st}: the body takes its gradient`);
    strict.equal(/class="ring"/.test(x), st === 'waiting', `${st}: the ring`);
  }
  const badge = st => (svg(st).match(/r="4\.7" fill="(#[0-9a-f]{6})"/) || [])[1];
  strict.deepEqual(['working', 'waiting', 'done', 'failed', 'idle'].map(badge), ['#5b8def', '#ff7a4d', '#34c38f', '#e5484d', undefined], 'a badge per status, none asleep');
  strict.notEqual(svg('working').match(/id="(bx\d+)"/)[1], svg('working').match(/id="(bx\d+)"/)[1], 'a gradient id per picture');
  strict.equal(new Set(['working', 'waiting', 'done', 'idle'].map(st => plain(svg(st)))).size, 4, 'each state looks its own');
  strict.notEqual(plain(svg('waiting', ['waiting', 0])), plain(svg('waiting', ['waiting', 1])), 'and moves from frame to frame');
  strict.equal(plain(svg('working')), plain(svg('working', ['typing', 0])), 'without a pose: the first frame');
});

test('effects start on the right changes, play once, and are drawn as flat shapes', () => {
  const ctx = {};
  require('vm').runInNewContext(['creature.js', 'pet-fx.js'].map(f => fs.readFileSync(path.join(__dirname, '../src/ui', f), 'utf8')).join('\n'), ctx);
  const run = code => JSON.parse(require('vm').runInContext(`JSON.stringify((() => { ${code} })())`, ctx));
  ctx.EV = (prev, s) => JSON.parse(JSON.stringify(ctx.fxEvents(prev, s)));
  const s = (o) => ({ id: 'a', state: 'working', lv: 1, xp: 0, ctx: 80, parent: null, tool: '', ...o });
  const kinds = (prev, now) => ctx.EV(prev, now).map(e => e[0]);
  strict.deepEqual(kinds(s(), s()), [], 'no change, no effect');
  strict.deepEqual(kinds(s(), s({ state: 'waiting' })), [], 'waiting shows with the ring and badge, not an effect');
  strict.deepEqual(ctx.EV(s({ xp: 1 }), s({ state: 'done', xp: 2 })), [['xp', { f0: 1 / 3, f1: 2 / 3, lv: 1 }]], 'a finished task: +1 XP');
  strict.deepEqual(ctx.EV(s({ xp: 2 }), s({ state: 'done', lv: 2, xp: 0 })), [['xp', { f0: 2 / 3, f1: 1, lv: 2 }]], 'the third one: a level');
  strict.deepEqual(kinds(s({ state: 'done' }), s({ state: 'done', since: 2 })), [], 'still done: no new XP');
  strict.deepEqual(kinds(s({ ctx: 30 }), s({ ctx: 24 })), ['ctx'], 'context drops under 25%');
  strict.deepEqual(kinds(s({ ctx: 20 }), s({ ctx: 10 })), [], 'already low: once');
  strict.deepEqual(kinds(s({ ctx: null }), s({ ctx: null })), [], 'unknown context is not low');
  strict.deepEqual(kinds(null, s({ ctx: 12 })), ['ctx'], 'a pet that shows up low');
  strict.deepEqual(kinds(null, s({ parent: 'p' })), [], 'a new side session just walks in behind its parent');
  const Q = (q, playing, kind, o) => JSON.parse(JSON.stringify(ctx.fxQueue(q, playing, kind, o)));
  strict.deepEqual(Q([['xp', { f0: 0, f1: 1 / 3, lv: 1 }]], null, 'xp', { f0: 1 / 3, f1: 2 / 3, lv: 1 }), [['xp', { f0: 0, f1: 2 / 3, lv: 1 }]]);
  strict.deepEqual(Q([['ctx']], null, 'xp', { f0: 0, f1: 1 / 3, lv: 1 }).map(e => e[0]), ['ctx', 'xp']);
  // Every effect runs, draws and ends on time; the same run draws the same frames (a fixed-step simulation).
  const r = run(`return Object.keys(FX).map(kind => {
    const go = () => { const r = fxRun(kind, { f0: 2 / 3, f1: 1, lv: 7 }), frames = []; for (let i = 0; i < 400 && !r.done; i++) { r.step(1 / 30); frames.push(r.draw().svg()); } return [r.done, frames]; };
    const [done, a] = go(), [, b] = go(), all = a.join('');
    return [kind, done, a.some(f => f), a.join() === b.join(), /crispEdges|pixelated|shape-rendering/.test(all), [...new Set(all.match(/<[a-z]+/g))].sort()];
  })`);
  for (const [kind, done, drew, same, pixel, tags] of r) {
    strict.deepEqual([done, drew, same, pixel], [true, true, true, false], `${kind}: ends, draws, repeats exactly, nothing pixel-style`);
    strict.ok(tags.every(t => ['<circle', '<rect', '<path', '<ellipse', '<text', '<g'].includes(t)), `${kind}: flat shapes only (${tags})`);
  }
  // Mirrored for the top edge: the frame is flipped top-down, the text in it stays upright.
  const flip = run(`const r = fxRun('xp', { f0: 2 / 3, f1: 1, lv: 7, flipY: true }); let svg = ''; for (let i = 0; i < 100; i++) { r.step(1 / 30); const s = r.draw().svg(); if (s.includes('LV 7')) { svg = s; break; } } return svg;`);
  strict.match(flip, new RegExp(`^<g transform="matrix\\(1 0 0 -1 0 ${require('vm').runInContext('GH', ctx)}\\)">`));
  strict.match(flip, /<g transform="translate\([\d.]+ [\d.]+\) scale\(1 -1\) translate\(-[\d.]+ -[\d.]+\)"><text [^>]*>LV 7<\/text><\/g>/, 'its "LV 7" upright');
});

// The overlay itself is checked pixel by pixel in a browser (overlay.test.js); here the drawing side: an effect drawn for
// a room never draws outside it, and what does not fit is left out whole.
test('an effect drawn for its room stays inside it', () => {
  const ctx = {};
  require('vm').runInNewContext(['creature.js', 'pet-fx.js'].map(f => fs.readFileSync(path.join(__dirname, '../src/ui', f), 'utf8')).join('\n'), ctx);
  const r = JSON.parse(require('vm').runInContext(`JSON.stringify((() => {
    const res = {};
    // A packed row: 10 cells either side of the pet's column, 20 above its feet (the top edge's name tag), 3 below; and the room it wants.
    for (const [kind, o] of [['xp', { f0: 2 / 3, f1: 1, lv: 9 }], ['ctx', {}], ['end', {}]]) for (const tight of [true, false]) {
      const run = fxRun(kind, o), room = tight ? [run.ox - 10, FEET - 20, run.ox + 10, FEET + 3] : [0, 0, GW, GH];
      let outside = 0, drawn = 0;
      for (let f = 0; f < 140 && !run.done; f++) { run.step(1 / 30); for (const b of run.draw(room).boxes) { drawn++; if (b[0] < room[0] - 1e-9 || b[1] < room[1] - 1e-9 || b[2] > room[2] + 1e-9 || b[3] > room[3] + 1e-9) outside++; } }
      res[kind + (tight ? '-tight' : '')] = { outside, drawn };
    }
    return res;
  })())`, ctx));
  for (const [k, v] of Object.entries(r)) { assert.equal(v.outside, 0, `${k} drew outside its room`); assert.ok(v.drawn > 0, `${k} drew nothing`); }
  // The rect maths the clip is made of.
  assert.deepEqual(JSON.parse(JSON.stringify(ctx.rectMinus([0, 0, 10, 10], [3, 3, 6, 6]))), [[0, 0, 10, 3], [0, 6, 10, 10], [0, 3, 3, 6], [6, 3, 10, 6]]);
  const hs = JSON.parse(JSON.stringify(ctx.fxHoles([0, 0, 100, 100], [40, 40, 60, 100], [[30, 50, 70, 70], [50, 60, 90, 80], [200, 0, 300, 10]])));
  const area = rs => rs.reduce((a, r) => a + (r[2] - r[0]) * (r[3] - r[1]), 0);
  assert.equal(area(hs), 20 * 10 + 10 * 20 + 30 * 20 - 10 * 10, 'the union of both minus the own pet, nothing counted twice, nothing outside the box');
  assert.equal(area(JSON.parse(JSON.stringify(ctx.fxHoles([0, 0, 100, 100], [40, 40, 60, 100], [], [[30, 50, 70, 70]])))), 40 * 20, 'a pet in front is cut out over its own pet too');
});

test('balloons are flat: a soft balloon with its icon, upright on either edge', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  for (const [kind, fill] of [['waiting', '#ff7a4d'], ['done', '#34c38f'], ['ctx', '#f6c445']]) for (const down of [false, true]) {
    const b = ctx.balloonSVG(kind, down);
    strict.match(b, /^<svg viewBox="0 0 11 22" overflow="visible"/);
    strict.ok(b.includes(`<ellipse cx="5.5" cy="6.4" rx="5.2" ry="6.2" fill="${fill}"/>`), `${kind}: its colour`);
    strict.doesNotMatch(b, /crispEdges|var\(--|width="1" height="1"/, `${kind}: no pixels`);
    strict.equal(b.includes('matrix(1 0 0 -1 0 22)'), down, 'flipped on the top edge');
    if (down) strict.ok(b.endsWith('</g>' + ctx.balloonParts(kind).icon(5.5, 15.6) + '</svg>'), 'the icon outside the flip, so it stays upright');
  }
});

// The overlay itself is checked pixel by pixel in a browser (overlay.test.js); here the drawing side: an effect drawn for
// a room never draws outside it, and a tight room shrinks it instead of cutting it.

test('sessions of one project share a shape; colours tell them apart', () => {
  const ctx = {};
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const fill = svg => [...new Set((svg.match(/#[0-9a-f]{6}/g) || []))].sort().join();
  const shape = svg => svg.replace(/(fill|stroke|stop-color)="[^"]*"/g, '').replace(/bx\d+/g, '');
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
  const plain = x => x.replace(/bx\d+/g, ''), pixel = plain(ctx.creature('p', 'idle', '/a.png', undefined, true)), smooth = plain(ctx.creature('p', 'idle', '/a.png'));
  strict.match(pixel, /<image href="\/a\.png" [^>]* style="image-rendering:pixelated"\/>/);
  strict.doesNotMatch(smooth, /image-rendering/);
  strict.equal(pixel.replace(' style="image-rendering:pixelated"', ''), smooth, 'only the smoothing differs');
});
test('picture pets are off: a project with a saved picture shows its flat pet', () => {
  const src = fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx = { URLSearchParams, location: { search: '?t=tok' } };
  strict.equal((src.match(/^const PICTURE_PETS = (true|false);$/m) || [])[1], 'false', 'one switch, off');
  require('vm').runInNewContext(src, ctx);
  const plain = x => x.replace(/bx\d+/g, ''), s = { id: 's1', project: '/p', image: '0123456789abcdef01234567.gif', pixel: true, whole: true };
  strict.equal(plain(ctx.petArt(s, 'working', 2)), plain(ctx.creature('/p', 'working', null, 2)));
  strict.doesNotMatch(ctx.petArt(s, 'working', 2), /pet-image/);
  const panel = fs.readFileSync(path.join(__dirname, '../src/ui/panel.html'), 'utf8');
  for (const act of ['pic', 'picEdit', 'picReset']) strict.ok(panel.includes(`PICTURE_PETS && ${act === 'picReset' ? "d.act === 'picReset'" : "(d.act === 'pic' || d.act === 'picEdit')"}`), `${act}: behind the switch`);
  strict.match(panel, /\$\{PICTURE_PETS \? `<button class="btn" type="button" data-act="pic">/, 'the buttons only with it on');
  strict.match(panel, /\.\.\.\(PICTURE_PETS \? \[\[T\('pane\.petPick'\)/, 'the action and its keys only with it on');
});
// With the switch on (the redo), pictures are worn as before.
test('a pixel-art picture is the whole pet when the person chose so, else the face in the pet', () => {
  const ctx = { URLSearchParams, location: { search: '?t=tok' } };
  require('vm').runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8').replace('const PICTURE_PETS = false;', 'const PICTURE_PETS = true;'), ctx);
  const s = { id: 's1', project: '/p', image: '0123456789abcdef01234567.gif', pixel: true }, plain = x => x.replace(/bx\d+/g, '');
  const url = '/pet-image/0123456789abcdef01234567.gif?t=tok';
  const whole = ctx.petArt({ ...s, whole: true }, 'waiting', 2);
  strict.equal(whole, ctx.picturePet(url, 'waiting'));
  strict.match(whole, /^<svg class="flat pic" [^>]*viewBox="0 0 36 36"[^>]*><rect class="ring" [^>]*\/><image href="\/pet-image\/0123456789abcdef01234567\.gif\?t=tok" x="2" y="2" width="32" height="32" preserveAspectRatio="xMidYMax meet" style="image-rendering:pixelated"\/><circle cx="29\.6" cy="6\.4" r="4\.7" fill="#ff7a4d"/,
    'the picture, with the waiting ring and badge');
  strict.doesNotMatch(ctx.picturePet(url, 'idle'), /class="ring"|r="4\.7"/, 'asleep: nothing round it');
  strict.equal(plain(ctx.petArt(s, 'working', 2)), plain(ctx.creature('/p', 'working', url, 2, true)), 'not chosen: the face in the body');
  strict.equal(plain(ctx.petArt({ ...s, pixel: false, whole: true }, 'working', 2)), plain(ctx.creature('/p', 'working', url, 2, false)), 'a picture from before pixel art stays a face');
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

test('board: minutes Claude waited for you today, per project, in the report', () => {
  const B = require('../src/ui/board');
  const now = new Date(2026, 10, 8, 17, 0).getTime(), min = 60e3, midnight = new Date(2026, 10, 8).getTime();
  const waits = [{ from: now - 30 * min, to: now - 18 * min, project: 'shop' }, { from: midnight - 10 * min, to: midnight + 5 * min, project: 'bobix' },
    { from: midnight - 60 * min, to: midnight - 30 * min, project: 'api' }, { from: now - 20 * 1e3, to: now - 10 * 1e3, project: 'api' }];
  const ss = [{ id: 'a', state: 'waiting', since: now - 3 * min, project: '/src/shop' }, { id: 'b', state: 'working', since: now - 50 * min, cwd: 'C:\\src\\bobix' },
    { id: 'c', state: 'waiting', since: now - 9 * min, cwd: '/src/bobix/sub', project: '/src/bobix' }];
  assert.deepEqual(B.waited(waits, ss, now), [{ project: 'shop', mins: 15 }, { project: 'bobix', mins: 14 }],
    'ended waits plus the ones going on; only since midnight; under half a minute left out');
  assert.deepEqual(B.waited([], [], now), []);
  const I18N = require('../src/ui/i18n');
  const T = (k, v) => I18N.t('vi', k, v), none = { done: [], verify: [] };
  assert.equal(B.report(none, T, now, 'vi-VN', B.waited(waits, ss, now)), 'Báo cáo ngày 8/11/2026\n\nHôm nay chưa có việc xong hay chờ kiểm tra.\n\nClaude đã chờ bạn tổng cộng 29 phút hôm nay:\n- shop: 15 phút\n- bobix: 14 phút\n');
  assert.equal(B.report(none, T, now, 'vi-VN', []), 'Báo cáo ngày 8/11/2026\n\nHôm nay chưa có việc xong hay chờ kiểm tra.\n\nHôm nay Claude chưa phải chờ bạn.\n');
  assert.equal(B.folder({ cwd: 'C:\\src\\bobix' }), 'bobix');
});

test('board: stuck work, one item each, past the limits in Settings', () => {
  const B = require('../src/ui/board');
  const now = Date.UTC(2026, 10, 9, 10), H = 3600e3, iso = t => new Date(t).toISOString();
  const git = (branch, own, ago) => ({ branch, own, last: { t: now - ago * H, subject: 'x' } });
  const sessions = [
    { id: 's1', state: 'done', since: now - 3 * H, updated: now - 3 * H, git: git('1-cart', 2, 5) }, // finished 3 h ago, no PR
    { id: 's2', state: 'done', since: now - 1 * H, updated: now - 1 * H }, // finished 1 h ago: not yet
    { id: 's3', state: 'idle', since: now - 5 * H, updated: now - 5 * H, git: git('3-login', 0, 9) }, // resting 5 h
    { id: 's4', state: 'working', since: now - 9 * H, updated: now }, // works: never stuck
    { id: 's5', state: 'done', since: now - 9 * H, updated: now - 9 * H }, // its issue has a PR
    { id: 's6', state: 'done', since: now - 7 * H, updated: now - 7 * H, git: git('feat/cache', 4, 6) }, // no issue, 4 commits, 6 h
    { id: 's7', state: 'idle', since: 0, updated: 0, git: git('main', 0, 50) },
    { id: 's8', state: 'waiting', since: now, updated: now, git: git('feat/busy', 3, 9) },
    { id: 's9', state: 'idle', since: 0, updated: 0, git: git('feat/new', 3, 1) },
  ];
  const on = (...ids) => ids.map(id => ({ id, why: 'branch' }));
  const model = {
    issues: [
      { project: 'g/app', iid: 1, title: 'Cart', sessions: on('s1'), reviews: [] }, { project: 'g/app', iid: 2, title: 'Two', sessions: on('s2'), reviews: [] },
      { project: 'g/app', iid: 3, title: 'Login', sessions: on('s3'), reviews: [] }, { project: 'g/app', iid: 4, title: 'Four', sessions: on('s4', 's3'), reviews: [] },
      { project: 'g/app', iid: 5, title: 'Five', sessions: on('s5'), reviews: [{ url: 'https://h/g/app/pull/9', why: 'closes' }] },
      { project: 'g/app', iid: 6, title: 'Six', sessions: [], reviews: [] },
    ],
    reviews: [
      { kind: 'pr', url: 'https://h/g/app/pull/9', project: 'g/app', branch: 'r-branch', created: iso(now - 26 * H), reviewed: false, issues: [{ project: 'g/app', iid: 5, why: 'closes' }] },
      { kind: 'mr', url: 'https://h/g/web/-/merge_requests/3', project: 'g/web', branch: 'x', created: iso(now - 30 * H), reviewed: true, issues: [] },
      { kind: 'pr', url: 'https://h/g/app/pull/10', project: 'g/app', branch: 'y', created: iso(now - 1 * H), reviewed: false, issues: [] },
      { kind: 'pr', url: 'https://h/g/app/pull/11', project: 'g/app', branch: 'z', created: null, reviewed: false, issues: [] },
    ],
    branches: [['1-cart', 's1'], ['3-login', 's3'], ['feat/cache', 's6'], ['main', 's7'], ['feat/busy', 's8'], ['feat/new', 's9'], ['r-branch', 's5']]
      .map(([name, s]) => ({ project: 'g/app', name, issue: null, sessions: [s], reviews: [] })),
  };
  model.branches.find(b => b.name === 'r-branch').sessions = ['s5'];
  sessions.find(s => s.id === 's5').git = git('r-branch', 2, 40);
  const got = B.stuck(model, sessions, undefined, now);
  assert.deepEqual(got.map(x => [x.kind, x.iid ?? x.title]), [['nopr', 1], ['noreview', 5], ['branch', 'feat/cache'], ['idle', 3]],
    'finished with no PR, PR not reviewed, branch with commits and no PR, resting; nothing that works, waits, is too new, has a PR or no commits of its own');
  assert.deepEqual([got[0].branch, got[0].session, got[0].limit, got[0].ms], ['1-cart', 's1', 2, 3 * H]);
  assert.deepEqual([got[1].url, got[1].review, got[2].commits, got[3].session], ['https://h/g/app/pull/9', 'pr', 4, 's3']);
  assert.deepEqual(B.stuck(model, sessions, { stuckOn: false }, now), []);
  assert.deepEqual(B.stuck(model, sessions, { stuckNoPr: 4, stuckNoReview: 48, stuckBranch: 5, stuckIdle: 6 }, now).map(x => [x.kind, x.title]), [['branch', 'feat/cache'], ['branch', '1-cart']],
    'longer limits: a finished session not yet listed leaves its branch to the branch rule; the oldest first');
  assert.deepEqual(B.stuck(null, sessions, undefined, now), []);
  assert.ok(got.every(x => !x.old) && got[0].at === now - 3 * H, 'all of these moved within 14 days');
});

test('board: stuck work quiet for weeks is old, and a skip lasts until the item moves again', () => {
  const B = require('../src/ui/board');
  const now = Date.UTC(2026, 10, 9, 10), H = 3600e3, D = 24 * H, iso = t => new Date(t).toISOString();
  const sessions = [{ id: 's1', state: 'done', since: now - 3 * H, updated: now - 3 * H }, { id: 'o1', state: 'idle', since: now - 30 * D, updated: now - 30 * D, git: { branch: 'feat/old', own: 2, last: { t: now - 31 * D } } }];
  const model = {
    issues: [{ project: 'g/a', iid: 1, title: 'New', sessions: [{ id: 's1' }], reviews: [] }],
    reviews: [{ kind: 'pr', url: 'u/pull/1', project: 'g/a', branch: 'x', created: iso(now - 532 * D), updated: iso(now - 470 * D), reviewed: false, issues: [] },
      { kind: 'pr', url: 'u/pull/2', project: 'g/a', branch: 'y', created: iso(now - 40 * D), updated: iso(now - 2 * D), reviewed: false, issues: [] }],
    branches: [{ project: 'g/a', name: 'feat/old', issue: null, sessions: ['o1'], reviews: [] }],
  };
  const pick = xs => xs.map(x => [x.id, x.old]);
  assert.deepEqual(pick(B.stuck(model, sessions, undefined, now)), [['nopr:g/a#1', false], ['noreview:u/pull/1', true], ['noreview:u/pull/2', false], ['branch:g/a:feat/old', true]],
    'quiet for over 14 days (by its last change, not its age): old');
  assert.deepEqual(pick(B.stuck(model, sessions, { stuckMaxAge: 60 }, now)).filter(x => x[1]).map(x => x[0]), ['noreview:u/pull/1'], 'the limit is a setting (days)');
  const all = B.stuck(model, sessions, undefined, now), at = id => all.find(x => x.id === id).at;
  const skipped = { 'nopr:g/a#1': at('nopr:g/a#1'), 'noreview:u/pull/2': at('noreview:u/pull/2') - 1 };
  assert.deepEqual(B.stuck(model, sessions, { stuckSkipped: skipped }, now).map(x => x.id), ['noreview:u/pull/1', 'noreview:u/pull/2', 'branch:g/a:feat/old'],
    'skipped: gone while nothing moved; one that changed since it was skipped comes back');
  sessions[0].updated = now; // the session moved again
  assert.ok(B.stuck(model, sessions, { stuckSkipped: skipped, stuckNoPr: 0 }, now).some(x => x.id === 'nopr:g/a#1'));
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
  assert.deepEqual(B.skillFor(skills, ['bug'], ''), { name: 'fix-ticket', label: 'bug', role: undefined }, "a label's skill; the repo's own wins a tie");
  assert.deepEqual(B.skillFor(skills, ['bug'], 'dev'), { name: 'fix-ticket', label: 'bug', role: 'dev' }, 'label and role both');
  assert.deepEqual(B.skillFor(skills, [], 'qa'), { name: 'ba-qa-kit:write-test-cases', label: undefined, role: 'qa' }, 'the role alone');
  assert.deepEqual(B.skillFor(skills, ['enhancement'], 'ba'), { name: 'ba-qa-kit:write-stories', label: 'enhancement', role: 'ba' }, 'label and role together score most');
  assert.equal(B.skillFor(skills, ['design'], ''), null, 'a label with no matching words: no skill');
  assert.equal(B.skillFor(skills, [], ''), null);
  assert.equal(B.skillFor([sk('docx')], ['bug'], 'dev'), null, 'only words in the name count');
  const text = '#53 T\n\nbody\n\nhttps://h/i/53';
  const one = B.withSkill(text, 'fix-ticket', 'https://h/i/53');
  assert.equal(one, '/fix-ticket https://h/i/53\n\n#53 T\n\nbody\n\nhttps://h/i/53');
  assert.equal(B.withSkill(one, 'ak:cook', 'https://h/i/53'), '/ak:cook https://h/i/53\n\n' + text, 'another skill replaces the first line');
  assert.equal(B.withSkill(one, '', 'https://h/i/53'), text, 'no skill');
});

test('board: a process replaces the rules; without one the team first, then your pins', () => {
  const B = require('../src/ui/board');
  const sk = (name, source) => ({ name, source, description: '' });
  const skills = [sk('fix-ticket', 'project'), sk('verify-change', 'project'), sk('ba-qa-kit:write-test-cases', 'team'), sk('ak:cook', 'personal'), sk('ak:fix', 'personal'), sk('docx', 'synced')];
  const process = { rules: [{ when: { label: ['bug'] }, skill: 'deploy-fix' }, { when: { label: ['bug'] }, skill: 'verify-change' }, { when: { role: 'qa' }, skill: 'ba-qa-kit:write-test-cases' },
    { when: { stage: 'verify' }, skill: 'fix-ticket' }, { when: { label: ['enhancement'], role: 'dev' }, skill: 'ak:cook' }], install: { 'deploy-fix': 'Install deploy-fix from the team wiki' } };
  assert.deepEqual(B.suggest(skills, { labels: ['Bug'], role: 'qa', process }), { pick: { name: 'verify-change', from: 'process', label: 'bug', role: undefined, stage: undefined },
    missing: [{ name: 'deploy-fix', hint: 'Install deploy-fix from the team wiki' }] }, "the first fitting rule with an installed skill; the one Claude lacks is listed with the process's hint");
  assert.equal(B.suggest(skills, { labels: [], role: 'qa', process }).pick.name, 'ba-qa-kit:write-test-cases');
  assert.equal(B.suggest(skills, { labels: [], role: 'dev', process }).pick, null, 'a process with no fitting rule suggests nothing (the label / role rules are not used)');
  assert.equal(B.suggest(skills, { labels: ['enhancement'], role: 'dev', process }).pick.name, 'ak:cook', "a process may name one of your own skills");
  assert.equal(B.suggest(skills, { labels: [], stage: 'verify', process }).pick.name, 'fix-ticket');
  assert.equal(B.suggest(skills, { labels: ['bug'], role: 'dev' }).pick.name, 'fix-ticket', 'no process: the team first');
  assert.deepEqual(B.suggest(skills, { labels: [], role: 'ba', pins: { 'ak:cook': ['ba'], 'ak:fix': [] } }).pick, { name: 'ak:cook', from: 'pin', role: 'ba' }, 'no team skill fits: a skill you pinned for your role');
  assert.equal(B.suggest(skills, { labels: [], role: 'dev', pins: { 'ak:cook': ['dev'] } }).pick.name, 'fix-ticket', "a team skill that fits comes before your pin");
  assert.equal(B.suggest(skills.filter(s => s.source !== 'project'), { labels: ['bug'], role: '', pins: { 'ak:fix': [] } }).pick.name, 'ak:fix', 'then your pins by label');
  assert.equal(B.suggest(skills, { labels: ['bug'], role: '', pins: {} }).pick.name, 'fix-ticket');
  assert.equal(B.suggest([sk('docx', 'synced')], { labels: ['bug'] }).pick, null, 'skills you did not pin are not suggested');
  assert.equal(B.installHint('ba-qa-kit:write-test-cases'), '/plugin install ba-qa-kit');
  assert.equal(B.installHint('fix-ticket'), '');
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

test('board: the step a ticket is at and why, two sessions on one piece of work, and the stand-up', () => {
  const B = require('../src/ui/board');
  const I18N = require('../src/ui/i18n');
  const T = (k, v) => I18N.t('vi', k, v);
  // "ask" is listed with the tickets in progress
  const work = { status: 'ok', local: [{ path: 'g/app', dir: '/src/app', issues: [{ iid: 1, title: 'a', stage: 'ask', sessions: ['w'] }, { iid: 2, title: 'b', stage: 'doing', sessions: ['x'] }] }], closed: [] };
  assert.deepEqual(B.stages(work, [{ id: 'w', st: 'waiting' }, { id: 'x', st: 'working' }]).doing.map(([i]) => i.iid), [1, 2]);
  // why
  const s = (id, st, extra = {}) => ({ id, st, ...extra });
  assert.equal(B.why({ stage: 'verify', review: 'https://github.com/a/b/pull/62' }, [], T), 'có PR #62 đang mở');
  assert.equal(B.why({ stage: 'verify', review: 'https://gl.example.com/g/p/-/merge_requests/41' }, [], T), 'có MR !41 đang mở');
  assert.equal(B.why({ stage: 'ask' }, [s('a', 'waiting'), s('b', 'waiting'), s('c', 'working')], T), '2 yêu cầu đang chờ bạn duyệt');
  assert.equal(B.why({ stage: 'doing' }, [s('a', 'working', { git: { branch: 'feat/7-x' } })], T), '1 phiên đang làm trên nhánh feat/7-x');
  assert.equal(B.why({ stage: 'doing' }, [s('a', 'done'), s('b', 'idle')], T), 'các phiên đã dừng, chưa có PR hay MR');
  assert.equal(B.why({ stage: 'todo' }, [], T), 'chưa có phiên nào');
  assert.equal(B.why({ closedAt: new Date(2026, 10, 9, 14, 5).toISOString() }, [], T, 'vi-VN'), 'đóng lúc 14:05');
  assert.deepEqual(['todo', 'doing', 'ask', 'verify'].map(stage => B.stepOf({ stage })).concat(B.stepOf({ closedAt: 'x' })), B.STEPS);
  // duplicates: a side session goes with its parent; a resting session counts for nothing; one issue warning, not two
  const model = { issues: [{ project: 'g/app', iid: 1, sessions: [{ id: 'p' }, { id: 'kid' }] }, { project: 'g/app', iid: 2, sessions: [{ id: 'q' }, { id: 'r' }] }, { project: 'g/app', iid: 3, sessions: [{ id: 'u' }, { id: 'zz' }] }],
    branches: [{ project: 'g/app', name: '2-x', issue: { project: 'g/app', iid: 2 }, sessions: ['q', 'r'] }, { project: 'g/app', name: 'main', issue: null, sessions: ['m1', 'm2', 'zz'] }, { project: 'g/app', name: 'feat/a', issue: null, sessions: ['p', 'kid'] }] };
  const ss = [s('p', 'working'), s('kid', 'waiting', { parent: 'p' }), s('q', 'working'), s('r', 'done'), s('u', 'working'), s('zz', 'idle'), s('m1', 'working'), s('m2', 'waiting')];
  const d = B.dups(model, ss);
  assert.deepEqual([...d.issues], [['g/app#2', 2]], 'parent + side session is one; a resting one is none');
  assert.deepEqual([...d.sessions], [['m1', { branch: 'main', with: 'm2' }], ['m2', { branch: 'main', with: 'm1' }]], 'branch 2-x is covered by its issue warning');
  assert.deepEqual(B.dups({}, ss), { issues: new Map(), sessions: new Map() });
  // stand-up: the last working day, today's tickets, what is stuck
  const mon = new Date(2026, 10, 9, 9, 0).getTime(); // a Monday
  assert.equal(B.lastWorkday(mon), '2026-11-06', 'Monday looks back to Friday');
  assert.equal(B.lastWorkday(new Date(2026, 10, 11, 9).getTime()), '2026-11-10');
  const fri = new Date(2026, 10, 6, 16, 0).toISOString(), sun = new Date(2026, 10, 8, 10, 0).toISOString();
  const st = { doing: [[{ iid: 1, title: 'a', stage: 'ask' }, { path: 'g/app' }], [{ iid: 2, title: 'b', stage: 'doing' }, { path: 'g/app' }]], verify: [[{ iid: 3, title: 'c', stage: 'verify' }, { path: 'g/web' }]] };
  const closed = [{ iid: 9, title: 'x', project: 'g/app', closedAt: fri }, { iid: 9, title: 'x', project: 'g/app', closedAt: fri }, { iid: 8, title: 'y', project: 'g/app', closedAt: sun }];
  assert.equal(B.standup(st, closed, [{ text: '[app] #1 a', why: 'chờ bạn duyệt 5 phút' }], T, mon, 'vi-VN'),
    'Standup 9/11/2026\n\nHôm qua xong:\n- [app] #9 x\n\nHôm nay đang làm:\n- [app] #1 a (Chờ duyệt)\n- [app] #2 b\n- [web] #3 c (Chờ kiểm tra)\n\nĐang kẹt:\n- [app] #1 a: chờ bạn duyệt 5 phút\n');
  assert.match(B.standup({ doing: [], verify: [] }, [], [], T, mon, 'vi-VN'), /Hôm qua xong:\n- không có\n\nHôm nay đang làm:\n- không có\n\nĐang kẹt:\n- không có\n$/);
});

test('habits: days of activity, the first-days state, and each suggestion rule', () => {
  const Hb = require('../src/ui/habits');
  const at = (d, h, m = 0) => new Date(2026, 10, d, h, m).getTime(); // November 2026; the 9th is a Monday
  const now = at(9, 9, 5), D = 864e5;
  const ev = (t, k, more = {}) => ({ t, k, s: more.s || 's' + t, d: '/src/shop', p: 'team/shop', ...more });
  // days: per session per local day, with approvals, waiting and skills
  const one = [ev(at(6, 14), 'start', { s: 'a', i: 128, b: '128-cart' }), ev(at(6, 14, 5), 'skill', { s: 'a', v: 'fix' }), ev(at(6, 15), 'ask', { s: 'a', ms: 120e3, ok: true, r: 'Bash(npm test:*)' }),
    ev(at(6, 17, 35), 'end', { s: 'a', i: 128 })];
  assert.deepEqual(Hb.days(one).map(x => [x.day, x.items.map(y => [y.iid, y.asks, y.waitMs, y.skills, y.ended, y.to - y.from])]), [['2026-11-06', [[128, 1, 120e3, ['fix'], true, (3 * 60 + 35) * 60e3]]]]);
  assert.deepEqual(Hb.days([...one, ev(at(6, 16), 'skill', { s: 'a', v: 'clear' })])[0].items[0].skills, ['fix'], 'a Claude Code command (/clear) is not a skill');
  assert.equal(Hb.days(one)[0].items[0].project, 'team/shop');
  // not ready while fewer than 5 days were logged
  assert.deepEqual(Hb.suggest(one, {}, {}, now), { ready: false, logged: 1, need: 5, cards: [] });
  const filler = [2, 3, 4, 5].map(d => ev(at(d, 10), 'st', { v: 'working', d: '/src/other' })); // other days, another folder
  // morning: the issue last worked on Friday is open, has no PR and nobody is on it
  const model = { repos: [{ path: 'team/shop', dirs: [] }], issues: [{ project: 'team/shop', iid: 128, title: 'Cart total', reviews: [], sessions: [] }] };
  const s1 = Hb.suggest([...filler, ...one], { model, sessions: [] }, {}, now);
  assert.equal(s1.ready, true);
  assert.deepEqual(s1.cards.map(c => [c.kind, c.iid, c.dir, c.at]), [['morning', 128, '/src/shop', at(6, 17, 35)]]);
  assert.equal(Hb.morning(one, model, [], at(9, 13)), null, 'only in the morning');
  assert.equal(Hb.morning(one, { ...model, issues: [{ ...model.issues[0], reviews: [{ url: 'u' }] }] }, [], now), null, 'a PR closes it: not unfinished');
  assert.equal(Hb.morning(one, { ...model, issues: [] }, [], now), null, 'closed (no longer open)');
  assert.equal(Hb.morning([...one, ev(at(9, 8), 'st', { i: 128, v: 'working' })], model, [], now), null, 'already worked on today');
  // allow: the same approval allowed again and again this week, never denied
  const asks = n => Array.from({ length: n }, (_, k) => ev(now - (k + 1) * 3600e3, 'ask', { s: 'w', ok: true, r: 'Bash(npm test:*)', tool: 'Bash' }));
  assert.deepEqual(Hb.allows(asks(23), now, 0).map(c => [c.what, c.count, c.rule, c.dir]), [['npm test', 23, 'Bash(npm test:*)', '/src/shop']]);
  assert.deepEqual(Hb.allows([...asks(23), ev(now - 3600e3, 'ask', { ok: false, r: 'Bash(npm test:*)' })], now, 0), [], 'denied once: no suggestion');
  assert.deepEqual(Hb.allows(asks(9), now, 0), []);
  assert.deepEqual(Hb.allows(asks(23), now, 2), [], '"Không cần" twice: needs 30');
  assert.deepEqual(Hb.allows(asks(23).map(e => ({ ...e, t: e.t - 8 * D })), now, 0), [], 'only this week');
  // recurring: the stand-up opened on Monday around 9:00, 4 weeks in a row
  const mondays = [12, 19, 26].map(d => new Date(2026, 9, d, 8, 55).getTime()).concat(new Date(2026, 10, 2, 9, 10).getTime()).map(t => ({ t, k: 'ui', v: 'standup' }));
  assert.deepEqual(Hb.recurring(mondays, now, 0).map(c => [c.what, c.weekday, Hb.hhmm(c.at), c.count]), [['standup', 1, '8:55', 4]]);
  assert.deepEqual(Hb.recurring(mondays, at(9, 11), 0), [], 'too late in the day');
  assert.deepEqual(Hb.recurring([...mondays, { t: at(9, 8, 58), k: 'ui', v: 'standup' }], now, 0), [], 'already opened today');
  assert.deepEqual(Hb.recurring(mondays.slice(1), now, 0).length, 1, '3 of the last 4 weeks are enough');
  assert.deepEqual(Hb.recurring(mondays.slice(2), now, 0), []);
  // skill: most sessions in a folder this week start with the same skill
  const sess = Array.from({ length: 11 }, (_, k) => [ev(now - k * 3600e3, 'start', { s: 'q' + k }), ...(k < 9 ? [ev(now - k * 3600e3 + 60e3, 'skill', { s: 'q' + k, v: 'fix' })] : [ev(now - k * 3600e3 + 60e3, 'skill', { s: 'q' + k, v: 'clear' })])]).flat();
  assert.deepEqual(Hb.skills(sess, now, 0, {}).map(c => [c.skill, c.count, c.of]), [['fix', 9, 11]]);
  assert.deepEqual(Hb.skills(sess, now, 0, { '/src/shop': 'fix' }), [], 'already chosen for that folder');
  assert.deepEqual(Hb.skills(sess.filter(e => e.v !== 'fix'), now, 0, {}), [], 'Claude Code commands are not skills');
  // all together: at most 3, the kinds turned down less first, a hidden one stays out
  const all = [...filler, ...one, ...asks(23), ...mondays, ...sess];
  const ids = cfg => Hb.suggest(all, { model, sessions: [] }, cfg, now).cards.map(c => c.kind);
  assert.deepEqual(ids({}), ['morning', 'allow', 'recurring']);
  assert.deepEqual(ids({ habitsNo: { morning: 1 } }), ['allow', 'recurring', 'skill', 'morning'].slice(0, 3));
  const morningId = Hb.suggest(all, { model, sessions: [] }, {}, now).cards[0].id;
  assert.deepEqual(ids({ habitsHidden: { [morningId]: now } }), ['allow', 'recurring', 'skill']);
  assert.deepEqual(ids({ habitsNo: { morning: 3 } }), ['allow', 'recurring', 'skill'], '"Không cần" 3 times: no more start-of-day');
});

test('tour: every step has its words in both languages, NEWS is this version, and the bubble stays on screen', () => {
  const TOUR = require('../src/ui/tour');
  const I18N = require('../src/ui/i18n');
  assert.ok(TOUR.STEPS.length >= 6 && TOUR.STEPS.length <= 12, 'a short tour');
  assert.equal(new Set(TOUR.STEPS.map(s => s.key)).size, TOUR.STEPS.length, 'one key per step (settings.tourSeen)');
  assert.deepEqual(TOUR.fresh(['flat', 'stuck', 'gone']).map(s => s.key), TOUR.STEPS.map(s => s.key).filter(k => k !== 'flat' && k !== 'stuck'), 'only the steps not seen yet');
  assert.equal(TOUR.fresh(TOUR.STEPS.map(s => s.key)).length, 0);
  assert.equal(TOUR.fresh(undefined).length, TOUR.STEPS.length);
  for (const s of TOUR.STEPS) {
    const TABS = ['overview', 'sessions', 'skills', 'settings', 'setup'];
    assert.ok(TABS.includes(s.tab) && s.at.length && (!s.alt || (TABS.includes(s.alt.tab) && s.alt.at.length)), s.key);
    assert.match(s.key, /^[a-z]{1,24}$/, 'a key settings.rs keeps');
    for (const lang of ['en', 'vi']) for (const k of ['title', 'body']) assert.ok(I18N.STR[lang][`tour.${s.key}.${k}`], `${lang} tour.${s.key}.${k}`);
  }
  assert.equal(I18N.NEWS, require('../package.json').version.split('-')[0], 'the tour opens once for this version');
  const on = (p, vw, vh, bh) => p.left >= 12 && p.top >= 12 && p.left + p.width <= vw - 12 && p.top + bh <= vh - 12;
  // wide: below the element when it fits, else above
  assert.deepEqual(TOUR.place({ left: 100, top: 100, width: 300, height: 40 }, 1440, 900, 380, 160), { left: 100, top: 152, width: 380 });
  assert.deepEqual(TOUR.place({ left: 1300, top: 800, width: 120, height: 40 }, 1440, 900, 380, 160), { left: 1048, top: 628, width: 380 });
  // a tall element: neither fits, it sits at the bottom
  const tall = TOUR.place({ left: 0, top: 20, width: 800, height: 860 }, 1440, 900, 380, 160);
  assert.ok(on(tall, 1440, 900, 160), JSON.stringify(tall));
  // no element: centred
  assert.deepEqual(TOUR.place(null, 1440, 900, 380, 160), { left: 530, top: 370, width: 380 });
  // narrow: across the bottom, or the top when the element is low
  assert.deepEqual(TOUR.place({ left: 10, top: 100, width: 300, height: 40 }, 420, 900, 380, 200), { left: 12, top: 688, width: 396 });
  assert.deepEqual(TOUR.place({ left: 10, top: 700, width: 300, height: 40 }, 420, 900, 380, 200), { left: 12, top: 12, width: 396 });
});

test('usage: one line, details and warnings from the numbers the app sends', () => {
  const U = require('../src/ui/usage');
  const I18N = require('../src/ui/i18n');
  const T = (k, v) => I18N.t('en', k, v);
  assert.deepEqual(['claude-opus-5-5', 'claude-haiku-4-5-20251001', 'claude-fable-5-1', 'claude-sonnet-5', 'gpt-x'].map(U.modelName), ['Opus 5.5', 'Haiku 4.5', 'Fable 5.1', 'Sonnet 5', 'gpt-x']);
  assert.deepEqual([999, 12_400, 3_450_000, 45_000_000].map(U.tokens), ['999', '12k', '3.5M', '45M']);
  assert.deepEqual([U.dur(25 * 60e3), U.dur(130 * 60e3), U.usd(4.5), U.usd(1234.4)], ['25m', '2h 10m', '$4.50', '$1,234']);
  const u = { turns: 40, mainTurns: 30, tokens: { input: 100, output: 20_000, cacheWrite: 50_000, cacheRead: 3_000_000 }, activeMs: 80 * 60e3, ctx: 320_000,
    reread: 0.98, rereadCost: 0.78, times: 3.14, cost: { usd: 42.4, date: '2026-10-06', partial: false },
    models: [{ model: 'claude-opus-5-5', side: false, turns: 29 }, { model: 'claude-sonnet-5-5', side: false, turns: 1 }, { model: 'claude-haiku-5-5', side: true, turns: 10 }],
    warnings: [{ kind: 'long', why: 'context', ctx: 320_000, limit: 300_000 }, { kind: 'light', why: 'output', out: 150, turns: 10, model: 'claude-opus-5-5' }, { kind: 'explore', n: 9, model: 'claude-opus-5-5' }] };
  assert.equal(U.line(u, T, {}), '3.1× your usual session · 1h 20m active · ≈ $42 API est.');
  assert.equal(U.line(u, T, { usageCost: false }), '3.1× your usual session · 1h 20m active', 'cost hidden by the switch');
  assert.equal(U.line({ ...u, times: null, cost: undefined }, T, {}), '30 turns · 1h 20m active', 'no normal yet, no cost');
  const facts = Object.fromEntries(U.facts(u, T, {}));
  assert.equal(facts.Models, 'Opus 5.5 97%, Sonnet 5.5 3%, Haiku 5.5 (subagents) 100%');
  assert.equal(facts.Cost, '≈ $42 API-equivalent estimate, prices of 2026-10-06');
  assert.equal(facts['Re-reading context'], '98% of input tokens, 78% of the estimate');
  const w = U.warns(u, T);
  assert.deepEqual(w.map(x => x.kind), ['long', 'light', 'explore']);
  assert.match(w[0].text, /re-reads 320k tokens.*warns at 300k.*\/compact/);
  assert.match(w[1].text, /^Opus 5\.5 on light work: the last 10 turns answered in about 150 tokens.*Sonnet/);
  assert.match(w[2].text, /^Read-only exploration on Opus 5\.5: 9 reads in a row\. .*Haiku subagent/);
  assert.equal(U.line(null, T, {}), '');
  assert.deepEqual(U.facts(undefined, T, {}), []);
  for (const k of Object.keys(I18N.STR.en).filter(k => k.startsWith('use.') || k.startsWith('set.warn') || k.startsWith('set.usage') || k.startsWith('set.prices'))) assert.ok(I18N.STR.vi[k], `vi ${k}`);
});

test('skills audit: every string of the Skills screen in both languages, with the same {names}', () => {
  const I18N = require('../src/ui/i18n');
  const vars = s => (s.match(/\{\w+\}/g) || []).sort().join();
  const keys = Object.keys(I18N.STR.en).filter(k => k.startsWith('sk.'));
  assert.ok(keys.length > 60);
  for (const k of keys) assert.equal(vars(I18N.STR.vi[k] || ''), vars(I18N.STR.en[k]), `vi ${k}`);
  for (const k of keys) assert.ok(I18N.STR.vi[k], `vi ${k}`);
});

test('habits become skills: every string in both languages, and every one the panel uses exists', () => {
  const I18N = require('../src/ui/i18n');
  const vars = s => (s.match(/\{\w+\}/g) || []).sort().join();
  const keys = Object.keys(I18N.STR.en).filter(k => k.startsWith('hs.'));
  for (const k of keys) assert.equal(vars(I18N.STR.vi[k] || '-'), vars(I18N.STR.en[k]), `vi ${k}`);
  for (const k of Object.keys(I18N.STR.vi).filter(k => k.startsWith('hs.'))) assert.ok(I18N.STR.en[k], `en ${k}`);
  const html = require('fs').readFileSync(require('path').join(__dirname, '../src/ui/panel.html'), 'utf8');
  const used = new Set([...html.matchAll(/T\('(hs\.[\w.]*\w)'/g)].map(m => m[1]));
  for (const pre of ['hs.why.', 'hs.act.', 'hs.act.short.', 'hs.how.']) for (const w of ['new', 'improve', 'edit', 'original', 'create', 'use', '1', '2', '3', '4']) if (I18N.STR.en[pre + w]) used.add(pre + w);
  assert.ok(used.size > 50, `${used.size}`);
  for (const k of used) assert.ok(I18N.STR.en[k], `missing ${k}`);
});

test('room: every string in both languages, and every one the panel uses exists', () => {
  const I18N = require('../src/ui/i18n');
  const vars = s => (s.match(/\{\w+\}/g) || []).sort().join();
  const keys = Object.keys(I18N.STR.en).filter(k => k.startsWith('rm.'));
  for (const k of keys) assert.equal(vars(I18N.STR.vi[k] || '-'), vars(I18N.STR.en[k]), `vi ${k}`);
  for (const k of Object.keys(I18N.STR.vi).filter(k => k.startsWith('rm.'))) assert.ok(I18N.STR.en[k], `en ${k}`);
  const html = require('fs').readFileSync(require('path').join(__dirname, '../src/ui/panel.html'), 'utf8');
  const used = new Set([...html.matchAll(/T\('(rm\.[\w.]*\w)'/g)].map(m => m[1]));
  for (const k of ['rm.kind.option', 'rm.kind.pro', 'rm.kind.con', 'rm.kind.note', 'rm.why.not invited', 'rm.state.open', 'rm.state.decided', ...['create', 'invite', 'uninvite', 'join', 'saw', 'add', 'record', 'refused', 'decide'].map(e => 'rm.ev.' + e)]) used.add(k);
  assert.ok(used.size > 60, `${used.size}`);
  for (const k of used) assert.ok(I18N.STR.en[k], `missing ${k}`);
});
