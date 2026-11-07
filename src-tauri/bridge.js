// window.bobix for the pages served from 127.0.0.1, as src/preload.js gives it in Electron, backed by Tauri commands.
// The app sends events to the page with window.__bobixEmit(channel, value). Calls not ported yet answer null.
(() => {
  if (window.bobix || !window.__TAURI_INTERNALS__) return;
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  const subs = {};
  window.__bobixEmit = (ch, v) => (subs[ch] || []).forEach(cb => { try { cb(v); } catch (err) { console.error(err); } });
  const on = ch => cb => { (subs[ch] ||= []).push(cb); };
  const later = () => Promise.resolve(null);

  // Errors the page logs or throws, for diagnostics and the smoke run (Electron reads them from console-message).
  const report = msg => invoke('page_error', { msg: String(msg).slice(0, 300) }).catch(() => {});
  const logError = console.error.bind(console);
  console.error = (...a) => { report(a.map(String).join(' ')); logError(...a); };
  addEventListener('error', e => report('uncaught: ' + e.message + ' @' + e.lineno));
  addEventListener('unhandledrejection', e => report('rejection: ' + e.reason));

  window.bobix = {
    getState: () => invoke('state'),
    setSettings: later, resetSettings: later, revealSettings: later, copyDiagnostics: later, feedbackInfo: later, sendFeedback: later,
    installHooks: () => invoke('install_hooks'), uninstallHooks: () => invoke('uninstall_hooks'),
    testEvent: () => invoke('test_event'),
    openSession: id => invoke('open_session', { id }),
    answer: (id, choice) => invoke('answer', { id, choice }),
    petImage: later,
    setPet: (id, mode) => invoke('pet_set', { id, mode }),
    refreshWork: later, openIssue: later, cancelNew: later, startIssue: later,
    onSettings: on('settings'), onSelect: on('select'), onShowTab: on('show-tab'), onOpenFail: on('open-fail'),
    // overlay only
    petClick: id => invoke('pet_click', { id }),
    setRects: r => invoke('rects', { r }),
    onMouse: on('mouse'),
  };
})();
