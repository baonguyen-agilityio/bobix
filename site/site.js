// Bobix site: the control panel as a live surface. The pinned "turn" act publishes its scroll progress as --sc-p
// (scroll-craft engine, lib/scrollcraft.js); this file turns that progress into one sample session's state: the
// log fills, the status and the pet change, and at the end the session asks for permission. The visitor answers on
// the pet or in the panel, as in the app; unanswered for 20 s, it falls back to "approve it in the terminal".
// Pets are drawn by the app's own creature() (lib/creature.js). Everything here is page code; the engine is untouched.
(() => {
  const D = JSON.parse(document.getElementById('i18n').textContent);
  const $ = (s, r = document) => r.querySelector(s), $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const fill = (s, o) => s.replace(/\{(\w+)\}/g, (_, k) => o[k] ?? '');
  if (window.ScrollCraft) ScrollCraft.mount(document.body);

  // ---- the sample turn ------------------------------------------------------------------------------------------
  const turn = $('#turn'), lines = $$('[data-log] li[data-at]'), after = $('[data-log] li[data-after]');
  const ask = $('[data-ask]'), within = $('[data-within]'), ecard = $('[data-ecard]'), bub = $('[data-bub]');
  const petEl = $('[data-pet="billing"]'), rowEl = $('[data-id="billing"]');
  const edge = $('[data-edge]'), form = $('[data-settings]');
  const ASK = D.turn.length - 1, WAIT = 20;
  let answer = null, left = WAIT, clock = 0, shown = '';

  // State of "billing" for a given number of passed events: thinking, working on a tool, waiting, or the answer.
  function stateAt(i) {
    if (answer === 'allow' || answer === 'always') return { st: 'done', status: D.status.done, bubble: D.pet.done };
    if (answer === 'deny') return { st: 'idle', status: D.status.idle, bubble: '' };
    if (i >= ASK) return { st: 'waiting', status: fill(D.status.waitingTool, { tool: 'Bash' }), bubble: fill(D.pet.waiting, { tool: 'Bash' }) };
    const e = D.turn[i];
    if (!e || e.kind !== 'tool') return { st: 'working', status: D.status.thinking, bubble: D.status.thinking };
    return { st: 'working', status: fill(D.status.working, { tool: D.tool[e.tool] }), bubble: D.tool[e.tool] };
  }

  function art(st) { return creature('billing', st, null, 4).replace(' width="80%" height="80%"', ''); }

  function render() {
    const p = parseFloat(turn.style.getPropertyValue('--sc-p')) || 0;
    let i = -1;
    D.turn.forEach((e, k) => { if (p >= e.at) i = k; });
    if (answer) i = ASK; // an answer is the visitor's, it stays whatever the scroll does
    lines.forEach((li, k) => { li.classList.toggle('is-on', k <= i); li.classList.toggle('is-now', k === i && !answer); });
    const s = stateAt(i);
    const key = s.st + s.status + (answer || '');
    if (key === shown) return;
    shown = key;

    $('[data-sel-status]').textContent = s.status;
    $('[data-sel-status]').className = 'pill' + (s.st === 'waiting' ? ' pill--wait' : s.st === 'done' ? ' pill--done' : '');
    $('[data-sel-art]').innerHTML = art(s.st);
    petEl.querySelector('[data-art]').innerHTML = art(s.st);
    petEl.classList.toggle('is-waiting', s.st === 'waiting');
    petEl.classList.toggle('is-idle', s.st === 'idle');
    petEl.dataset.st = s.st;
    bub.textContent = s.bubble;
    bub.hidden = !s.bubble;
    bub.className = 'ebub' + (s.st === 'waiting' ? ' ebub--wait' : s.st === 'done' ? ' ebub--done' : '');

    const asking = s.st === 'waiting' && answer !== 'timeout';
    ask.hidden = s.st !== 'waiting';
    ecard.hidden = !asking;
    $$('[data-ask] button').forEach(b => { b.disabled = !asking; });
    if (s.st === 'waiting' && !answer && !clock) startClock();
    if (answer === 'timeout') within.textContent = D.hint;

    after.hidden = !(answer && answer !== 'timeout');
    after.querySelector('code').textContent = answer === 'deny' ? D.denied
      : answer ? `${fill(D.logs.tool, { tool: 'Bash', x: 'npm test' })} · ${D.logs.done} · ${D.answered}` : '';

    // The list in the hero regroups the session, the way the panel does.
    if (rowEl) {
      rowEl.querySelector('[data-status]').textContent = s.status;
      rowEl.querySelector('[data-status]').className = 'pill' + (s.st === 'waiting' ? ' pill--wait' : s.st === 'done' ? ' pill--done' : '');
      rowEl.querySelector('[data-art]').innerHTML = art(s.st);
      const group = s.st === 'waiting' ? 'waiting' : s.st === 'done' ? 'done' : s.st === 'idle' ? 'idle' : 'working';
      const list = $(`[data-list="${group}"]`);
      if (rowEl.parentElement !== list) list.prepend(rowEl);
      counts();
    }
    applyWhich();
  }

  function startClock() {
    left = WAIT;
    within.textContent = fill(D.within, { n: left });
    clock = setInterval(() => {
      if (answer) return stopClock();
      left -= 1;
      within.textContent = fill(D.within, { n: left });
      if (left <= 0) { answer = 'timeout'; stopClock(); shown = ''; render(); }
    }, 1000);
  }
  function stopClock() { clearInterval(clock); clock = -1; }

  function counts() {
    for (const g of ['waiting', 'working', 'done', 'idle']) {
      const n = $(`[data-list="${g}"]`).children.length;
      $(`[data-c="${g}"]`).textContent = n;
      const h = $(`[data-g="${g}"]`);
      h.hidden = !n;
      h.querySelector('small').textContent = n;
    }
  }

  document.addEventListener('click', e => {
    const b = e.target.closest('[data-answer]');
    if (!b || b.disabled || answer) return;
    answer = b.dataset.answer;
    stopClock();
    shown = '';
    render();
  });

  let queued = false;
  addEventListener('scroll', () => { if (!queued) { queued = true; requestAnimationFrame(() => { queued = false; render(); }); } }, { passive: true });
  addEventListener('resize', render);
  render();

  // ---- side sessions join the parent once their act is on screen --------------------------------------------------
  const kids = $$('[data-kid]');
  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver(es => { if (es.some(e => e.isIntersecting)) { kids.forEach(k => { k.hidden = false; }); applyWhich(); io.disconnect(); } }, { threshold: 0.35 });
    io.observe($('#family'));
  } else kids.forEach(k => { k.hidden = false; });

  // ---- settings: real controls for the real pets on this page -----------------------------------------------------
  function applyWhich() {
    const which = form ? new FormData(form).get('which') : 'all';
    $$('.epet', edge).forEach(el => {
      const st = el.dataset.st || (el.classList.contains('is-idle') ? 'idle' : el.querySelector('.balloon--waiting') ? 'waiting' : el.querySelector('.balloon--done') ? 'done' : 'working');
      const show = which === 'all' || (which === 'active' && st !== 'idle') || (which === 'attention' && st === 'waiting');
      el.classList.toggle('is-filtered', !show);
    });
  }
  if (form) form.addEventListener('input', () => {
    const f = new FormData(form);
    edge.classList.toggle('edge--top', f.get('edge') === 'top');
    edge.style.setProperty('--pet-scale', (f.get('size') / 100).toFixed(2));
    edge.style.setProperty('--pet-alpha', (f.get('vis') / 100).toFixed(2));
    applyWhich();
  });

  // ---- tabs follow the part of the panel in view ------------------------------------------------------------------
  const tabs = $$('.tabs a');
  const tabFor = { sessions: 0, turn: 0, family: 0, settings: 1, setup: 2 };
  if ('IntersectionObserver' in window) {
    const seen = new IntersectionObserver(es => es.forEach(e => {
      if (!e.isIntersecting) return;
      tabs.forEach((a, k) => { const on = k === tabFor[e.target.id]; a.classList.toggle('is-on', on); if (on) a.setAttribute('aria-current', 'true'); else a.removeAttribute('aria-current'); });
    }), { rootMargin: '-45% 0px -50% 0px' });
    Object.keys(tabFor).forEach(id => { const el = document.getElementById(id); if (el) seen.observe(el); });
  }

  // ---- setup: pick your computer, get its file and its terminal line ---------------------------------------------
  const cmd = $('#cmd'), osInputs = $$('input[name="os"]');
  function pickOS(k) {
    $$('.osbox').forEach(b => { b.hidden = b.dataset.os !== k; });
    cmd.value = D.cmd[k];
  }
  osInputs.forEach(r => r.addEventListener('change', () => pickOS(r.value)));
  const ua = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '';
  const guess = /win/i.test(ua) ? 'win' : /linux/i.test(ua) && !/android/i.test(navigator.userAgent) ? 'deb' : null;
  if (guess) { const r = osInputs.find(x => x.value === guess); if (r) { r.checked = true; pickOS(guess); } }
  const copyBtn = $('[data-copy]');
  copyBtn.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(cmd.value); } catch { cmd.select(); document.execCommand && document.execCommand('copy'); }
    copyBtn.textContent = D.copied;
    setTimeout(() => { copyBtn.textContent = D.copy; }, 1600);
  });
  cmd.addEventListener('focus', () => cmd.select());

  // ---- the demo plays muted on its own, with its controls, unless motion is reduced -------------------------------
  const demo = $('.demo video');
  if (demo && !matchMedia('(prefers-reduced-motion: reduce)').matches) { demo.autoplay = true; demo.play().catch(() => {}); }
})();
