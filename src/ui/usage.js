// A session's usage (the app's usage.rs sends it as numbers) as short text for the board, the details and the pet card.
// Thin on purpose: the panel's layout will change, the data stays. Works in the browser (window.USAGE) and in Node.
(function (root) {
  // claude-opus-5-5 → Opus 5.5, claude-haiku-4-5-20251001 → Haiku 4.5
  function modelName(id) {
    const m = String(id || '').match(/(opus|sonnet|haiku|fable|mythos)-(\d+)(?:-(\d{1,2}))?(?!\d)/i);
    return m ? `${m[1][0].toUpperCase()}${m[1].slice(1)} ${m[2]}${m[3] ? '.' + m[3] : ''}` : String(id || '?');
  }
  const tokens = n => (n >= 1e6 ? `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : String(n || 0));
  const dur = ms => { const m = Math.round(ms / 60e3); return m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, '0')}m`; };
  const usd = x => '$' + (x < 10 ? x.toFixed(2) : Math.round(x).toLocaleString('en-US'));
  const pct = x => `${Math.round(x * 100)}%`;
  const times = x => `${x < 10 ? x.toFixed(1) : Math.round(x)}×`;

  // Models by share of turns, the session's own first: [{ name, side, share }]
  function mix(u) {
    const all = (u?.models || []).filter(m => m.turns > 0);
    const out = [];
    for (const side of [false, true]) {
      const xs = all.filter(m => !!m.side === side), n = xs.reduce((a, m) => a + m.turns, 0);
      for (const m of xs.sort((a, b) => b.turns - a.turns)) out.push({ name: modelName(m.model), side, share: m.turns / n, turns: m.turns });
    }
    return out;
  }

  // Cost, only when the app sent it (Settings → usageCost) and the page's settings agree.
  const costOf = (u, settings) => (u?.cost && settings?.usageCost !== false ? u.cost : null);

  // One line: "3.1× your usual · 1h 20m active · ≈ $42 API est."
  function line(u, T, settings) {
    if (!u) return '';
    const c = costOf(u, settings);
    return [u.times != null ? T('use.times', { x: times(u.times) }) : T('use.turns', { n: u.mainTurns }), T('use.active', { t: dur(u.activeMs) }),
      c ? T('use.costShort', { usd: usd(c.usd) }) : ''].filter(Boolean).join(' · ');
  }

  // Each warning with its reason and what to do.
  function warns(u, T) {
    return (u?.warnings || []).map(w => {
      const p = { model: modelName(w.model), n: w.n ?? w.turns, out: w.out, ctx: tokens(w.ctx), limit: w.why === 'normal' ? times(w.limit) : tokens(w.limit), x: w.times != null ? times(w.times) : '' };
      return { kind: w.kind, text: T(`use.w.${w.kind}.${w.why || 'run'}`, p) };
    });
  }

  // The details: tokens, model mix, re-reading share, the estimate with its table's date.
  function facts(u, T, settings) {
    if (!u) return [];
    const t = u.tokens, c = costOf(u, settings);
    const models = mix(u).map(m => `${m.name}${m.side ? ` (${T('use.side')})` : ''} ${pct(m.share)}`).join(', ');
    return [
      [T('use.k.size'), u.times != null ? T('use.times', { x: times(u.times) }) : T('use.learning')],
      [T('use.k.time'), `${dur(u.activeMs)} · ${T('use.turns', { n: u.mainTurns })}`],
      [T('use.k.tokens'), T('use.tokens', { in: tokens(t.input), cw: tokens(t.cacheWrite), cr: tokens(t.cacheRead), out: tokens(t.output) })],
      [T('use.k.reread'), u.rereadCost != null ? T('use.rereadCost', { p: pct(u.reread), c: pct(u.rereadCost) }) : pct(u.reread)],
      [T('use.k.models'), models],
      ...(c ? [[T('use.k.cost'), T('use.cost', { usd: usd(c.usd), date: c.date }) + (c.partial ? ' ' + T('use.partial') : '')]] : []),
      ...(u.tasks ? [[T('use.k.tasks'), Object.entries(u.tasks).map(([k, n]) => `${T('use.task.' + k)} ${n}`).join(', ')]] : []),
    ];
  }

  const api = { modelName, tokens, dur, usd, mix, line, warns, facts };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.USAGE = api;
})(this);
