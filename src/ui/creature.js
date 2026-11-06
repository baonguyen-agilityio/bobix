function hashStr(str) { let h = 2166136261; for (const c of str) h = Math.imul(h ^ c.charCodeAt(0), 16777619); return h >>> 0; }

// Sinh "sinh vật" pixel 7x7 đối xứng từ session_id, có viền tối 1px. Mắt/miệng đổi theo trạng thái.
const BODY = ['--sky', '--mint', '--lilac', '--peach', '--butter', '--rose'];
// color: an index into BODY (see petColor); by default the colour that goes with id.
// pixel: the face is pixel art, so it is scaled without smoothing.
function creature(id, st, face, color, pixel) {
  let r = hashStr(id) || 1;
  const bit = () => (r = Math.imul(r ^ (r >>> 15), 2246822507) >>> 0, r & 1);
  color = `var(${BODY[color ?? hashStr(id + 'c') % BODY.length]})`;
  if (face) return bigHead(color, bit, face, pixel);
  const on = new Set(), put = (x, y) => { on.add(`${x},${y}`); on.add(`${6 - x},${y}`); };
  for (let y = 1; y <= 5; y++) for (let x = 1; x <= 3; x++) put(x, y);
  if (bit()) put(1, 0); if (bit()) put(2, 0);          // tai / sừng
  for (let y = 2; y <= 4; y++) if (bit()) put(0, y);    // tay
  if (bit()) put(1, 6); else put(2, 6);                 // chân
  if (bit()) { on.delete('1,1'); on.delete('5,1'); }    // đầu tròn
  const cells = [...on].map(k => k.split(',').map(Number));
  const edge = new Set();
  for (const [x, y] of cells) for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
    const k = `${x + dx},${y + dy}`; if (!on.has(k)) edge.add(k);
  }
  const P = 1, O = 1; // 1 đơn vị/ô, lệch 1 ô cho viền
  const rect = (x, y, fill, cls = '', h = 1, dy = 0) => `<rect x="${x + O}" y="${y + O + dy}" width="1" height="${h}" fill="${fill}"${cls ? ` class="${cls}"` : ''}/>`;
  const ink = 'var(--n-900)';
  const outline = [...edge].map(k => { const [x, y] = k.split(',').map(Number); return rect(x, y, `color-mix(in oklch, ${color} 45%, var(--n-900))`); }).join('');
  const body = cells.map(([x, y]) => rect(x, y, color)).join('');
  let svg = outline + body;
  if (st === 'idle') svg += rect(2, 3, ink, 'eye', .34, .55) + rect(4, 3, ink, 'eye', .34, .55);
  else svg += rect(2, 3, ink, 'eye') + rect(4, 3, ink, 'eye');
  if (st === 'waiting') svg += rect(3, 4, ink);
  if (st === 'done') svg += rect(1, 4, 'var(--rose)') + rect(5, 4, 'var(--rose)') + rect(2, 4, ink, '', .4, .6) + rect(3, 4, ink, '', .4, .8) + rect(4, 4, ink, '', .4, .6);
  return `<svg viewBox="0 0 9 9" width="80%" height="80%" shape-rendering="crispEdges" aria-hidden="true">${svg}</svg>`;
}

// A creature wearing an uploaded picture: the picture (13 of 22 units) sits in the creature's body, and the outline around the
// body is drawn last, so it trims only the picture's corners into pixel steps. Ears, arms and legs come from the same project's creature (same bits,
// same order).
function bigHead(color, bit, face, pixel) {
  const px = (x, y, fill) => `<rect x="${x}" y="${y}" width="1" height="1" fill="${fill}"/>`;
  const line = `color-mix(in oklch, ${color} 45%, var(--n-900))`;
  const limbs = new Set(), put = (x, y) => { limbs.add(`${x},${y}`); limbs.add(`${21 - x},${y}`); };
  const block = (x, y, w, h) => { for (let i = 0; i < w; i++) for (let j = 0; j < h; j++) put(x + i, y + j); };
  if (bit()) block(6, 1, 2, 2); if (bit()) block(8, 1, 2, 2); // ears / horns
  for (const y of [7, 10, 13]) if (bit()) block(1, y, 2, 2);  // arms
  block(bit() ? 6 : 8, 19, 2, 2);                             // legs
  const body = new Set();
  for (let x = 4; x <= 17; x++) for (let y = 4; y <= 17; y++) body.add(`${x},${y}`);
  const cut = bit() ? [[4, 4], [5, 4], [4, 5]] : [[4, 4]];    // the corners of the body: rounder or squarer
  for (const [x, y] of cut) for (const [a, b] of [[x, y], [21 - x, y], [x, 21 - y], [21 - x, 21 - y]]) body.delete(`${a},${b}`);
  const around = (set, skip) => {
    const out = new Set();
    for (const k of set) { const [x, y] = k.split(',').map(Number); for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) { const n = `${x + dx},${y + dy}`; if (!skip.has(n)) out.add(n); } }
    return out;
  };
  const cells = (set, fill) => [...set].map(k => { const [x, y] = k.split(',').map(Number); return px(x, y, fill); }).join('');
  const ring = around(body, body), limbLine = around(limbs, new Set([...limbs, ...body]));
  return `<svg class="faced" viewBox="0 0 22 22" width="100%" height="100%" shape-rendering="crispEdges" aria-hidden="true">`
    + `${cells(limbLine, line)}${cells(limbs, color)}${cells(body, color)}`
    + `<image href="${face}" x="4.5" y="4.5" width="13" height="13"${pixel ? ' style="image-rendering:pixelated"' : ''} preserveAspectRatio="xMidYMid slice"/>${cells(ring, line)}</svg>`;
}

// A pixel-art picture that is the whole pet: it stands on the bottom of the pet's box, scaled without smoothing.
function picturePet(href) {
  return `<svg class="pic" viewBox="0 0 1 1" width="100%" height="100%" aria-hidden="true">`
    + `<image href="${href}" width="1" height="1" preserveAspectRatio="xMidYMax meet" style="image-rendering:pixelated"/></svg>`;
}

// Where to answer a permission request Bobix can't answer: the Claude app asks in its own card for its sessions.
const approveWhere = s => (s.hostSession ? 'ask.whereApp' : 'pane.askHint');

// The pet of a session: its project's creature, wearing the picture chosen for that project if there is one, or the
// picture itself when the person chose it to be the whole pet.
function petArt(s, st, color) {
  const token = new URLSearchParams(location.search).get('t') || '';
  const pic = s.image && `/pet-image/${encodeURIComponent(s.image)}?t=${encodeURIComponent(token)}`;
  return pic && s.pixel && s.whole ? picturePet(pic) : creature(s.project || s.id, st, pic, color, s.pixel);
}

// A session's colour. Its project gives the shape; the colour tells sessions apart: sessions of one project take the
// project's colour and the next ones in turn, and a side session never has its parent's colour or its neighbour's.
function petColor(s, list) {
  const n = BODY.length, proj = x => x.project || x.id, ids = xs => xs.map(x => x.id).sort();
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
