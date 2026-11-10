// Costumes by state: an accessory drawn into the pet's own 36×36 sprite, in Bobix's palette and pixel style (1-pixel
// dark outline, a light top-left, the body tokens). Because it is part of the sprite, the pet SVG and the pixel effects
// (pet-fx.js) draw the same dressed pet: the waiting outline hugs the lamp, the pet's hop and shiver carry it. Every
// costume pixel stays inside the pet's own 36×36 box (its 80% creature box on screen), so it never reaches the XP bar or
// battery above it, the name tag under it, or a neighbour.
// Working: a hard hat (a headlamp while reading or searching) and, in the hand, the tool the session is using, moving
// so it reads as work being done: a pencil writing a line on a sheet (editing), a little terminal typing (a command), a
// magnifier sweeping (reading, searching), a turning globe (the web). It takes the effect's tool card's place; a tool
// with no prop keeps the card. Waiting: a signal lamp, nothing held (the orange outline already says it). Done: a crown
// and a finish flag, its cloth waving, put on once the XP effect has played. Low context: heavy eyelids. Asleep: a
// nightcap. Held props go in the hand on the side the pet walks to.
// Needs creature.js (TOKENS, oklch, mixOklch, mix32, packRGB, WHITE32).
const COSTUME = (() => {
  const W = 36;
  const tok = n => TOKENS[n] || { '--coral': [.66, .175, 33], '--grey': [.66, .02, 262] }[n];
  const col = n => oklch(tok(n)), dark = n => oklch(mixOklch(tok(n), TOKENS['--n-900'], .6)), light = n => mix32(oklch(tok(n)), WHITE32, .5);
  const INK = oklch(TOKENS['--n-900']), PAPER = packRGB(251, 248, 241), GREEN = packRGB(120, 230, 160);
  const SEARCH = new Set(['Read', 'Grep', 'Glob', 'WebFetch', 'WebSearch', 'LS']);

  // Which costume a session wears now. ('error' has no session state in the app yet: drawn, never chosen.)
  function kindOf(s, st) {
    if (st === 'idle') return 'sleep';
    if (st === 'waiting') return 'wait';
    if (st === 'done') return 'done';
    if (s.ctx != null && s.ctx < 25) return 'ctx';
    if (st === 'working') return SEARCH.has(s.tool) ? 'search' : 'work';
    return null;
  }
  // The top of the head: the first row with a pixel in the middle of the box.
  function headTop(spr) {
    for (let y = 0; y < W; y++) for (let x = 11; x < 25; x++) if (spr.data[y * W + x] >>> 24) return y;
    return 4;
  }
  // A layer: filled pixels, then a 1-pixel dark outline round them (4 ways), then pixels drawn on top without one.
  // hand(true): what follows is held, drawn mirrored when the pet walks left. hand(true, [x0, x1]): moved to the other
  // side instead of mirrored, for a prop with writing on it (x0..x1 its columns).
  function layer(left) {
    const fill = new Map(), top = new Map();
    let mir = false, shift = 0;
    const P = (m, x, y, c) => { x = Math.round(x); if (mir) x = shift ? x - shift : W - 1 - x; y = Math.round(y); if (x >= 0 && y >= 0 && x < W && y < W) m.set(y * W + x, c); };
    return {
      hand: (on, span) => { mir = on && left; shift = span ? span[0] + span[1] - (W - 1) : 0; },
      p: (x, y, c) => P(fill, x, y, c), t: (x, y, c) => P(top, x, y, c),
      rect: (x, y, w, h, c, m = fill) => { for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) P(m, x + i, y + j, c); },
      line(x0, y0, x1, y1, c, m = fill) { const n = Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0)) || 1; for (let i = 0; i <= n; i++) P(m, x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n, c); },
      onto(data) {
        const out = new Map(fill);
        for (const k of fill.keys()) { const x = k % W, y = (k / W) | 0; for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) { const xx = x + dx, yy = y + dy; if (xx >= 0 && yy >= 0 && xx < W && yy < W && !fill.has(yy * W + xx)) out.set(yy * W + xx, INK); } }
        for (const [k, c] of top) out.set(k, c);
        for (const [k, c] of out) data[k] = c;
        return out.size;
      },
    };
  }
  const CX = 17.5; // the box's middle column (between 17 and 18)
  const span = hw => [Math.ceil(CX - hw), Math.floor(CX + hw)];

  // ---- the pieces (t: the head's top row) ----
  function hardHat(L, t) { // a sky-blue work helmet with a light ridge and a wide brim
    [[3, -2], [5, -1], [6, 0], [6, 1]].forEach(([hw, dy]) => { const [a, b] = span(hw); for (let x = a; x <= b; x++) L.p(x, t + dy, x === 17 || x === 18 ? light('--sky') : col('--sky')); });
    L.p(14, t - 1, WHITE32); L.p(13, t, WHITE32);
    for (let x = 9; x <= 26; x++) L.p(x, t + 2, dark('--sky'));
  }
  function headlamp(L, t) { // a lilac band round the forehead and a butter lamp at its front
    for (let x = 9; x <= 26; x++) L.t(x, t + 3, dark('--lilac'));
    L.rect(15, t + 1, 6, 3, col('--butter')); L.rect(16, t + 2, 4, 1, WHITE32); L.p(15, t + 1, light('--butter'));
  }
  // ---- held tools (working): f is the frame, 0..3 ----
  const PROP_OF = { Edit: 'pencil', Write: 'pencil', MultiEdit: 'pencil', NotebookEdit: 'pencil', Bash: 'term', PowerShell: 'term',
    Read: 'lens', Grep: 'lens', Glob: 'lens', LS: 'lens', WebFetch: 'globe', WebSearch: 'globe' };
  function pencil(L, f) { // a sheet with two written lines; a long pencil writes the third, a pixel a frame
    L.rect(22, 23, 10, 8, PAPER);
    L.rect(24, 25, 6, 1, light('--grey')); L.rect(24, 27, 5, 1, light('--grey'));
    const n = f + 1, tx = 24 + n; L.rect(24, 29, n, 1, INK);
    for (let i = 0; i <= 8; i++) { // the pencil leaning right from its point: lead, wood, yellow body, pink eraser (below the eyes)
      const x = tx + Math.round(i * .45), y = 28 - i, c = i === 0 ? INK : i < 3 ? light('--peach') : i < 7 ? col('--butter') : col('--rose');
      L.p(x, y, c); L.p(x + 1, y, i > 2 && i < 7 ? dark('--butter') : c);
    }
  }
  function term(L, f) { // a little terminal: three window dots, a prompt, a line being typed, a blinking cursor, output
    L.rect(23, 20, 12, 10, INK); L.rect(23, 20, 12, 1, col('--grey'));
    L.t(24, 20, col('--coral')); L.t(25, 20, col('--butter')); L.t(26, 20, col('--mint'));
    for (const [x, y] of [[24, 22], [25, 23], [24, 24]]) L.t(x, y, GREEN);
    const n = [1, 3, 5, 6][f]; for (let i = 0; i < n; i++) L.t(27 + i, 23, WHITE32);
    if (f % 2 === 0) L.rect(27 + n, 22, 1, 3, col('--butter'));
    for (let x = 24; x <= 32; x++) L.t(x, 26, mix32(GREEN, INK, .45));
    for (let x = 24; x <= 29; x++) L.t(x, 28, mix32(GREEN, INK, .45));
  }
  function lens(L, f) { // a magnifier sweeping to and fro: a lilac rim, a pale lens with a glint, a wooden handle
    const [ox, oy] = [[0, 0], [-1, -1], [-2, 0], [-1, 1]][f];
    for (let y = 16; y <= 26; y++) for (let x = 23; x <= 33; x++) { const d = Math.hypot(x - 28, y - 21); if (d <= 4.6) L.p(x + ox, y + oy, d > 3.4 ? dark('--lilac') : light('--sky')); }
    L.p(26 + ox, 19 + oy, WHITE32); L.p(27 + ox, 18 + oy, WHITE32);
    L.line(31 + ox, 25 + oy, 33 + ox, 28 + oy, dark('--peach')); L.line(32 + ox, 25 + oy, 34 + ox, 28 + oy, dark('--peach'));
  }
  // The globe's lands, one turn of it (8 columns), rows top to bottom.
  const LANDS = ['..##....', '.####...', '.###..#.', '..##.###', '...#.###', '.....##.', '..#...#.', '.###....', '..##....', '........'];
  function globe(L, f) { // a turning globe: a deep sea, green lands sliding round two columns a frame, a glint, a stand
    const cx = 28, cy = 21.5, sea = oklch([.68, .12, 245]), land = oklch([.78, .16, 145]);
    for (let y = 17; y <= 26; y++) for (let x = 22; x <= 34; x++) {
      const d = Math.hypot(x + .5 - cx, y + .5 - cy); if (d > 5) continue;
      L.p(x, y, d > 4.1 ? dark('--sky') : LANDS[y - 17][(x - 22 + f * 2) % 8] === '#' ? land : sea);
    }
    L.t(26, 18, WHITE32); L.t(25, 19, WHITE32);
    L.rect(26, 28, 5, 1, col('--grey')); L.p(28, 27, col('--grey'));
  }
  const PROPS = { pencil, term, lens, globe };
  // A held tool; one with writing on it (the terminal, the written sheet) moves hands rather than mirroring.
  const held = (L, o) => { const k = PROP_OF[o.tool]; if (k) { L.hand(true, { term: [23, 34], pencil: [22, 34] }[k]); PROPS[k](L, o.frame); } };
  function callLamp(L, t) { // waiting: a little coral signal lamp on a grey base
    L.rect(14, t + 1, 8, 2, col('--grey')); L.rect(14, t + 1, 8, 1, light('--grey'));
    [[2, -2], [3, -1], [3, 0]].forEach(([hw, dy]) => { const [a, b] = span(hw); for (let x = a; x <= b; x++) L.p(x, t + dy, col('--coral')); });
    L.p(16, t - 1, WHITE32); L.p(16, t, light('--coral'));
  }
  function crown(L, t) { // done: a butter crown with a mint gem
    L.rect(12, t, 12, 2, col('--butter')); L.rect(12, t, 12, 1, light('--butter'));
    for (const x of [12, 17, 18, 23]) L.p(x, t - 1, col('--butter'));
    for (const x of [12, 17, 18, 23]) L.p(x, t - 2, x === 17 || x === 18 ? col('--butter') : light('--butter'));
    L.p(17, t + 1, col('--mint')); L.p(18, t + 1, dark('--mint'));
  }
  function flag(L, f) { // done: a short finish flag held at the side, a big cloth checked in 3×3 squares. The cloth waves:
    // a ripple runs from the pole to the free end, each column of squares up or down a pixel in turn (f is the frame,
    // 0..3), so the squares stay square; the pole and the squares at the pole stay still.
    L.rect(33, 9, 1, 21, col('--grey')); L.p(33, 8, light('--grey'));
    for (let i = 0; i < 9; i++) {
      const d = 2 - (i / 3 | 0), dy = d ? Math.round(Math.sin(f * Math.PI / 2 - d * 1.6)) : 0; // d: columns of squares from the pole
      for (let j = 0; j < 6; j++) L.p(24 + i, 9 + j + dy, ((i / 3 | 0) + (j / 3 | 0)) % 2 ? INK : WHITE32);
    }
  }
  function bandage(L, t) { // error: two crossed plasters on the head
    L.line(13, t + 1, 21, t + 4, light('--peach')); L.line(13, t + 2, 21, t + 5, light('--peach'));
    L.line(13, t + 5, 21, t + 2, light('--peach')); L.line(13, t + 4, 21, t + 1, light('--peach'));
    L.t(17, t + 3, dark('--peach')); L.t(15, t + 2, dark('--peach')); L.t(19, t + 2, dark('--peach'));
  }
  function cheekPlaster(L) { L.rect(25, 21, 4, 2, light('--peach')); L.t(26, 21, dark('--peach')); L.t(27, 22, dark('--peach')); }
  function tired(L, spr) { // low context: heavy eyelids over the working face (the battery effect says the rest)
    const body = spr.data[17 * W + 17];
    for (const x0 of [12, 20]) for (let i = 0; i < 4; i++) { L.t(x0 + i, 16, body); L.t(x0 + i, 17, body); L.t(x0 + i, 18, INK); }
  }
  function nightcap(L, t) { // asleep: a lilac cap with a white cuff, its tip flopping down the left side (the "z" is on the right), a butter pompom
    for (let x = 10; x <= 23; x++) L.p(x, t + 1, WHITE32);
    for (let x = 11; x <= 22; x++) L.p(x, t, col('--lilac'));
    for (let x = 10; x <= 20; x++) L.p(x, t - 1, x > 17 ? light('--lilac') : col('--lilac'));
    for (let x = 9; x <= 16; x++) L.p(x, t - 2, col('--lilac'));
    [[8, t], [7, t + 1], [7, t + 2], [6, t + 3], [6, t + 4]].forEach(([x, y]) => { L.p(x, y, col('--lilac')); L.p(x + 1, y, dark('--lilac')); });
    L.p(8, t - 1, col('--lilac')); L.p(9, t - 1, col('--lilac'));
    L.rect(4, t + 5, 3, 3, col('--butter')); L.p(4, t + 5, light('--butter'));
  }

  const SETS = {
    work: (L, t, spr, o) => { hardHat(L, t); held(L, o); },
    search: (L, t, spr, o) => { headlamp(L, t); held(L, o); },
    wait: (L, t) => callLamp(L, t),
    done: (L, t, spr, o) => { crown(L, t); L.hand(true); flag(L, o.frame); },
    error: (L, t) => { bandage(L, t); cheekPlaster(L); },
    ctx: (L, t, spr) => tired(L, spr),
    sleep: (L, t) => nightcap(L, t),
  };
  // A held tool takes the tool card's place: no tool card then. A tool without a prop keeps its card.
  const holdsTool = (kind, tool) => (kind === 'work' || kind === 'search') && !!PROP_OF[tool];
  // What moves, 4 frames: the held tool, and the finish flag waving.
  const moves = (kind, tool) => holdsTool(kind, tool) || kind === 'done';
  const CACHE = new WeakMap(); // by the sprite's pixels (fxSprite hands out a fresh object each call)
  // The pet dressed for a costume kind, or the pet as it is (no kind, or not a 36×36 creature sprite).
  // o: { left (the pet walks left: its prop is in its left hand), tool (the session's tool), frame (0..3),
  //      only (the costume pixels alone, for the checks in test/) }.
  function dress(spr, kind, o = {}) {
    if (!spr || !SETS[kind] || spr.w !== W || spr.h !== W) return spr;
    const { only = false, left = false } = o, tool = PROP_OF[o.tool] ? o.tool : '', frame = moves(kind, tool) ? (o.frame || 0) % 4 : 0;
    const key = `${kind}|${only}|${left}|${kind === 'done' ? '' : PROP_OF[tool] || ''}|${frame}`;
    let per = CACHE.get(spr.data); if (!per) CACHE.set(spr.data, (per = new Map()));
    let data = per.get(key); // only the pixels are kept: the sprite's other fields (k) come from the caller's
    if (!data) { const L = layer(left); SETS[kind](L, headTop(spr), spr, { tool, frame }); data = only ? new Uint32Array(W * W) : spr.data.slice(); L.onto(data); per.set(key, data); }
    return { ...spr, data };
  }
  // One-shot effects that play before the costume is put on, as they draw round the head: the XP orbs fly from the body
  // up past it to the bar, the low-context battery and its sweat sit on and beside it. So a finished task gets its XP
  // first and its crown and flag after; worn together, the orbs or the battery would cross the crown, flag or nightcap.
  const after = fxKind => fxKind === 'xp' || fxKind === 'ctx';
  return { kindOf, dress, holdsTool, moves, after, KINDS: Object.keys(SETS) };
})();
