// The pet picture pixel art, off the panel's main thread (pet-import.js starts it as a Web Worker). It holds the decoded
// source of the one import in progress, so adjusting an option only repeats the pixel art.
// { id, token, input?, opts } in -> { id, data, ext, opts, opaque, frames, ms } | { id, gone } | { id, error } out.
// input: { kind: 'gif', data: bytes } or { kind: 'rgba', width, height, data: straight RGBA }.
self.exports = {}; // the GIF helpers are CommonJS files: they fill this in
importScripts('gif/lzw.js', 'gif/deinterlace.js', 'gif/gifenc.js', 'pixelate.js');
const P = self.Pixelate;

let held = null; // { token, source }

function decode(input) {
  if (input.kind === 'gif') return P.decodeGif(new Uint8Array(input.data));
  return P.fromRGBA(input.width, input.height, new Uint8ClampedArray(input.data));
}

self.onmessage = ({ data: m }) => {
  try {
    if (m.input) held = { token: m.token, source: decode(m.input) };
    if (held?.token !== m.token) return self.postMessage({ id: m.id, gone: true });
    const t = Date.now(), { opaque, plainBg } = held.source;
    const opts = P.normalizeOpts(m.opts, { plainBg });
    const art = P.pixelate(held.source, opts), out = P.encode(art);
    self.postMessage({ id: m.id, data: out.data, ext: out.ext, opts, opaque, frames: art.frames.length, ms: Date.now() - t });
  } catch (err) { self.postMessage({ id: m.id, error: String(err?.message || err) }); }
};
