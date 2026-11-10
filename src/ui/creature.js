function hashStr(str) { let h = 2166136261; for (const c of str) h = Math.imul(h ^ c.charCodeAt(0), 16777619); return h >>> 0; }

// The session colours, as the design tokens of ui/theme.css (oklch L, C, h; a test keeps them equal).
const BODY = ['--sky', '--mint', '--lilac', '--peach', '--butter', '--rose'];
const TOKENS = { '--n-900': [.24, .035, 266], '--sky': [.84, .075, 235], '--mint': [.86, .085, 162], '--lilac': [.83, .075, 300], '--peach': [.86, .075, 55], '--butter': [.90, .095, 98], '--rose': [.84, .075, 5] };
// Colours packed 0xAABBGGRR, written out as #rrggbb (hex32).
const packRGB = (r, g, b) => ((255 << 24) | (b << 16) | (g << 8) | r) >>> 0;
function oklch([L, C, h]) {
  const a = C * Math.cos(h * Math.PI / 180), b = C * Math.sin(h * Math.PI / 180);
  const l = (L + .3963377774 * a + .2158037573 * b) ** 3, m = (L - .1055613458 * a - .0638541728 * b) ** 3, s = (L - .0894841775 * a - 1.291485548 * b) ** 3;
  const g = x => Math.round(255 * Math.min(1, Math.max(0, x <= .0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - .055)));
  return packRGB(g(4.0767416621 * l - 3.3077115913 * m + .2309699292 * s), g(-1.2684380046 * l + 2.6097574011 * m - .3413193965 * s), g(-.0041960863 * l - .7034186147 * m + 1.707614701 * s));
}
const hex32 = c => '#' + [0, 8, 16].map(s => ((c >>> s) & 255).toString(16).padStart(2, '0')).join('');

// The pet of a project: one of the flat-icon designs (its shape, from the project), in its session's colour (an index into
// BODY, see petColor; by default the colour that goes with id), acting its state. face: an uploaded picture, worn as the
// pet's face in a window on its body instead; pixel: that picture is pixel art, scaled without smoothing. pose: [pose,
// frame] from poseAt (the overlay animates it); without it, the state's first frame.
function creature(id, st, face, color, pixel, pose) {
  const ci = color ?? hashStr(id + 'c') % BODY.length;
  return flatSVG(face ? 'picture' : designOf(id), ci, st, pose || poseAt(st), true, face && { href: face, pixel });
}

// ---- the flat-icon pets ----------------------------------------------------------------------------------------------
// Original pets drawn in code: soft rounded shapes, 2–3 colours, a gentle top-to-bottom gradient, no outline, on a
// 36×36 box standing on its bottom. Each acts its state with the whole body (flatPose) and shows it in an app-icon
// badge; waiting for you adds a coral ring and tint round the icon.
const DESIGNS = ['default', 'nota', 'tty', 'miso', 'ship']; // Bobix's own mascot ('bobix') is the default one
const designOf = id => DESIGNS[hashStr(id) % DESIGNS.length];
const FACE = '#1f2230', CHEEK = '#f6a9bd', STATUS_FILL = { working: '#5b8def', waiting: '#ff7a4d', done: '#34c38f', failed: '#e5484d' };
// Each state's pose loop: frames and ms a frame. Done waves, then jumps; a working pet that walks the line walks.
const LOOPS = { walkR: [4, 150], walkL: [4, 150], typing: [4, 150], waiting: [4, 220], wave: [4, 150], jump: [4, 150], failed: [4, 300], asleep: [2, 1200] };
const STATUS = { working: 'working', waiting: 'waiting', done: 'done', failed: 'failed', error: 'failed' }; // idle: asleep, no badge
// The pose a pet in state st shows t ms into its loop: [pose, frame]. walk: 'R' or 'L' while it walks that way.
function poseAt(st, t = 0, walk = '') {
  if (st === 'done') { const w = LOOPS.wave, k = t % (w[0] * w[1] * 2); return k < w[0] * w[1] ? ['wave', Math.floor(k / w[1])] : ['jump', Math.floor((k - w[0] * w[1]) / w[1])]; }
  const ps = st === 'working' && walk ? 'walk' + walk : { waiting: 'waiting', idle: 'asleep', failed: 'failed', error: 'failed' }[st] || 'typing';
  return [ps, Math.floor(t / LOOPS[ps][1]) % LOOPS[ps][0]];
}
function flatPose(ps, f) {
  const o = { dy: 0, rot: 0, sx: 1, sy: 1, lift: 0, feet: [0, 0], hands: [[6.5, 21], [29.5, 21]], eyes: 'open', mouth: 'smile', look: 0, cheeks: false, shadow: 1, blink: 1 };
  switch (ps) {
    case 'walkR': o.dy = f % 2 ? -1 : 0; o.rot = [-3, 0, 3, 0][f]; o.feet = [[1.6, 0], [0, 0], [0, 1.6], [0, 0]][f]; o.hands = f < 2 ? [[6.5, 19], [29.5, 23]] : [[6.5, 23], [29.5, 19]]; o.look = .8; o.mouth = 'small'; o.blink = f % 2; break;
    case 'walkL': { const r = flatPose('walkR', f); return { ...r, rot: -r.rot, look: -r.look, feet: [r.feet[1], r.feet[0]], hands: [[36 - r.hands[1][0], r.hands[1][1]], [36 - r.hands[0][0], r.hands[0][1]]] }; }
    case 'typing': o.eyes = 'down'; o.mouth = 'small'; o.hands = f % 2 ? [[13, 25], [23, 26.6]] : [[13, 26.6], [23, 25]]; o.blink = f % 2; break;
    case 'waiting': o.eyes = 'look'; o.mouth = 'o'; o.hands = [[5, 22], [31, 22]]; o.feet = [0, f % 2 ? 1.8 : 0]; o.sy = f % 2 ? 1.02 : 1; break;
    case 'wave': o.eyes = 'happy'; o.mouth = 'open'; o.cheeks = true; o.hands = [[6.5, 22], f % 2 ? [32.5, 9.5] : [31, 5.5]]; o.rot = f % 2 ? 2 : -1; break;
    case 'jump': o.eyes = 'happy'; o.mouth = 'open'; o.cheeks = true; o.lift = [0, 4, 6, 2][f]; o.sy = f === 0 ? .92 : f === 2 ? 1.04 : 1; o.sx = f === 0 ? 1.06 : 1;
      o.hands = f ? [[5.5, 7], [30.5, 7]] : [[6.5, 23], [29.5, 23]]; o.shadow = [1, .7, .55, .85][f]; o.feet = f ? [1, 1] : [0, 0]; break;
    case 'failed': o.dy = 2; o.rot = 5; o.sy = .95; o.eyes = 'sad'; o.mouth = 'frown'; o.hands = [[8, 28.5], [28, 28.5]]; break;
    case 'asleep': o.dy = f ? 1.6 : 1; o.sy = f ? .97 : 1; o.eyes = 'closed'; o.mouth = 'none'; o.hands = [[8.5, 26.5], [27.5, 26.5]]; break;
  }
  return o;
}
const n1 = v => +v.toFixed(2);
// Eyes and mouth as soft strokes and dots. cx: between the eyes, ey: the eyes' row, gap: between them.
function faceSVG(cx, ey, gap, o, ink = FACE) {
  let s = '';
  for (const x0 of [cx - gap / 2, cx + gap / 2]) {
    const x = n1(x0 + o.look), d = x0 < cx ? 1 : -1;
    switch (o.eyes) {
      case 'open': s += `<circle cx="${x}" cy="${ey}" r="1.5" fill="${ink}"/>`; break;
      case 'look': s += `<circle cx="${x}" cy="${ey}" r="2.05" fill="${ink}"/><circle cx="${n1(x - .65)}" cy="${n1(ey - .75)}" r=".7" fill="#fff"/>`; break;
      case 'down': s += `<ellipse cx="${x}" cy="${ey + 1}" rx="1.5" ry=".8" fill="${ink}"/>`; break;
      case 'happy': s += `<path d="M${n1(x - 1.7)} ${n1(ey + .9)}Q${x} ${n1(ey - 1.3)} ${n1(x + 1.7)} ${n1(ey + .9)}" fill="none" stroke="${ink}" stroke-width="1.25" stroke-linecap="round"/>`; break;
      case 'sad': s += `<circle cx="${x}" cy="${ey + 1}" r="1.2" fill="${ink}"/><path d="M${n1(x - 1.6 * d)} ${n1(ey - .8)}L${n1(x + 1.3 * d)} ${n1(ey - 2)}" stroke="${ink}" stroke-width="1" stroke-linecap="round"/>`; break;
      case 'closed': s += `<path d="M${n1(x - 1.7)} ${n1(ey + .5)}Q${x} ${n1(ey + 1.8)} ${n1(x + 1.7)} ${n1(ey + .5)}" fill="none" stroke="${ink}" stroke-width="1.15" stroke-linecap="round"/>`; break;
    }
  }
  if (o.cheeks) s += `<ellipse cx="${n1(cx - gap / 2 - 2.4)}" cy="${n1(ey + 2.7)}" rx="1.5" ry=".85" fill="${CHEEK}"/><ellipse cx="${n1(cx + gap / 2 + 2.4)}" cy="${n1(ey + 2.7)}" rx="1.5" ry=".85" fill="${CHEEK}"/>`;
  const mx = n1(cx + o.look), my = ey + 4, st = `fill="none" stroke="${ink}" stroke-width="1.1" stroke-linecap="round"`;
  switch (o.mouth) {
    case 'smile': s += `<path d="M${n1(mx - 1.6)} ${my}Q${mx} ${n1(my + 1.6)} ${n1(mx + 1.6)} ${my}" ${st}/>`; break;
    case 'small': s += `<path d="M${n1(mx - .9)} ${n1(my + .3)}H${n1(mx + .9)}" ${st}/>`; break;
    case 'o': s += `<ellipse cx="${mx}" cy="${n1(my + .4)}" rx=".95" ry="1.15" fill="${ink}"/>`; break;
    case 'open': s += `<path d="M${n1(mx - 2)} ${n1(my - .2)}Q${mx} ${n1(my + 3.2)} ${n1(mx + 2)} ${n1(my - .2)}Z" fill="${ink}"/>`; break;
    case 'frown': s += `<path d="M${n1(mx - 1.6)} ${n1(my + 1.1)}Q${mx} ${n1(my - .5)} ${n1(mx + 1.6)} ${n1(my + 1.1)}" ${st}/>`; break;
  }
  return s;
}
// Tty's face is on its screen: glowing bars, a cursor for a mouth.
function ttyFace(o, glow) {
  let s = '';
  for (const x0 of [13.2, 22.8]) {
    const x = n1(x0 - 1 + o.look);
    switch (o.eyes) {
      case 'open': s += `<rect x="${x}" y="11" width="2" height="3" rx=".4" fill="${glow}"/>`; break;
      case 'look': s += `<rect x="${n1(x - .3)}" y="10.4" width="2.6" height="3.8" rx=".5" fill="${glow}"/>`; break;
      case 'down': s += `<rect x="${x}" y="13" width="2.2" height="1.4" rx=".4" fill="${glow}"/>`; break;
      case 'happy': s += `<path d="M${n1(x - .7)} 13.4L${n1(x + 1)} 11.6L${n1(x + 2.7)} 13.4" fill="none" stroke="${glow}" stroke-width="1" stroke-linecap="round" stroke-linejoin="round"/>`; break;
      case 'sad': s += `<rect x="${x}" y="12.2" width="2" height="2" rx=".4" fill="${glow}"/><path d="M${n1(x0 < 18 ? x - .4 : x + 2.4)} 10.8L${n1(x0 < 18 ? x + 2 : x)} 10" stroke="${glow}" stroke-width=".9" stroke-linecap="round"/>`; break;
      case 'closed': s += `<rect x="${n1(x - .5)}" y="13.4" width="3" height=".9" rx=".45" fill="${glow}"/>`; break;
    }
  }
  const tall = o.mouth === 'open' || o.mouth === 'o';
  if (o.blink || tall) s += `<rect x="${n1(16.5 + o.look)}" y="16.6" width="3" height="${tall ? 2 : 1.1}" rx=".4" fill="${glow}"/>`;
  return s;
}
// Each design: where its feet stand, the colour of its hands and feet, its body and its face. g: its gradient's
// url(), c: its colours ({ hi, lo }: the session's colour and a deeper shade of it), pic: a picture worn as its face
// ({ href, pixel, id }: id for its clip).
const FLAT = {
  picture: { // the default body with a rounded window for an uploaded picture
    feet: [10.3, 20.1], hand: c => c.lo, grad: c => [c.hi, c.lo],
    body: (g, c, pic) => `<circle cx="11.5" cy="8.6" r="3.3" fill="${g}"/><circle cx="24.5" cy="8.6" r="3.3" fill="${g}"/><rect x="6.5" y="7" width="23" height="22" rx="8.5" fill="${g}"/>`
      + `<clipPath id="${pic.id}c"><rect x="9.5" y="10" width="17" height="16" rx="5"/></clipPath><rect x="9.5" y="10" width="17" height="16" rx="5" fill="#fff"/>`
      + `<image href="${pic.href}" x="9.5" y="10" width="17" height="16" clip-path="url(#${pic.id}c)" preserveAspectRatio="xMidYMid slice"${pic.pixel ? ' style="image-rendering:pixelated"' : ''}/>`,
    face: () => '',
  },
  default: {
    feet: [10.3, 20.1], hand: c => c.lo, grad: c => [c.hi, c.lo],
    body: g => `<circle cx="11.5" cy="8.6" r="3.3" fill="${g}"/><circle cx="24.5" cy="8.6" r="3.3" fill="${g}"/><rect x="6.5" y="7" width="23" height="22" rx="8.5" fill="${g}"/>`,
    face: o => faceSVG(18, 16, 7, o),
  },
  nota: { // a spiral notebook: face on the cover, rings, a page edge and a label
    feet: [10.8, 19.6], hand: c => c.lo, grad: c => [c.hi, c.lo],
    body: g => `<rect x="8" y="4" width="20" height="26" rx="5" fill="${g}"/><rect x="24.5" y="5.6" width="2.3" height="22.8" rx="1.15" fill="#fbf6ea"/>`
      + [9, 13.5, 18, 22.5].map(y => `<circle cx="8.7" cy="${y}" r="1.05" fill="#fbf6ea"/>`).join('') + `<rect x="12" y="21.4" width="9.6" height="4" rx="1.2" fill="#fbf6ea" opacity=".92"/>`,
    face: o => faceSVG(16.8, 12.4, 7, o),
  },
  tty: { // a little terminal: its face glows on the screen, red when it fails, dim asleep
    feet: [10.6, 19.8], hand: c => c.lo, grad: c => [c.hi, c.lo],
    body: (g, c) => `<rect x="15" y="23.5" width="6" height="5" rx="1" fill="${c.lo}"/><rect x="5.5" y="5" width="25" height="20" rx="5" fill="${g}"/><rect x="8.5" y="7.8" width="19" height="13" rx="3" fill="#1e2a33"/>`,
    face: (o, st) => ttyFace(o, st === 'failed' ? '#ff6b6b' : st === 'idle' ? '#3f7a50' : '#7df29b'),
  },
  miso: { // a round cat: ears, a curled tail, whiskers, a light muzzle
    feet: [10.3, 20.1], hand: c => c.lo, grad: c => [c.hi, c.lo],
    body: (g, c) => `<path d="M28 25.5Q34.5 23.5 32.2 15.8" fill="none" stroke="${c.lo}" stroke-width="2.6" stroke-linecap="round"/>`
      + `<path d="M9.2 11L10.6 3L16 8Z" fill="${g}" stroke="${c.hi}" stroke-width="2" stroke-linejoin="round"/><path d="M26.8 11L25.4 3L20 8Z" fill="${g}" stroke="${c.hi}" stroke-width="2" stroke-linejoin="round"/>`
      + `<path d="M11.2 7.6L11.7 5.2L13.6 7Z" fill="${CHEEK}"/><path d="M24.8 7.6L24.3 5.2L22.4 7Z" fill="${CHEEK}"/>`
      + `<rect x="6.5" y="6.5" width="23" height="22.5" rx="10" fill="${g}"/><ellipse cx="18" cy="20.2" rx="4.4" ry="3.1" fill="#f6f3fb"/>`
      + `<path d="M3.4 17.6H7.6M3.8 20.2H7.6M28.4 17.6H32.6M28.4 20.2H32.2" stroke="${FACE}" stroke-opacity=".35" stroke-width=".7" stroke-linecap="round"/><path d="M17 17.4h2l-1 1.1z" fill="#e07a9a"/>`,
    face: o => faceSVG(18, 14.2, 7, o),
  },
  ship: { // a small rocket: white body, its nose and fins in the session's colour, a porthole
    feet: [11.8, 18.6], hand: () => '#cdd5e2', grad: () => ['#f6f8fc', '#cdd5e2'], across: true,
    body: (g, c) => `<path d="M11.2 19.5L6.6 26.6Q6.3 28.6 8.3 28.4L11.6 27Z" fill="${c.lo}"/><path d="M24.8 19.5L29.4 26.6Q29.7 28.6 27.7 28.4L24.4 27Z" fill="${c.lo}"/>`
      + `<rect x="10.5" y="5.5" width="15" height="23.5" rx="7.5" fill="${g}"/><path d="M10.7 11.2Q18 -2.2 25.3 11.2Z" fill="${c.lo}"/>`
      + `<circle cx="18" cy="13.6" r="2.9" fill="#2d4a7a" stroke="#cdd5e2" stroke-width="1"/><circle cx="17.1" cy="12.7" r=".85" fill="#9fd8ff"/>`,
    face: o => faceSVG(18, 20.2, 5.4, o),
  },
};
// An app-icon badge for the status, top right.
function badge(st) {
  const x = 29.6, y = 6.4, w = 'stroke="#fff" stroke-width="1.3" stroke-linecap="round" fill="none"';
  const glyph = {
    working: `<circle cx="${n1(x - 1.8)}" cy="${y}" r=".65" fill="#fff"/><circle cx="${x}" cy="${y}" r=".65" fill="#fff"/><circle cx="${n1(x + 1.8)}" cy="${y}" r=".65" fill="#fff"/>`,
    waiting: `<path d="M${x} ${n1(y - 2.4)}V${n1(y + .5)}" ${w}/><circle cx="${x}" cy="${n1(y + 2.1)}" r=".72" fill="#fff"/>`,
    done: `<path d="M${n1(x - 2)} ${n1(y + .1)}L${n1(x - .5)} ${n1(y + 1.6)}L${n1(x + 2.1)} ${n1(y - 1.5)}" ${w}/>`,
    failed: `<path d="M${n1(x - 1.6)} ${n1(y - 1.6)}L${n1(x + 1.6)} ${n1(y + 1.6)}M${n1(x + 1.6)} ${n1(y - 1.6)}L${n1(x - 1.6)} ${n1(y + 1.6)}" ${w}/>`,
  }[st];
  return glyph ? `<circle cx="${x}" cy="${y}" r="4.7" fill="${STATUS_FILL[st]}" stroke="#fff" stroke-width="1.2"/>${glyph}` : '';
}
// The session colour ci as { hi, lo }: the token, and a deeper, a little stronger shade of it for hands, feet and shade.
const PET_COLORS = BODY.map(t => { const [L, C, h] = TOKENS[t]; return { hi: hex32(oklch([L, C, h])), lo: hex32(oklch([L - .1, C + .03, h])) }; });
let GID = 0; // each picture's gradient gets an id of its own: one in a hidden part of a page never blanks the others
// The status ring and badge over a pet's box: while it waits for you, the coral ring (its brightness pulses with the
// frame f); the badge for its status.
const statusMarks = (status, f = 0) => (status === 'waiting' ? `<rect class="ring" x="1.6" y="-.6" width="32.8" height="34" rx="10.5" fill="${STATUS_FILL.waiting}" fill-opacity=".14" stroke="${STATUS_FILL.waiting}" stroke-width="1.7" opacity="${[1, .7, .45, .7][f % 4]}"/>` : '');
// The pet as SVG, every colour written out (no CSS variable). withBadge: the status badge and the waiting ring. pic: a
// picture worn as its face (the 'picture' design).
function flatSVG(key, ci, st, [ps, f], withBadge, pic) {
  const P = FLAT[key], c = PET_COLORS[ci % PET_COLORS.length], o = flatPose(ps, f), id = `bx${++GID}`, [g0, g1] = P.grad(c), status = STATUS[st];
  const T = `translate(18 30) rotate(${o.rot}) scale(${o.sx} ${o.sy}) translate(-18 -30) translate(0 ${o.dy})`, hand = P.hand(c);
  const feet = P.feet.map((x, i) => `<rect x="${x}" y="${n1(28.4 - o.feet[i])}" width="5.6" height="3.2" rx="1.6" fill="${hand}"/>`).join('');
  const hands = o.hands.map(([x, y]) => `<circle cx="${x}" cy="${y}" r="2.3" fill="${hand}"/>`).join('');
  let s = `<defs><linearGradient id="${id}" x1="0" y1="0" x2="${P.across ? 1 : 0}" y2="${P.across ? 0 : 1}"><stop offset="0" stop-color="${g0}"/><stop offset="1" stop-color="${g1}"/></linearGradient></defs>`
    + `<ellipse cx="18" cy="31.7" rx="${n1(9 * o.shadow)}" ry="1.3" fill="#000" fill-opacity=".22"/>`;
  if (withBadge) s += statusMarks(status, f);
  s += `<g transform="translate(0 ${-o.lift})">${feet}<g transform="${T}">${P.body(`url(#${id})`, c, pic && { ...pic, id })}${P.face(o, st)}${hands}</g>${withBadge ? badge(status) : ''}</g>`;
  return `<svg class="flat" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" width="100%" height="100%" overflow="visible" aria-hidden="true">${s}</svg>`;
}
// A pixel-art picture that is the whole pet: it stands on the bottom of the pet's box, scaled without smoothing (it is
// the person's own art), with the status badge and the waiting ring like every pet.
function picturePet(href, st) {
  const status = STATUS[st];
  return `<svg class="flat pic" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" width="100%" height="100%" overflow="visible" aria-hidden="true">${statusMarks(status)}`
    + `<image href="${href}" x="2" y="2" width="32" height="32" preserveAspectRatio="xMidYMax meet" style="image-rendering:pixelated"/>${badge(status)}</svg>`;
}

// Where to answer a permission request Bobix can't answer: the Claude app asks in its own card for its sessions.
const approveWhere = s => (s.hostSession ? 'ask.whereApp' : 'pane.askHint');

// The pet of a session: its project's pet, or the picture chosen for that project worn as its face, or the picture itself
// when the person chose it to be the whole pet (no poses then). pose: as creature's.
function petArt(s, st, color, pose) {
  const token = new URLSearchParams(location.search).get('t') || '';
  const pic = s.image && `/pet-image/${encodeURIComponent(s.image)}?t=${encodeURIComponent(token)}`;
  return pic && s.pixel && s.whole ? picturePet(pic, st) : creature(s.project || s.id, st, pic, color, s.pixel, pose);
}

// A session's colour. Its project gives the shape; the colour tells sessions apart: sessions of one project take the
// project's colour and the next ones in turn, and a side session never has its parent's colour or its neighbour's.
function petColor(s, list) {
  // In the order sessions came (the list keeps it), so one that arrives later never shifts the colour of one already out.
  const n = BODY.length, proj = x => x.project || x.id, ids = xs => xs.map(x => x.id);
  const top = x => !(x.parent && list.some(y => y.id === x.parent)); // stands on its own (no parent on screen)
  const par = s.parent && list.find(x => x.id === s.parent);
  if (par) { // side sessions avoid the colours of their project's own sessions, so a kid never looks like one of them
    const taken = new Set(list.filter(x => top(x) && proj(x) === proj(par)).map(x => petColor(x, list)));
    const free = [...Array(n).keys()].map(i => (petColor(par, list) + 1 + i) % n).filter(i => !taken.has(i));
    const k = ids(list.filter(x => x.parent === s.parent)).indexOf(s.id);
    return free.length ? free[k % free.length] : (petColor(par, list) + 1 + (k % (n - 1))) % n;
  }
  const mates = ids(list.filter(x => top(x) && proj(x) === proj(s)));
  return (hashStr(proj(s) + 'c') % n + Math.max(0, mates.indexOf(s.id))) % n;
}

// A party balloon in the pets' flat style: coral with "!" waits for you, green with a tick is done, butter with "!" is
// low on context. Drawn on an 11 × 22 box, the knot at (5.5, 13), the string hanging to 22. balloonParts gives the
// balloon (body, shine, knot) and its icon at (cx, cy) apart, so a flipped balloon keeps its icon upright.
const BALLOON = { waiting: ['#ff7a4d', '#e2603a', '#fff'], done: ['#34c38f', '#249c71', '#fff'], ctx: ['#f6c445', '#d9a52c', FACE] };
function balloonParts(kind) {
  const [fill, deep, ink] = BALLOON[kind] || BALLOON.waiting;
  const body = `<ellipse cx="5.5" cy="6.4" rx="5.2" ry="6.2" fill="${fill}"/><path d="M5.5 12.6Q2 12.2 1 8.6Q3.4 11.6 5.5 11.8Q7.6 11.6 10 8.6Q9 12.2 5.5 12.6Z" fill="${deep}" opacity=".55"/>`
    + `<ellipse cx="3.4" cy="3.6" rx="1.3" ry="2" transform="rotate(25 3.4 3.6)" fill="#fff" opacity=".55"/><path d="M5.5 12.4L4.4 13.6H6.6Z" fill="${deep}"/>`;
  const w = `stroke="${ink}" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round" fill="none"`;
  const icon = (cx, cy) => (kind === 'done' ? `<path d="M${n1(cx - 1.9)} ${n1(cy + .1)}L${n1(cx - .5)} ${n1(cy + 1.5)}L${n1(cx + 2)} ${n1(cy - 1.4)}" ${w}/>`
    : `<path d="M${cx} ${n1(cy - 2.4)}V${n1(cy + .5)}" ${w}/><circle cx="${cx}" cy="${n1(cy + 2.1)}" r=".7" fill="${ink}"/>`);
  return { body, icon, string: `<path d="M5.5 13.6Q4 15.6 5.5 17.6T5.5 21.8" fill="none" stroke="#868b99" stroke-width=".55" stroke-linecap="round"/>` };
}
// On the top edge it floats below the pet: balloon and string are flipped, the icon is not.
function balloonSVG(kind, down = false) {
  const b = balloonParts(kind), g = b.body + b.string;
  return `<svg viewBox="0 0 11 22" overflow="visible" aria-hidden="true">${down ? `<g transform="matrix(1 0 0 -1 0 22)">${g}</g>${b.icon(5.5, 15.6)}` : g + b.icon(5.5, 6.4)}</svg>`;
}
