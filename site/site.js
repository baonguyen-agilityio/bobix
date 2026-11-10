// Bobix site: OS tabs in the install section, and copy buttons for the terminal lines.
(() => {
  const me = document.currentScript.dataset, $$ = s => [...document.querySelectorAll(s)];
  const tabs = $$('.ostabs [role=tab]');
  const pick = b => {
    tabs.forEach(x => { const on = x === b; x.setAttribute('aria-selected', on); x.tabIndex = on ? 0 : -1; });
    $$('.ospanel').forEach(p => { p.hidden = p.dataset.panel !== b.dataset.os; });
  };
  tabs.forEach((b, i) => {
    b.addEventListener('click', () => pick(b));
    b.addEventListener('keydown', e => {
      const d = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
      if (d) { const n = tabs[(i + d + tabs.length) % tabs.length]; pick(n); n.focus(); }
    });
  });
  // Open the tab for the visitor's computer when it is easy to tell.
  const ua = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '';
  const guess = /win/i.test(ua) ? 'win' : /linux/i.test(ua) && !/android/i.test(navigator.userAgent) ? 'lin' : null;
  if (guess) pick(tabs.find(b => b.dataset.os === guess));

  $$('.copy').forEach(b => b.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(b.previousElementSibling.textContent); b.textContent = me.copied; }
    catch { b.textContent = me.fail; }
    setTimeout(() => { b.textContent = me.copy; }, 1500);
  }));
})();
