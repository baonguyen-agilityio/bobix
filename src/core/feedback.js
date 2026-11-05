// Feedback (a bug or an idea): a link that opens GitHub's "new issue" form, filled in. The person reads it there and submits it
// themselves, so Bobix needs no token and no server. Issues are public: only listed, non-identifying facts go in.

// GitHub answered a 6,500-character link and failed on 6,900 (checked by hand): stay under that.
const MAX_URL = 6000;

// Takes what could identify someone out of an error message: home folder, folders of a path, tokens.
function redact(text, home = '') {
  let out = String(text);
  if (home) out = out.split(home).join('~');
  return out
    .replace(/([?&]t=)[^\s&]+/g, '$1[redacted]')
    .replace(/\b(?:sk-[\w-]{10,}|[a-f0-9]{32,})\b/gi, '[redacted]')
    .replace(/(?:~|[A-Za-z]:)?(?:[\\/][^\s"'`:,()\\/]+){2,}/g, m => `.../${m.split(/[\\/]/).pop()}`);
}

// The facts from diagnostics() a report needs. A list of fields, not a list of exclusions, so that something added to
// diagnostics later stays out until it is checked. Left out on purpose: process names and paths, environment names.
function summarize(d, errors = [], home = '') {
  const s = d.settings || {}, h = d.hooks || {}, o = d.overlay || {};
  const lines = [
    `- Bobix ${d.app}, ${d.platform} ${d.release} ${d.arch}${d.wayland ? ', Wayland' : ''}${d.ozone ? `, ozone ${d.ozone}` : ''}`,
    `- Hooks: ${h.mode || 'none'}, complete: ${!!h.complete}, stale: ${!!h.stale}, outdated: ${!!h.outdated}, port: ${h.port ?? '-'}, node: ${!!d.node}${h.error ? `, error: ${redact(h.error, home)}` : ''}`,
    `- Displays: ${(d.displays || []).map(x => `${x.scale}x${x.primary ? ' (primary)' : ''}`).join(', ') || '-'}`,
    `- Pets window: ready: ${!!o.ready}, visible: ${!!o.visible}, always on top: ${!!o.alwaysOnTop}, pets: ${o.page?.pets ?? '-'}`,
    `- Settings: pets visible: ${s.petsVisible}, show idle: ${s.showIdle}, only attention: ${s.onlyAttention}, sleep hide: ${s.sleepHide}, edge: ${s.edge}, display: ${s.display}, click: ${s.clickAction}`,
    `- Sessions: ${(d.sessions || []).map(x => `${x.state} ${x.sinceMin}m`).join(', ') || 'none'}`,
  ];
  const errs = errors.slice(-10).map(e => `- ${redact(e, home).slice(0, 300)}`);
  return { lines, errs };
}

// The form's address. Ideas carry no technical details. Long bug reports lose the oldest errors first, then the end of
// the description.
function issueUrl({ repo, kind, title, description, info }) {
  const idea = kind === 'idea', errs = idea ? [] : [...info.errs];
  let text = String(description || '').trim(), truncated = false;
  const build = () => {
    const body = (idea ? ['## The idea', text || '_(not described)_']
      : ['## What happened', text || '_(not described)_', '', '## Technical details (added by Bobix)', ...info.lines, ...(errs.length ? ['', '## Recent errors', ...errs] : [])]).join('\n');
    const heading = `[${idea ? 'Idea' : 'Bug'}] ${String(title).trim().slice(0, 200)}`;
    return `https://github.com/${repo}/issues/new?title=${encodeURIComponent(heading)}&body=${encodeURIComponent(body)}`;
  };
  let url = build();
  while (url.length > MAX_URL && errs.length) { errs.shift(); truncated = true; url = build(); }
  while (url.length > MAX_URL && text) { text = text.slice(0, -100); truncated = true; url = build(); }
  return { url, truncated };
}

module.exports = { redact, summarize, issueUrl, MAX_URL };
