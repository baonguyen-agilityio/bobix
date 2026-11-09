// window.bobix for the pages served from 127.0.0.1, backed by the app's commands (main.rs): the pages' only way in.
// The app sends events to the page with window.__bobixEmit(channel, value).
(() => {
  if (window.bobix || !window.__TAURI_INTERNALS__) return;
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  const subs = {};
  window.__bobixEmit = (ch, v) => (subs[ch] || []).forEach(cb => { try { cb(v); } catch (err) { console.error(err); } });
  const on = ch => cb => { (subs[ch] ||= []).push(cb); };
  // The picture import lives in its own file, loaded the first time the panel needs it.
  let pets = null;
  const petImport = () => (pets ||= new Promise((ok, no) => {
    const s = document.createElement('script');
    s.src = '/ui/pet-import.js';
    s.onload = () => ok(window.__bobixPetImport(invoke));
    s.onerror = () => { pets = null; no(new Error('pet-import.js did not load')); };
    document.head.appendChild(s);
  }));

  // Errors the page logs or throws, for diagnostics and the smoke run.
  const report = msg => invoke('page_error', { msg: String(msg).slice(0, 300) }).catch(() => {});
  const logError = console.error.bind(console);
  console.error = (...a) => { report(a.map(String).join(' ')); logError(...a); };
  addEventListener('error', e => report('uncaught: ' + e.message + ' @' + e.lineno));
  addEventListener('unhandledrejection', e => report('rejection: ' + e.reason));

  window.bobix = {
    getState: () => invoke('state'),
    setSettings: patch => invoke('settings_set', { patch }),
    resetSettings: () => invoke('settings_reset'),
    revealSettings: () => invoke('reveal_settings'),
    copyDiagnostics: () => invoke('diagnostics_copy'),
    feedbackInfo: () => invoke('feedback_info'),
    sendFeedback: v => invoke('feedback_send', { v: v || {} }), // opens GitHub's form; the person submits it there
    installHooks: () => invoke('install_hooks'), uninstallHooks: () => invoke('uninstall_hooks'),
    testEvent: () => invoke('test_event'),
    openSession: id => invoke('open_session', { id }),
    answer: (id, choice) => invoke('answer', { id, choice }), // a waiting permission request: allow | always | deny
    petImage: (id, act, opts) => petImport().then(f => f(id, act, opts)), // act: choose | edit | preview | save | cancel | reset
    setPet: (id, mode) => invoke('pet_set', { id, mode }), // mode: auto | hidden | shown; id '*' = every pet
    refreshWork: () => invoke('work_refresh'), // GitLab issues, read again now (only while GitLab is on)
    openIssue: url => invoke('work_open', { url }), // in the browser; only links to the configured GitLab
    cancelNew: dir => invoke('work_cancel', { dir }), // stop waiting for a new session in that folder
    newReview: (project, branch) => invoke('work_new_review', { project, branch }), // the forge's new PR / MR page for that branch
    // prompt: prefilled in the new session's prompt box (claude://code/new?q=…)
    startIssue: (path, iid, prompt) => (typeof path === 'string' && Number.isInteger(iid) ? invoke('work_start', { path, iid, prompt: typeof prompt === 'string' ? prompt : null }) : Promise.resolve(null)),
    issueBrief: (path, iid) => (typeof path === 'string' && Number.isInteger(iid) ? invoke('work_brief', { path, iid }) : Promise.resolve(null)), // title, description (read now), link
    onSettings: on('settings'), onSelect: on('select'), onShowTab: on('show-tab'), onOpenFail: on('open-fail'),
    // overlay only
    petClick: id => invoke('pet_click', { id }),
    setRects: r => invoke('rects', { r }),
    onMouse: on('mouse'),
  };
})();
