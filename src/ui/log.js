// A session's log as the panel lists it. Works in the browser (window.LOG) and in Node (require).
(function (root) {
  // Newest first; the same tool on the same thing several times in a row is one line with a count, timed by its newest
  // repeat (`from` keeps the first one's time, which stays the same as repeats come in).
  function fold(log) {
    const out = [];
    for (const l of log || []) {
      const prev = out[out.length - 1];
      if (prev && l.kind === 'tool' && prev.kind === 'tool' && l.tool === prev.tool && l.text === prev.text) { prev.n++; prev.t = l.t; }
      else out.push({ ...l, n: 1, from: l.t });
    }
    return out.reverse();
  }
  // Long commands and paths lose their middle, not their end: the file name is what tells them apart.
  const mid = (s, n = 60) => { s = String(s ?? ''); if (s.length <= n) return s; const head = Math.ceil((n - 1) * 0.4); return s.slice(0, head) + '…' + s.slice(head - n + 1); };

  // Every line starts with a capital; tool labels are lower case to sit inside the panel's other sentences.
  const cap = (s, locale) => { s = String(s ?? ''); return s.charAt(0).toLocaleUpperCase(locale) + s.slice(1); };

  const api = { fold, mid, cap };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.LOG = api;
})(this);
