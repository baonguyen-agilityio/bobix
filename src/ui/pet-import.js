// A project's own pet picture, in the panel (window.bobix.petImage, see src-tauri/bridge.js): the app opens the file
// dialog and stores the result; this page decodes the picture and runs the pixel art in a worker (pixelate-worker.js),
// so the person can adjust options in a preview. One import at a time.
// petImage(id, act, opts), act: choose | edit | preview | save | cancel | reset ->
// { ok, preview, opts, opaque, animated, frames, ms } or { ok: false, reason: bad | big | gone | save }.
window.__bobixPetImport = invoke => {
  const DECODE_SIDE = 1024; // long side before the pixel art shrinks it
  const PIXEL_WAIT_MS = 20e3, DECODE_WAIT_MS = 15e3; // longer than this, a decoder is stuck on a broken file
  let importing = null; // { project, original, kind, input, token }
  let worker = null, workerToken = null, req = 0;

  const toB64 = u8 => { let s = ''; for (let i = 0; i < u8.length; i += 0x8000) s += String.fromCharCode(...u8.subarray(i, i + 0x8000)); return btoa(s); };
  const fromB64 = s => Uint8Array.from(atob(s), c => c.charCodeAt(0));
  const gone = { ok: false, reason: 'gone' };

  // A stopped worker fails every request waiting on it; the next request starts a new one.
  function stop(w, err) {
    if (worker === w) { worker = null; workerToken = null; }
    w.terminate();
    for (const r of w.waiting.values()) { clearTimeout(r.timer); r.reject(err); }
    w.waiting.clear();
  }
  function pixelWorker() {
    if (worker) return worker;
    const w = worker = new Worker('/ui/pixelate-worker.js');
    w.waiting = new Map();
    w.onmessage = ({ data: m }) => { const r = w.waiting.get(m.id); if (!r) return; w.waiting.delete(m.id); clearTimeout(r.timer); m.error ? r.reject(new Error(m.error)) : r.resolve(m); };
    w.onerror = e => { e.preventDefault(); stop(w, new Error(e.message || 'pixel worker failed')); };
    return w;
  }
  const runPixel = msg => new Promise((resolve, reject) => {
    const w = pixelWorker(), id = ++req;
    w.waiting.set(id, { resolve, reject, timer: setTimeout(() => stop(w, new Error('pixel worker stuck')), PIXEL_WAIT_MS) });
    w.postMessage({ ...msg, id });
  });
  const endImport = () => { importing = null; if (worker) stop(worker, new Error('import ended')); };

  // What the worker needs: a GIF as it is, a still picture as straight RGBA, drawn at no more than DECODE_SIDE.
  async function decode(bytes, kind) {
    if (kind === 'gif') return { kind: 'gif', data: bytes };
    const work = (async () => {
      const bmp = await createImageBitmap(new Blob([bytes], { type: `image/${kind === 'jpg' ? 'jpeg' : kind}` }));
      const k = Math.min(1, DECODE_SIDE / Math.max(bmp.width, bmp.height));
      const w = Math.max(1, Math.round(bmp.width * k)), h = Math.max(1, Math.round(bmp.height * k));
      const c = document.createElement('canvas');
      c.width = w; c.height = h;
      const g = c.getContext('2d');
      g.imageSmoothingQuality = 'high';
      g.drawImage(bmp, 0, 0, w, h);
      bmp.close?.();
      return { kind: 'rgba', width: w, height: h, data: g.getImageData(0, 0, w, h).data };
    })();
    return Promise.race([work, new Promise((_, no) => setTimeout(() => no(new Error('decode timed out')), DECODE_WAIT_MS))]);
  }

  // The pixel art of the import in progress: `reply` for the panel, and the encoded `result` with it when reply.ok.
  async function runImport(p, opts) {
    let r;
    try {
      r = await runPixel({ token: p.token, input: workerToken !== p.token ? p.input : undefined, opts });
      if (r.gone) return { reply: gone };
      workerToken = p.token;
    } catch (err) {
      if (importing !== p) return { reply: gone }; // its worker was stopped by a newer import or a cancel
      console.warn('[bobix] pet picture:', err.message);
      return { reply: { ok: false, reason: 'bad' } };
    }
    if (importing !== p) return { reply: gone }; // cancelled or replaced while it ran
    const data = new Uint8Array(r.data);
    return { result: { data, ext: r.ext }, reply: { ok: true, preview: `data:image/${r.ext};base64,${toB64(data)}`, opts: r.opts, opaque: r.opaque, animated: r.frames > 1, frames: r.frames, ms: r.ms } };
  }

  // choose and edit: make `original` the import in progress and answer with its first preview.
  async function startImport(project, original, kind, opts) {
    let input = null;
    try { input = await decode(original, kind); } catch (err) { console.warn('[bobix] pet picture:', err.message); }
    if (!input) return { ok: false, reason: 'bad' };
    endImport();
    const p = importing = { project, original, kind, input, token: Math.random().toString(36).slice(2) };
    const { reply } = await runImport(p, opts);
    if (!reply.ok && importing === p) endImport();
    return reply;
  }
  const fromApp = async (call, opts) => { const r = await call; return r?.ok ? startImport(r.project, fromB64(r.data), r.kind, opts ?? r.opts) : r; };

  const acts = {
    choose: id => fromApp(invoke('pet_choose', { id })),
    edit: id => fromApp(invoke('pet_original', { id })),
    preview: async (id, opts) => (importing ? (await runImport(importing, opts)).reply : gone),
    async save(id, opts) {
      const p = importing;
      if (!p) return gone;
      const { reply, result } = await runImport(p, opts);
      if (!reply.ok) return reply;
      const r = await invoke('pet_save', { id, project: p.project, original: toB64(p.original), result: toB64(result.data), ext: result.ext, opts: reply.opts });
      if (r?.ok && importing === p) endImport();
      return r;
    },
    cancel: async () => { endImport(); return { ok: true }; },
    reset: id => invoke('pet_reset', { id }),
  };
  const petImage = (id, act, opts) => (typeof id === 'string' && Object.hasOwn(acts, act) ? acts[act](id, opts && typeof opts === 'object' ? opts : undefined) : Promise.resolve(null));
  petImage.startImport = startImport; // the smoke run imports a picture it drew, without the file dialog
  window.__bobixPetImage = petImage;
  return petImage;
};
