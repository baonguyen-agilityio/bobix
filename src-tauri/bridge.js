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
    update: act => invoke('update_act', { act: String(act) }), // check | install | later | page: updating Bobix itself (src-tauri/src/update.rs)
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
    cancelNew: (dir, iid) => invoke('work_cancel', { dir, iid: iid ?? null }), // stop waiting for a new session in that folder (for one issue)
    newReview: (project, branch) => invoke('work_new_review', { project, branch }), // the forge's new PR / MR page for that branch
    // prompt: prefilled in the new session's prompt box (claude://code/new?q=…)
    // dir: a folder the habit log names, for a project no session has open here now (Bobix checks its git remote)
    startIssue: (path, iid, prompt, dir) => (typeof path === 'string' && Number.isInteger(iid) ? invoke('work_start', { path, iid, prompt: typeof prompt === 'string' ? prompt : null, dir: typeof dir === 'string' ? dir : null }) : Promise.resolve(null)),
    habitsRead: () => invoke('habits_read'), // what Bobix remembers of your work (only while Settings → habits is on)
    habitsClear: () => invoke('habits_clear'), // "Xoá hết"
    habitsNote: what => invoke('habits_note', { what }), // standup | report opened
    showBoard: () => invoke('show_board'),
    openPrices: () => invoke('usage_prices'), // the editable price table of the usage estimate, in the person's editor
    mySkills: () => invoke('skills_mine'), // your own skills, for Settings → My skills
    // Skills audit (src-tauri/src/skill_audit.rs). dir: a folder a session here works in, or null for your own skills only.
    skillsAudit: dir => invoke('skills_audit', { dir: typeof dir === 'string' ? dir : null }),
    // act: disable | remove, key: a skill's key from the audit. Only on the person's click: backed up first, undoable.
    changeSkill: (dir, key, act) => invoke('skills_change', { dir: typeof dir === 'string' ? dir : null, key: String(key), act: String(act) }),
    skillBackups: () => invoke('skills_backups'),
    undoSkill: id => invoke('skills_undo', { id: String(id) }), // a skill change or the CLAUDE.md rule block
    routingRules: dir => invoke('skills_rules', { dir: String(dir), add: false }), // { file, present, text, block, undo }
    addRoutingRules: dir => invoke('skills_rules', { dir: String(dir), add: true }), // only after showing the exact text
    // From your habits (src-tauri/src/habit_skills.rs): act view | scan | draft | approve | dismiss | restore | undo | remove.
    // Answers the view, with the act's { ok, error } in `result`. Changes run only on the person's click.
    // The war room (src-tauri/src/room.rs): act list | create | invite | uninvite | frame | add | summarize | decide | audit | markdown | get | radd | leave | open.
    room: (act, a) => invoke('room', { act: String(act), a: a && typeof a === 'object' ? a : {} }),
    habitSkills: (act, id, text, v) => invoke('habit_skills', { act: String(act || 'view'), id: id == null ? null : String(id), text: text == null ? null : String(text), v: Number.isFinite(v) ? v : null }),
    processStatus: () => invoke('process_status'), // per project: the repository's .bobix/skills.json and the imported process
    // a process for a project, from a file's text or an https URL; { remove: true } forgets the imported one
    importProcess: (project, v) => (typeof project === 'string' ? invoke('process_import', { project, text: v?.text ?? null, url: v?.url ?? null, remove: v?.remove ?? null }) : Promise.resolve(null)),
    // A project not seen on this computer (#163): link the folder you cloned it to (its remote is checked), or clone it
    // with your own glab / gh into a folder you choose. dir / parent: given by the smoke run; the app asks with a dialog.
    // `of`: the issue's project when `path` is the repository its code is in (it may have no issue listed itself)
    linkFolder: (path, dir, of) => invoke('work_link', { path: String(path), dir: typeof dir === 'string' ? dir : null, of: typeof of === 'string' ? of : null }),
    findClone: (path, of) => invoke('work_find', { path: String(path), of: typeof of === 'string' ? of : null }),
    cloneProject: (path, parent, of) => invoke('work_clone', { path: String(path), parent: typeof parent === 'string' ? parent : null, of: typeof of === 'string' ? of : null }),
    workRepos: fresh => invoke('work_repos', { fresh: !!fresh }), // the repo picker: every repository glab / gh can reach, and the ticked ones
    workPage: page => invoke('work_page', { page: Math.max(1, Math.floor(Number(page)) || 1) }), // the Work list's page, read again
    issueBrief: (path, iid) => (typeof path === 'string' && Number.isInteger(iid) ? invoke('work_brief', { path, iid }) : Promise.resolve(null)), // title, description (read now), link
    // The ticket flow (#112): where a ticket would run, Start (a worktree, then Claude desktop or a terminal line),
    // Haiku's split (only on click), the plan's one approval, the log's assumptions seen, a Haiku summary, cleanup.
    // `code`: the repository the code is in, when not the issue's own
    ticketPrep: (path, iid, code) => (typeof path === 'string' && Number.isInteger(iid) ? invoke('ticket_prep', { path, iid, code: typeof code === 'string' ? code : null }) : Promise.resolve(null)),
    ticketStart: (path, iid, prompt, models, how, code, anyway) => (typeof path === 'string' && Number.isInteger(iid) ? invoke('ticket_start', { path, iid, code: typeof code === 'string' ? code : null, prompt: String(prompt || ''), models: models || null, how: how === 'cli' ? 'cli' : 'desktop' , anyway: anyway === true }) : Promise.resolve(null)),
    ticketClassify: (title, body) => invoke('ticket_classify', { title: String(title || ''), body: String(body || '') }),
    ticketAnswer: (key, approve, change) => invoke('ticket_answer', { key: String(key), approve: !!approve, change: typeof change === 'string' ? change : null }),
    ticketSeen: key => invoke('ticket_seen', { key: String(key) }),
    ticketSummary: key => invoke('ticket_summary', { key: String(key) }),
    usageEffects: () => invoke('usage_effects'),
    ticketDetail: key => invoke('ticket_detail', { key: String(key) }),
    ticketRemove: (key, act) => invoke('ticket_remove', { key: String(key), act: ['check', 'keep', 'delete'].includes(act) ? act : 'check' }),
    ticketClean: (key, force) => invoke('ticket_clean', { key: String(key), force: force === true }), // force: also with commits not pushed
    ticketCi: key => invoke('ticket_ci', { key: String(key) }), // the MR's pipeline, read now (glab api, a GET)
    // the project's CLAUDE.md: does it name a test command? With cmd: add the one-line Tests entry (backed up, undoable)
    claudeTests: (dir, cmd) => invoke('claude_tests', { dir: String(dir), cmd: typeof cmd === 'string' ? cmd : null }),
    ticketMr: (key, title, description) => invoke('ticket_mr', { key: String(key), title: String(title || ''), description: String(description || '') }), // push + glab mr create, on your click
    ticketReview: (key, how) => invoke('ticket_review', { key: String(key), how: how === 'cli' ? 'cli' : 'desktop' }), // a second session reviews a done flow
    onSettings: on('settings'), onUpdate: on('update'), onSelect: on('select'), onShowTab: on('show-tab'), onOpenFail: on('open-fail'),
    // overlay only
    petClick: (id, force) => invoke('pet_click', { id, force: force === true }),
    setRects: r => invoke('rects', { r }),
    onMouse: on('mouse'),
  };
})();
