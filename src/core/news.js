// First-run guide and "What's new": which of them the panel shows. Both are remembered in settings
// (guideSeen, seenVersion) and nothing is fetched: the notes ship inside the app (ui/i18n.js, NEWS).
const base = v => String(v || '').replace(/-.*/, ''); // 0.1.4-rc.3 → 0.1.4: a test build shows its release's notes

// Once, the first time this version of Bobix starts with these settings. `existing`: hooks were already installed, so
// this person used an older Bobix: skip the guide, keep the notes. A new install sees the guide, not the notes.
function firstStart(settings, version, existing) {
  if (settings.seenVersion) return null;
  return existing ? { guideSeen: true } : { seenVersion: base(version) };
}

// The version whose notes the panel shows, or null.
const newsFor = (settings, version, notesVersion) => (base(version) === notesVersion && settings.seenVersion !== notesVersion ? notesVersion : null);

module.exports = { base, firstStart, newsFor };
