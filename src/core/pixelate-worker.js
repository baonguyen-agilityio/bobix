// Runs the pet picture pixel art off the main thread (main.js starts it as a worker thread; see runPixel there).
// It holds the decoded source of the one import in progress, so adjusting an option only repeats the pixel art.
// { id, token, input?, opts } in -> { id, data, ext, opts, opaque, frames, ms } | { id, gone } | { id, error } out.
const { parentPort } = require('worker_threads');
const P = require('../ui/pixelate');
const { fromBitmap } = require('./petstore');

let held = null; // { token, source }

// What main.js hands over: a GIF as it is, or one decoded still (BGRA from nativeImage, RGBA from a renderer).
function decode(input) {
  if (input.kind === 'gif') return P.decodeGif(Buffer.from(input.data.buffer, input.data.byteOffset, input.data.length));
  const rgba = input.kind === 'bgra' ? fromBitmap(input.data) : new Uint8ClampedArray(input.data.buffer, input.data.byteOffset, input.data.length);
  return P.fromRGBA(input.width, input.height, rgba);
}

parentPort.on('message', m => {
  try {
    if (m.input) held = { token: m.token, source: decode(m.input) };
    if (held?.token !== m.token) return parentPort.postMessage({ id: m.id, gone: true });
    const t = Date.now(), { opaque, plainBg } = held.source;
    const opts = P.normalizeOpts(m.opts, { plainBg });
    const art = P.pixelate(held.source, opts), out = P.encode(art);
    parentPort.postMessage({ id: m.id, data: out.data, ext: out.ext, opts, opaque, frames: art.frames.length, ms: Date.now() - t });
  } catch (err) { parentPort.postMessage({ id: m.id, error: String(err?.message || err) }); }
});
