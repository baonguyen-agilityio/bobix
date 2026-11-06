// Files behind a project's pet picture: the original the person chose (kept so it can be adjusted later) and the
// pixel-art result made from it. Pure (no electron), so it is tested with a temp folder.
// <dir>/<file>            results, served to our pages by /pet-image/
// <dir>/originals/<src>   originals, in a sub folder the route cannot reach
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const hash = buf => crypto.createHash('sha256').update(buf).digest('hex').slice(0, 24);

// What a picture really is, from its first bytes: the file name a person gave it proves nothing.
function kindOf(buf) {
  if (buf.length < 12) return null;
  if (buf[0] === 0x89 && buf.toString('latin1', 1, 4) === 'PNG') return 'png';
  if (buf[0] === 0xff && buf[1] === 0xd8 && buf[2] === 0xff) return 'jpg';
  if (buf.toString('latin1', 0, 4) === 'GIF8') return 'gif';
  if (buf.toString('latin1', 0, 4) === 'RIFF' && buf.toString('latin1', 8, 12) === 'WEBP') return 'webp';
  return null;
}

// A settings entry is a { src, file, opts } object, or a file name from before pictures were processed.
const filesOf = e => (typeof e === 'string' ? [e] : e && typeof e === 'object' ? [`originals/${e.src}`, e.file] : []);
const served = e => (typeof e === 'string' ? e : e?.file || null);

function write(dir, rel, data) {
  const f = path.join(dir, rel);
  fs.mkdirSync(path.dirname(f), { recursive: true });
  if (fs.existsSync(f)) return; // a name is the hash of its bytes: an existing file is the same file
  // ...so a write cut short (a full disk) must never leave a file under that name: it would be kept on the next try
  const tmp = `${f}.tmp`;
  try {
    fs.writeFileSync(tmp, data);
    fs.renameSync(tmp, f);
  } catch (err) {
    fs.rmSync(tmp, { force: true });
    throw err;
  }
}

// Stores an import and returns the pictures map with this project's entry set. The caller saves the settings,
// then calls release() with the entry it replaced.
function saveImport(dir, images, project, { original, kind, result, opts }) {
  const src = `${hash(original)}.${kind}`, file = `${hash(result.data)}.${result.ext}`;
  write(dir, `originals/${src}`, original);
  write(dir, file, result.data);
  return { ...images, [project]: { src, file, opts } };
}

// Deletes the files of `entry` (the one that was replaced or reset) that no entry of `images` uses any more:
// two projects can share a file, since a file is named after its bytes.
function release(dir, images, entry) {
  const used = new Set(Object.values(images).flatMap(filesOf));
  const gone = filesOf(entry).filter(f => !used.has(f));
  for (const f of gone) fs.rmSync(path.join(dir, f), { force: true });
  return gone;
}

// The stored original and the options it was made with, for adjusting it; null for a legacy entry or a lost file.
function readOriginal(dir, entry) {
  if (!entry || typeof entry !== 'object') return null;
  try { return { original: fs.readFileSync(path.join(dir, 'originals', entry.src)), opts: entry.opts }; } catch { return null; }
}

// nativeImage hands out BGRA with the colour already multiplied by alpha; the pixel art wants straight RGBA.
function fromBitmap(bgra) {
  const out = new Uint8ClampedArray(bgra.length);
  for (let i = 0; i < bgra.length; i += 4) {
    const a = bgra[i + 3];
    out[i + 3] = a;
    if (!a) continue;
    out[i] = Math.min(255, Math.round(bgra[i + 2] * 255 / a));
    out[i + 1] = Math.min(255, Math.round(bgra[i + 1] * 255 / a));
    out[i + 2] = Math.min(255, Math.round(bgra[i] * 255 / a));
  }
  return out;
}

module.exports = { kindOf, served, saveImport, release, readOriginal, fromBitmap };
