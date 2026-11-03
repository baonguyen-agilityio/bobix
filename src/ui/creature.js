function hashStr(str) { let h = 2166136261; for (const c of str) h = Math.imul(h ^ c.charCodeAt(0), 16777619); return h >>> 0; }

// Sinh "sinh vật" pixel 7x7 đối xứng từ session_id, có viền tối 1px. Mắt/miệng đổi theo trạng thái.
const BODY = ['--sky', '--mint', '--lilac', '--peach', '--butter', '--rose'];
function creature(id, st) {
  let r = hashStr(id) || 1;
  const bit = () => (r = Math.imul(r ^ (r >>> 15), 2246822507) >>> 0, r & 1);
  const color = `var(${BODY[hashStr(id + 'c') % BODY.length]})`;
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
  let svg = [...edge].map(k => { const [x, y] = k.split(',').map(Number); return rect(x, y, `color-mix(in oklch, ${color} 45%, var(--n-900))`); }).join('');
  svg += cells.map(([x, y]) => rect(x, y, color)).join('');
  if (st === 'idle') svg += rect(2, 3, ink, 'eye', .34, .55) + rect(4, 3, ink, 'eye', .34, .55);
  else svg += rect(2, 3, ink, 'eye') + rect(4, 3, ink, 'eye');
  if (st === 'waiting') svg += rect(3, 4, ink);
  if (st === 'done') svg += rect(1, 4, 'var(--rose)') + rect(5, 4, 'var(--rose)') + rect(2, 4, ink, '', .4, .6) + rect(3, 4, ink, '', .4, .8) + rect(4, 4, ink, '', .4, .6);
  return `<svg viewBox="0 0 9 9" width="80%" height="80%" shape-rendering="crispEdges" aria-hidden="true">${svg}</svg>`;
}
