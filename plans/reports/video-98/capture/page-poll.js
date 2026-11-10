// Evidence-only page script (both windows): runs the scripts queued on cmd-server.mjs; results go back to its /log.
(() => {
  const w = location.pathname.startsWith('/pets') ? 'pets' : 'panel';
  const log = m => fetch('http://127.0.0.1:4799/log?w=' + w, { method: 'POST', body: String(m) }).catch(() => {});
  setInterval(async () => {
    try { const r = await fetch('http://127.0.0.1:4799/next?w=' + w); if (r.status !== 200) return;
      const code = await r.text(); try { const v = await (0, eval)(code); if (v !== undefined) log(typeof v === 'string' ? v : JSON.stringify(v)); } catch (e) { log('error: ' + e.message); } } catch {}
  }, 250);
})();
