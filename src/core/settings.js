// User settings: defaults, validation and a JSON file in the app's data folder.
const fs = require('fs');
const path = require('path');

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
  approveWait: 20,         // seconds a permission request waits for Allow / Deny in Bobix before the terminal asks (0 = off)
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
};

const ENUMS = { language: ['auto', 'en', 'vi'], edge: ['bottom', 'top'], label: ['title', 'folder', 'both'], clickAction: ['focus', 'panel'] };
const RANGES = { port: [1024, 65535], zoneStart: [0, 95], zoneEnd: [5, 100], size: [32, 128], opacity: [0.2, 1], sleepHide: [0, 1440], approveWait: [0, 300] };

// Keeps only known keys with valid values; anything else falls back to the current value.
function sanitize(input, base = DEFAULTS) {
  const out = { ...base };
  for (const [k, def] of Object.entries(DEFAULTS)) {
    if (!(k in (input || {}))) continue;
    const v = input[k];
    if (ENUMS[k]) { if (ENUMS[k].includes(v)) out[k] = v; }
    else if (k === 'port') { const n = Number(v); if (Number.isInteger(n) && n >= RANGES.port[0] && n <= RANGES.port[1]) out.port = n; } // reject, never clamp: a guessed port could be busy
    else if (RANGES[k]) { const n = Number(v); if (Number.isFinite(n)) out[k] = Math.min(RANGES[k][1], Math.max(RANGES[k][0], k === 'opacity' ? n : Math.round(n))); }
    else if (typeof def === 'boolean') { if (typeof v === 'boolean') out[k] = v; }
    else if (k === 'display') { if (typeof v === 'string' && v.length < 40) out[k] = v; }
  }
  if (out.zoneEnd - out.zoneStart < 5) out.zoneEnd = Math.min(100, out.zoneStart + 5);
  return out;
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

module.exports = { DEFAULTS, sanitize, load, save };
