// User settings: defaults, validation and a JSON file in the app's data folder.
const fs = require('fs');
const path = require('path');
const { normalizeOpts } = require('./pixelate');

const DEFAULTS = {
  language: 'auto',        // auto | en | vi
  port: 4747,
  display: 'primary',      // 'primary' | display id (string)
  edge: 'bottom',          // bottom | top
  zoneStart: 0,            // % of screen width the pets may use
  zoneEnd: 100,
  size: 56,                // px
  opacity: 1,
  fadeNearCursor: true,    // fade pets when the cursor passes near them (not over them)
  showIdle: true,
  approveWait: -1,         // seconds a permission request waits for Allow / Deny in Bobix before the terminal asks (0 = off, -1 = until you answer)
  sleepHide: 30,           // minutes a pet may sleep on screen before it leaves (0 = never); it comes back with the next prompt
  onlyAttention: false,    // only pets that wait for you or just finished
  label: 'title',          // title | folder | both
  clickAction: 'focus',    // focus (bring the session's window forward) | panel
  notifyWaiting: true,
  notifyDone: true,
  sound: true,
  paused: false,           // do-not-disturb: no notifications
  petsVisible: true,
  autostart: false,
  checkUpdates: true,      // once a day, ask GitHub whether a newer release exists (nothing about you is sent)
  guideSeen: false,        // the panel's "How to read the pets" card was closed (see news.js)
  seenVersion: '',         // the last version whose "What's new" was closed, or the version first installed
  gitlab: false,           // Work tab: read the GitLab issues assigned to you through your own glab (off: no network for it)
  gitlabUrl: '',           // e.g. https://gitlab.com or a self-hosted GitLab; only its origin is kept
  petImages: {},           // project folder -> { src, file, opts } in the app's data folder (see petstore.js), or a file name
};

const PET_FILE = /^[a-f0-9]{24}\.(png|gif)$/;
const PET_SRC = /^[a-f0-9]{24}\.(png|jpg|gif|webp)$/;
const ENUMS = { language: ['auto', 'en', 'vi'], edge: ['bottom', 'top'], label: ['title', 'folder', 'both'], clickAction: ['focus', 'panel'] };
const UNTIL_ANSWER = -1; // approveWait: no countdown, the request waits until you answer (or Claude moves on)
const RANGES = { port: [1024, 65535], zoneStart: [0, 95], zoneEnd: [5, 100], size: [32, 128], opacity: [0.2, 1], sleepHide: [0, 1440], approveWait: [0, 300] };

// A picture entry: a file name (made before pictures were processed) or { src, file, opts }; null when it is neither.
function petEntry(e) {
  if (typeof e === 'string') return PET_FILE.test(e) ? e : null;
  if (!e || typeof e !== 'object' || !PET_SRC.test(e.src) || !PET_FILE.test(e.file)) return null;
  return { src: e.src, file: e.file, opts: normalizeOpts(e.opts) };
}

// Keeps only known keys with valid values; anything else falls back to the current value.
function sanitize(input, base = DEFAULTS) {
  const out = { ...base };
  for (const [k, def] of Object.entries(DEFAULTS)) {
    if (!(k in (input || {}))) continue;
    const v = input[k];
    if (ENUMS[k]) { if (ENUMS[k].includes(v)) out[k] = v; }
    else if (k === 'port') { const n = Number(v); if (Number.isInteger(n) && n >= RANGES.port[0] && n <= RANGES.port[1]) out.port = n; } // reject, never clamp: a guessed port could be busy
    else if (k === 'approveWait' && Number(v) === UNTIL_ANSWER) out[k] = UNTIL_ANSWER; // any other negative number is clamped to 0 (off) below
    else if (RANGES[k]) { const n = Number(v); if (Number.isFinite(n)) out[k] = Math.min(RANGES[k][1], Math.max(RANGES[k][0], k === 'opacity' ? n : Math.round(n))); }
    else if (typeof def === 'boolean') { if (typeof v === 'boolean') out[k] = v; }
    else if (k === 'seenVersion') { if (v === '' || /^\d{1,4}\.\d{1,4}\.\d{1,4}$/.test(v)) out[k] = v; }
    else if (k === 'gitlabUrl') { if (v === '') out[k] = ''; else if (typeof v === 'string' && v.length < 300) { try { const u = new URL(v.trim()); if (/^https?:$/.test(u.protocol) && u.hostname) out[k] = u.origin; } catch {} } }
    else if (k === 'display') { if (typeof v === 'string' && v.length < 40) out[k] = v; }
    else if (k === 'petImages') {
      if (!v || typeof v !== 'object' || Array.isArray(v)) continue;
      out.petImages = Object.fromEntries(Object.entries(v).map(([p, e]) => [p, petEntry(e)]).filter(([p, e]) => p.length < 1024 && e).slice(0, 200));
    }
  }
  if (out.zoneEnd - out.zoneStart < 5) out.zoneEnd = Math.min(100, out.zoneStart + 5);
  return out;
}

// How long a permission request of session `s` waits for an answer in Bobix: seconds, Infinity (until answered) or 0
// (don't hold it, Claude asks as usual). Never holds what the person may not see: pets hidden, notifications paused, or this pet hidden by hand.
function holdSecs(settings, s) {
  const w = settings.approveWait;
  if (!w || settings.paused || !settings.petsVisible || s?.pet === 'hidden') return 0;
  return w === UNTIL_ANSWER ? Infinity : w;
}

function load(file) {
  try { return sanitize(JSON.parse(fs.readFileSync(file, 'utf8'))); } catch { return { ...DEFAULTS }; }
}

function save(file, settings) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = file + '.tmp';
  fs.writeFileSync(tmp, JSON.stringify(settings, null, 2));
  fs.renameSync(tmp, file);
}

module.exports = { DEFAULTS, UNTIL_ANSWER, sanitize, holdSecs, load, save, PET_FILE };
