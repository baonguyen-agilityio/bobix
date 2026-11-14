// BOBIX_SMOKE=<dir>: start, install the hooks (run it with BOBIX_HOME set to a scratch folder), show a pretend session
// and a real one delivered by the installed hook.js, answer a permission request it holds, check the pets overlay is
// on screen, open the panel and use its bridge (state, a settings change that is saved, feedback), check the tray, the
// session's git status and a second start, write <dir>/report.json and <dir>/done, exit 0 (pass) or 1. Port of smoke()
// in the 0.1 (Electron) app's main.js. The panel also imports a picture it draws (decode, pixel art in the worker, save, served, reset).
use crate::{answers, hooks, overlay, panel, paths, Bobix};
use serde_json::{json, Value};
use std::{fs, io::Write, path::PathBuf, process::{Command, Stdio}, sync::{Arc, Mutex}, thread, time::{Duration, Instant}};
use tauri::{AppHandle, Manager};

static PAGE: Mutex<Option<Value>> = Mutex::new(None);
static OPENED: Mutex<Vec<String>> = Mutex::new(vec![]);

pub fn page_result(v: Value) { *PAGE.lock().unwrap() = Some(v); }
// A smoke run never opens other apps (a Start would open the person's own Claude app): the link is noted instead.
pub fn active() -> bool { std::env::var_os("BOBIX_SMOKE").is_some() }
pub fn opened(url: &str) { OPENED.lock().unwrap().push(url.to_string()); }

fn log(m: &str) { println!("[bobix] smoke: {m}"); }

pub fn run(app: AppHandle, b: Arc<Bobix>, dir: PathBuf) {
    thread::spawn(move || {
        let _ = fs::create_dir_all(&dir);
        let mut report = json!({ "platform": crate::platform(), "build": "tauri" });
        let write = |r: &Value| { let _ = fs::write(dir.join("report.json"), serde_json::to_string_pretty(r).unwrap()); };
        {
            let (dir, app) = (dir.clone(), app.clone());
            thread::spawn(move || {
                let extra = ticket_secs() + if std::env::var_os("BOBIX_SMOKE_CLONES").is_some() { 180 } else { 0 } + if std::env::var_os("BOBIX_SMOKE_REPOS").is_some() { 240 } else { 0 } + if std::env::var_os("BOBIX_SMOKE_START").is_some() { 300 } else { 0 } + if std::env::var_os("BOBIX_SMOKE_READS").is_some() { 300 } else { 0 } + if std::env::var_os("BOBIX_SMOKE_NAME").is_some() { 180 } else { 0 } + if std::env::var_os("BOBIX_SMOKE_FOLLOW").is_some() { 400 } else { 0 };
                thread::sleep(Duration::from_secs(90 + extra));
                let _ = fs::write(dir.join("report.json"), json!({ "watchdog": format!("gave up after {} s", 90 + extra) }).to_string());
                log("gave up after 90 s");
                app.exit(2);
            });
        }
        thread::sleep(Duration::from_millis(1500)); // the page loads and subscribes
        if b.core.lock().unwrap().node_path.is_none() { let n = crate::find_bin("node"); b.core.lock().unwrap().node_path = n; }
        // Never into the person's own ~/.claude/settings.json.
        report["install"] = if std::env::var_os("BOBIX_HOME").is_some() { crate::install(&b) } else { json!({ "error": "set BOBIX_HOME to a scratch folder" }) };
        report["hooks"] = hooks::status(&paths::home());
        b.test_event();

        // A session delivered the way Claude Code delivers it: hook.js on stdin, real cwd, a transcript, CLAUDE_PID.
        let transcript = dir.join("real-session.jsonl");
        let lines = [json!({ "type": "ai-title", "aiTitle": "Smoke real session" }),
            json!({ "type": "assistant", "timestamp": "2026-11-10T10:00:00.000Z", "message": { "id": "msg_smoke", "model": "claude-opus-5-5", "content": [{ "type": "text", "text": "Looking at the code." }], "usage": { "input_tokens": 10, "cache_read_input_tokens": 40000 } } })];
        let _ = fs::write(&transcript, lines.iter().map(|l| l.to_string() + "\n").collect::<String>());
        let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned();
        let sid = "3f1c2a9e-7b4d-4e21-9a0c-5d6e7f8a9b0c";
        let hook = hooks::hook_target(&paths::home()); // the copy the install above made
        let mut runs = vec![];
        for ev in [json!({ "hook_event_name": "SessionStart", "source": "startup" }), json!({ "hook_event_name": "UserPromptSubmit", "prompt": "Fix the login bug" }),
            json!({ "hook_event_name": "PreToolUse", "tool_name": "Read", "tool_input": { "file_path": format!("{cwd}/README.md") } })] {
            let mut e = json!({ "session_id": sid, "cwd": cwd, "transcript_path": transcript });
            for (k, v) in ev.as_object().unwrap() { e[k] = v.clone(); }
            runs.push(json!(run_hook(&hook, b.port, &e.to_string())));
        }
        report["hookRuns"] = json!(runs);
        // A permission request the hook holds until it is answered here, as a click on Allow would.
        let ask = json!({ "session_id": sid, "cwd": cwd, "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": { "command": "ls" } }).to_string();
        let (h, port) = (hook.clone(), b.port);
        let held = thread::spawn(move || run_hook_out(&h, port, &ask));
        let until = Instant::now() + Duration::from_secs(5);
        let mut aid = None;
        while aid.is_none() && Instant::now() < until {
            thread::sleep(Duration::from_millis(100));
            aid = b.core.lock().unwrap().sessions.get(sid).and_then(|s| s.answer.as_ref()?["id"].as_str().map(String::from));
        }
        report["answer"] = match aid {
            None => json!("error: not held"),
            Some(id) => {
                // The tray badge counts the held request, and clears once it is answered.
                let applied = || { thread::sleep(Duration::from_millis(600)); crate::tray::APPLIED.load(std::sync::atomic::Ordering::Relaxed) };
                report["badge"] = json!({ "held": applied() });
                answers::answer(&b, &id, "allow");
                let out = held.join().unwrap_or_default();
                report["badge"]["answered"] = json!(applied());
                json!(if out.contains("\"behavior\":\"allow\"") { "ok".to_string() } else { format!("error: hook printed {out:?}") })
            }
        };
        // The process chain of this app and bringing its window forward (the pet click path); not part of the verdict.
        let chain = crate::procs::ancestors(std::process::id());
        report["chain"] = json!(chain);
        report["focus"] = json!(crate::procs::focus_pids(&chain));
        thread::sleep(Duration::from_secs(6));
        {
            let c = b.core.lock().unwrap();
            report["realSession"] = c.sessions.get(sid).map(|s| json!({ "state": s.state, "title": s.title, "name": s.name, "claudePid": s.claude_pid, "hostSession": s.host_session, "usage": s.usage }))
                .unwrap_or(json!("not received"));
            report["sessions"] = json!(c.sessions.len());
        }
        if let Some(w) = app.get_webview_window(overlay::LABEL) {
            let scale = w.scale_factor().unwrap_or(1.0);
            let (p, s) = (w.outer_position().unwrap_or_default().to_logical::<f64>(scale), w.outer_size().unwrap_or_default().to_logical::<f64>(scale));
            report["visible"] = json!(w.is_visible().unwrap_or(false));
            report["onTop"] = json!(w.is_always_on_top().unwrap_or(false));
            report["bounds"] = json!({ "x": p.x, "y": p.y, "width": s.width, "height": s.height });
            let m = overlay::target_monitor(&app, "primary").map(|m| { let (a, f) = (m.work_area(), m.scale_factor()); (a.position.y as f64 / f, (a.position.y as f64 + a.size.height as f64) / f) });
            report["onScreen"] = json!(m.is_some_and(|(top, bottom)| p.y >= top - 1.0 && p.y + s.height <= bottom + 1.0));
            report["loadedUrl"] = json!(w.url().map(|u| u.path().to_string()).unwrap_or_default());
            // The page counts its pets and sends them back through the bridge (the IPC path the pets use).
            let _ = w.eval("window.__TAURI_INTERNALS__.invoke('smoke_page', { data: { pets: document.querySelectorAll('.pet').length, \
                petLabels: [...document.querySelectorAll('.pet .tag')].map(t => t.textContent), \
                petRects: [...document.querySelectorAll('.pet')].map(p => { const r = p.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round); }) } })");
        }
        let until = Instant::now() + Duration::from_secs(5);
        while PAGE.lock().unwrap().is_none() && Instant::now() < until { thread::sleep(Duration::from_millis(100)); }
        match PAGE.lock().unwrap().take() {
            Some(Value::Object(m)) => for (k, v) in m { report[k] = v; },
            _ => report["pets"] = json!("error: no answer from the page"),
        }

        // The panel, through the same bridge the person's clicks use. (Copy diagnostics is left out: it would replace
        // the clipboard of the machine running this.)
        panel::show(&app, &b, None, Some("settings".into()), None);
        let t0 = Instant::now();
        while app.get_webview_window(panel::LABEL).is_none() && t0.elapsed() < Duration::from_secs(15) { thread::sleep(Duration::from_millis(100)); }
        report["panelMs"] = json!(t0.elapsed().as_millis() as u64);
        thread::sleep(Duration::from_millis(3000)); // the page loads and subscribes
        match app.get_webview_window(panel::LABEL) {
            None => report["panel"] = json!("error: no window"),
            Some(w) => {
                let project = b.core.lock().unwrap().sessions.get(sid).and_then(|s| s.project.clone()).unwrap_or_default();
                let _ = w.eval(format!("(async () => {{ const r = {{}}, sid = {sid:?}, project = {project}; try {{ \
                    const st = await bobix.getState(); r.displays = (st.displays || []).length; r.screen = !!(st.screen && st.screen.width); \
                    r.size = (await bobix.setSettings({{ size: 64 }})).size; r.feedbackLines = ((await bobix.feedbackInfo()).lines || []).length; \
                    r.tabs = document.querySelectorAll('.tab[data-tab]').length; r.selected = document.querySelector('.tab[aria-selected=true]')?.dataset.tab || null; \
                    const c = document.createElement('canvas'); c.width = c.height = 80; const g = c.getContext('2d'); \
                    g.fillStyle = '#ffffff'; g.fillRect(0, 0, 80, 80); g.fillStyle = '#e04040'; g.beginPath(); g.arc(40, 40, 28, 0, 7); g.fill(); \
                    g.fillStyle = '#202020'; g.fillRect(28, 30, 8, 8); g.fillRect(46, 30, 8, 8); \
                    const png = new Uint8Array(await (await new Promise(ok => c.toBlob(ok, 'image/png'))).arrayBuffer()); \
                    await bobix.petImage(sid, 'cancel'); const pi = await window.__bobixPetImage.startImport(project, png, 'png'); \
                    const pic = r.picture = {{ preview: !!pi.ok && pi.preview.startsWith('data:image/png;base64,'), ms: pi.ms }}; \
                    pic.save = !!(await bobix.petImage(sid, 'save', pi.opts))?.ok; \
                    const file = (await bobix.getState()).settings.petImages[project]?.file; \
                    pic.served = !!file && (await fetch('/pet-image/' + file + '?t=' + new URLSearchParams(location.search).get('t'))).status === 200; \
                    pic.reset = !!(await bobix.petImage(sid, 'reset'))?.ok; \
                    }} catch (e) {{ r.error = String(e); }} \
                    for (const [k, call] of [['room', async () => (await bobix.room('list', {{}}))?.ok], ['habitSkills', async () => (await bobix.habitSkills('view'))?.result?.ok]]) \
                      try {{ r[k] = (await call()) === true; }} catch (e) {{ r[k] = String(e); }} window.__TAURI_INTERNALS__.invoke('smoke_page', {{ data: {{ panel: r }} }}); }})()", project = json!(project)));
                let until = Instant::now() + Duration::from_secs(10);
                while PAGE.lock().unwrap().is_none() && Instant::now() < until { thread::sleep(Duration::from_millis(100)); }
                report["panel"] = PAGE.lock().unwrap().take().map(|v| v["panel"].clone()).unwrap_or(json!("error: no answer from the panel"));
            }
        }
        thread::sleep(Duration::from_millis(500)); // the 300 ms save
        report["settingsSaved"] = json!(crate::settings::load(&paths::settings_file())["size"] == 64);
        report["skills"] = skills_audit(&app, &cwd);
        report["skillsPanel"] = skills_panel(&app, sid);
        report["tray"] = json!(app.tray_by_id("main").is_some());
        // Start at login, on and off again (macOS / Linux: files under BOBIX_HOME; Windows: the runner's own registry).
        if std::env::var_os("BOBIX_HOME").is_some() {
            let on = crate::update_settings(&b, json!({ "autostart": true }))["autostart"] == true && crate::autostart::get();
            let off = crate::update_settings(&b, json!({ "autostart": false }))["autostart"] == false && !crate::autostart::get();
            report["autostart"] = json!(on && off);
        }
        report["git"] = b.core.lock().unwrap().sessions.get(sid).and_then(|s| s.git.as_ref()).map(|g| json!({ "branch": g["branch"], "worktrees": g["worktrees"] })).unwrap_or(json!("none"));
        report["secondStart"] = json!(crate::server::show_running(b.port, &b.token, &app.package_info().version.to_string()));
        report["work"] = serde_json::from_str::<Value>(&b.snapshot()).map(|v| v["work"]["status"].clone()).unwrap_or_default();
        // Linux: drag a pet with the real mouse (xdotool) and check it moved, then point at a pet and check the line of
        // pets stops: the input shape lets the mouse in over a pet. Not part of the verdict where xdotool is missing.
        if cfg!(target_os = "linux") && crate::procs::exec("xdotool", &["version"], 5, &[]).ok {
            let rest = b.core.lock().unwrap().sessions.keys().find(|k| k.starts_with("bobix-test-")).cloned().unwrap_or_default();
            report["drag"] = drag(&app, &rest);
            report["hover"] = hover(&app);
        }
        if std::env::var("BOBIX_SMOKE_THEME").is_ok_and(|t| t == "dark") {
            if let Some(w) = app.get_webview_window(panel::LABEL) { let _ = w.set_theme(Some(tauri::Theme::Dark)); }
        }
        if let Ok(spec) = std::env::var("BOBIX_SMOKE_REPOS") { report["repos"] = repos_step(&app, &spec); }
        if let Ok(key) = std::env::var("BOBIX_SMOKE_TICKET") { report["ticket"] = ticket(&app, &b, &dir, &key); }
        if let Ok(spec) = std::env::var("BOBIX_SMOKE_AGAIN") { report["again"] = again_step(&app, &spec); }
        if let Ok(spec) = std::env::var("BOBIX_SMOKE_CLONES") { report["clones"] = clones_step(&app, &spec); }
        if let Ok(spec) = std::env::var("BOBIX_SMOKE_START") { report["start"] = start_step(&app, b.port, &dir, &spec); }
        if let Ok(sdir) = std::env::var("BOBIX_SMOKE_READS") { report["reads"] = reads_step(&app, &sdir); }
        if let Ok(spec) = std::env::var("BOBIX_SMOKE_NAME") { report["name"] = name_step(&app, b.port, &dir, &spec); }
        if let Ok(spec) = std::env::var("BOBIX_SMOKE_FOLLOW") { report["follow"] = follow_step(&app, &b, &dir, &spec); }
        report["open"] = open_step(&app, &b);
        report["pageErrors"] = json!(b.core.lock().unwrap().page_errors); // both pages
        let pass = report["answer"] == "ok" && report["hooks"]["complete"] == true && report["visible"] == true && report["onTop"] == true && report["onScreen"] == true && report["pets"].as_u64().unwrap_or(0) >= 2
            && report["pageErrors"].as_array().is_some_and(|a| a.is_empty()) && report["realSession"].is_object() && report["realSession"]["usage"]["turns"] == 1
            && report["panel"]["size"] == 64 && report["panel"]["tabs"] == 6 && report["panel"]["selected"] == "settings" && report["panel"]["feedbackLines"] == 6
            && ["preview", "save", "served", "reset"].iter().all(|k| report["panel"]["picture"][k] == true) && report["panel"]["room"] == true && report["panel"]["habitSkills"] == true
            && (report.get("drag").is_none() || (report["drag"]["moved"].as_f64().unwrap_or(0.0) > 200.0 && report["drag"]["held"] == false))
            && (report.get("hover").is_none() || (report["hover"]["hover"] == true && report["hover"]["stopped"] == true))
            && report["skills"]["uses"] == 1 && report["skills"]["removed"] == true && report["skills"]["undone"] == true && report["skills"]["rules"] == true
            && ["audit", "disable", "undoDisable", "remove", "undoRemove", "lines", "rulesAdd", "rulesRemove"].iter().all(|k| report["skillsPanel"][k] == true)
            && report["open"]["pass"] == true
            && (report.get("ticket").is_none() || report["ticket"]["pass"] == true)
            && (report.get("clones").is_none() || report["clones"]["pass"] == true)
            && (report.get("repos").is_none() || report["repos"]["pass"] == true)
            && (report.get("again").is_none() || report["again"]["pass"] == true)
            && report["settingsSaved"] == true && report["tray"] == true
            && report["badge"]["held"].as_u64().unwrap_or(0) >= 1 && report["badge"]["answered"].as_u64() < report["badge"]["held"].as_u64() && report["autostart"] == true && report["git"].is_object() && report["secondStart"] == true;
        report["pass"] = json!(pass);
        write(&report);
        log(&format!("done {report}"));
        let _ = fs::write(dir.join("done"), ""); // CI waits for this, then stops the app itself
        app.exit(if pass { 0 } else { 1 });
    });
}

// BOBIX_SMOKE_TICKET=<project>#<number> (with BOBIX_HOME set; not in CI): the ticket flow end to end on an issue the
// person's glab (a stand-in, in the e2e run; read with the panel's Refresh, smoke mode reads nothing by itself) lists for a project a session here works in. Clicks the Work screen's own
// buttons: Prepare start, then Copy terminal command; writes <dir>/ticket-start.json ({ dir, cmd }) for whoever runs
// that session (a real Claude session); waits for its plan and clicks Approve plan; watches for the pet's mark of a
// new assumption (Work not on screen meanwhile); waits for done or blocked; clicks Fill gaps with Haiku, then Clean up.
// BOBIX_SMOKE_CLONES=<found>|<link>|<wrong dir>|<right dir>|<clone>|<parent> (with BOBIX_HOME set; not in CI): projects
// not on this computer (#163), against a stand-in glab that lists their issues. <found> is cloned under BOBIX_HOME's
// ~/code beforehand and must be found by itself after Refresh; <link> is linked through the panel's bridge, first with
// a folder of another project (refused), then with its own; <clone> is cloned into <parent> (the stand-in clones). The
// app asks for these folders with a dialog; here the bridge gets them. BOBIX_SMOKE_THEME=dark: the panel window dark.
// BOBIX_SMOKE_HOLD=<ms>: hold on Work, the issue and Settings → Work for screenshots of the real window.
// BOBIX_SMOKE_REPOS=<pre-ticked, comma-separated>|<old repo>|<repo with no folder>|<its folder>|<repo found by its
// remote>|<repo to untick> (with BOBIX_HOME set; not in CI; stand-in gh and glab signed in, the settings from before
// the repo picker): #165. GitLab and GitHub are on; the repos active in the last 90 days are ticked, not <old repo>;
// one list of their issues, newest first, 20 a page, the next page older; Mine and Unassigned filter it; the picker
// lists the repos, searches them and unticks one; Start on <repo with no folder> asks where its code is, and the
// folder (reached through an ~/.ssh/config alias) makes it startable; <repo found by its remote> is found by itself.
fn repos_step(app: &AppHandle, spec: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let v: Vec<&str> = spec.split('|').collect();
    let [pre, old, away, dir, found, untick] = v.as_slice() else { return json!("bad BOBIX_SMOKE_REPOS") };
    let hold = std::env::var("BOBIX_SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let j = |s: &str| json!(s);
    let mut r = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)), r = {{}}; \
        const until = async (f, ms) => {{ for (let t = 0; t < ms; t += 200) {{ if (f()) return true; await w(200); }} return false; }}; \
        const ticked = () => Object.values(state.settings.workRepos || {{}}).flat().sort(), upd = () => st.todo.map(x => x[0].updated || '').sort(); \
        const first = () => {{ const it = workItems().find(x => x.grp === 'fstart'); if (it) bsel.work = it.id; render(); }}; \
        $('#toasts').replaceChildren(); r.on = state.settings.github === true && state.settings.gitlab === true; await bridge.refreshWork(); \
        r.read = await until(() => work.status === 'ok' && (work.hosts || []).length >= 2 && st.todo.length > 0 && ticked().length >= {n}, 40000); await bridge.refreshWork(); await w(1500); r.preTicked = ticked().join(','); r.skipped = (work.failed || []).map(f => f.host + ': ' + f.detail); r.oldOut = !ticked().includes({old}); \
        r.found = await until(() => state.settings.workDirs?.[{found}]?.how === 'found', 30000); \
        r.page1 = st.todo.length; r.more = work.more === true; const oldest = upd()[0]; showTab('work'); first(); await w({hold}); \
        await bridge.workPage(2); r.page2 = await until(() => work.page === 2 && st.todo.length > 0 && upd().at(-1) <= oldest, 20000); first(); await w({hold}); \
        await bridge.workPage(1); await until(() => work.page === 1 && st.todo.length === r.page1, 20000); \
        await save({{ workFilter: 'mine' }}); r.mine = await until(() => work.filter === 'mine' && st.todo.length > 0 && st.todo.every(x => x[0].mine), 20000); first(); await w({hold}); \
        await save({{ workFilter: 'unassigned' }}); r.unassigned = await until(() => st.todo.length > 0 && st.todo.every(x => x[0].assignees?.length === 0), 20000); \
        await save({{ workFilter: 'all' }}); await until(() => work.filter === 'all' && st.todo.length === r.page1, 20000); \
        bsel.work = 'repos'; render(); r.picker = await until(() => document.querySelectorAll('#b-dyn [data-rp-tick]').length >= 5, 20000); await w(300); await w({hold}); \
        r.draftTicks = document.querySelectorAll('#b-dyn [data-rp-tick]:checked').length; await until(() => document.querySelector('#b-dyn [data-rp-q]'), 5000); \
        const q = document.querySelector('#b-dyn [data-rp-q]'); q.value = 'api'; q.dispatchEvent(new Event('input', {{ bubbles: true }})); \
        r.search = [...document.querySelectorAll('#b-dyn label.rp')].filter(l => !l.hidden).map(l => l.querySelector('input').value).join(','); \
        q.value = ''; q.dispatchEvent(new Event('input', {{ bubbles: true }})); \
        [...document.querySelectorAll('#b-dyn [data-rp-tick]')].find(x => x.value === {untick})?.click(); await w(300); document.querySelector('#b-dyn [data-rp-save]')?.click(); \
        r.unticked = await until(() => !ticked().includes({untick}) && st.todo.length > 0 && !st.todo.some(x => x[1].path === {untick}), 20000); \
        const a = st.todo.find(x => x[1].path === {away}); if (a) {{ bsel.work = 'n:' + {away} + '#' + a[0].iid; render(); }} await w(300); \
        r.where = !!document.querySelector('#b-dyn [data-wk-link]') && !document.querySelector('#b-dyn [data-tk-prep]'); await w({hold}); \
        const ok = await bridge.linkFolder({away}, {dir}); r.linked = !!ok.ok; r.startable = await until(() => !!document.querySelector('#b-dyn [data-tk-prep]'), 15000); \
        const it = items.find(x => x.id.startsWith('n:') && /assigned|giao/.test(x.sub)); if (it) {{ bsel.work = it.id; render(); }} await w(300); r.who = !!it; await w({hold}); \
        showTab('settings'); bsel.settings = 'set:work'; render(); await w(300); await w({hold}); showTab('work'); return r; }})()",
        old = j(old), away = j(away), dir = j(dir), found = j(found), untick = j(untick), n = pre.split(',').count()), 180 + 8 * hold / 1000);
    let want: Vec<&str> = { let mut x: Vec<&str> = pre.split(',').collect(); x.sort(); x };
    r["preTickedOk"] = json!(r["preTicked"] == want.join(","));
    r["ticksShown"] = json!(r["draftTicks"].as_u64() == Some(want.len() as u64));
    r["searchOk"] = json!(r["search"].as_str().is_some_and(|s| !s.is_empty() && s.split(',').all(|x| x.contains("api"))));
    r["pass"] = json!(["on", "read", "preTickedOk", "oldOut", "found", "more", "page2", "mine", "unassigned", "picker", "ticksShown", "searchOk", "unticked", "where", "linked", "startable", "who"].iter().all(|k| r[*k] == true) && r["page1"] == 20);
    r
}

// BOBIX_SMOKE_AGAIN=<flow key>|<another repository> (after BOBIX_SMOKE_TICKET; not in CI): one issue, a second
// repository. "Start in another repo…" on the flow opens the issue's Start box; with the other repository picked it
// starts a second flow (its own worktree and key), and each flow's detail lists the other.
fn again_step(app: &AppHandle, spec: &str) -> Value {
    let Some((first, other)) = spec.split_once('|') else { return json!("bad BOBIX_SMOKE_AGAIN") };
    let hold = std::env::var("BOBIX_SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let mut r = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)), r = {{}}, k = {first}, ik = k.split('>')[0], other = {other}; \
        const until = async (f, ms) => {{ for (let t = 0; t < ms; t += 200) {{ if (f()) return true; await w(200); }} return false; }}; const q = s => document.querySelector('#b-dyn ' + s); \
        showTab('work'); bsel.work = 'f:' + k; render(); await w(300); r.againButton = !!q('[data-tk-again]'); q('[data-tk-again]')?.click(); await w(300); \
        r.box = bsel.work === 'n:' + ik && !!q('[data-tk-code]'); const sel = q('[data-tk-code]'); r.suggested = sel?.value || null; \
        sel.value = other; sel.dispatchEvent(new Event('change', {{ bubbles: true }})); await w(300); q('[data-tk-prep]')?.click(); \
        r.prepared = await until(() => !!q('.tk-prompt'), 20000); await w({hold}); q('[data-tk-cli]')?.click(); \
        const [p, n] = tkSplit(ik), mine = () => tickets.list.filter(f => f.path === p && f.iid === n); \
        r.second = await until(() => mine().length >= 2, 20000); r.keys = mine().map(f => f.key); r.dirs = new Set(mine().map(f => f.dir)).size; \
        await w(300); r.listed = bsel.work.startsWith('f:') && !!q('[data-sel-flow]'); await w({hold}); \
        bsel.work = 'f:' + k; render(); await w(300); r.onIssue = q('.tk-flows')?.querySelectorAll('li').length >= 1; await w({hold}); \
        showTab('work'); return r; }})()", first = json!(first), other = json!(other)), 60 + 3 * hold / 1000);
    r["pass"] = json!(["againButton", "box", "prepared", "second", "listed", "onIssue"].iter().all(|k| r[*k] == true) && r["dirs"] == 2);
    r
}

fn clones_step(app: &AppHandle, spec: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let v: Vec<&str> = spec.split('|').collect();
    let [found, link, wrong, right, clone, parent] = v.as_slice() else { return json!("bad BOBIX_SMOKE_CLONES") };
    let hold = std::env::var("BOBIX_SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let j = |s: &str| json!(s);
    let mut r = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)), r = {{}}; \
        const until = async (f, ms) => {{ for (let t = 0; t < ms; t += 200) {{ if (f()) return true; await w(200); }} return false; }}; \
        const local = p => (work.local || []).some(g => g.path === p), away = p => (work.away || []).some(g => g.path === p); \
        await bridge.refreshWork(); r.read = await until(() => work.status === 'ok', 30000); \
        r.found = await until(() => local({found}) && state.settings.workDirs?.[{found}]?.how === 'found', 30000); \
        r.awayBefore = away({link}) && away({clone}); showTab('work'); render(); await w(300); \
        const a = items.find(x => x.id.startsWith('a:' + {link} + '#')); if (a) {{ bsel.work = a.id; render(); }} \
        r.awayButtons = !!document.querySelector('#b-dyn [data-wk-link]') && !!document.querySelector('#b-dyn [data-wk-clone]'); await w({hold}); \
        const no = await bridge.linkFolder({link}, {wrong}); r.wrong = no.reason === 'other' && no.found === {found}; \
        const yes = await bridge.linkFolder({link}, {right}); r.linked = !!yes.ok; \
        const c = await bridge.cloneProject({clone}, {parent}); r.cloned = !!c.ok; r.cloneDir = c.dir || c.detail || null; \
        r.after = await until(() => local({link}) && local({clone}) && !away({link}) && !away({clone}), 15000); \
        r.how = Object.fromEntries(Object.entries(state.settings.workDirs || {{}}).map(([p, e]) => [p, e.how])); \
        const it = items.find(x => x.id.startsWith('n:' + {found} + '#')); if (it) {{ bsel.work = it.id; render(); }} await w(300); \
        r.via = document.querySelector('#b-dyn .tk-dir .badge')?.textContent === T('set.workDirs.found'); await w({hold}); \
        showTab('settings'); bsel.settings = 'set:work'; render(); await w(400); \
        r.rows = document.querySelectorAll('[data-wk-forget]').length; document.querySelector('[data-row=workDirs]')?.scrollIntoView({{ block: 'center' }}); await w({hold}); \
        return r; }})()", found = j(found), link = j(link), wrong = j(wrong), right = j(right), clone = j(clone), parent = j(parent)), 120 + 3 * hold / 1000);
    r["pass"] = json!(["read", "found", "awayBefore", "awayButtons", "wrong", "linked", "cloned", "after"].iter().all(|k| r[*k] == true) && r["rows"].as_u64() >= Some(3));
    r
}

// BOBIX_SMOKE_START=<project>|<issue A>|<issue B>|<another clone>|<clone parent>|<2nd clone parent>|<project fixed in it>|<stand-in state dir>
// (with BOBIX_HOME set; not in CI): #178, starting only in Work, against a stand-in glab whose `repo clone` is slow or
// fails while `clone-slow` / `clone-fail` is in its state dir. Sessions lists no issue to start but keeps the sessions on
// none; Full on A starts a flow (Copy terminal command), and its "Start in another project…" box refuses a second one
// with the way to it. Change folder… (the bridge given the folder the dialog would return) and Clone… (the parent it
// would) replace the remembered folder and prepare the box again; the flow keeps its worktree, the old folder stays, a
// project fixed in it gets the new folder too. Clone… is busy while the panel stays responsive, refuses a folder that is
// there and leaves nothing when it fails. A deleted folder makes Start ask where the code is. Quick on B opens the Start
// box in the folder and asks for a new Claude session there (smoke mode notes the link; it never opens Claude); the
// session that starts there (through the installed hook.js) shows in Work under Quick sessions, Sessions' View in Work
// leads to it, and Quick on B then offers Resume, not a second session.
fn start_step(app: &AppHandle, port: u16, dir: &std::path::Path, spec: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let v: Vec<&str> = spec.split('|').collect();
    let [p, a, bb, other, parent, parent2, cross, sdir] = v.as_slice() else { return json!("bad BOBIX_SMOKE_START") };
    let hold = std::env::var("BOBIX_SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let flag = |k: &str, on: bool| { let f = std::path::Path::new(sdir).join(k); if on { let _ = fs::write(f, "1"); } else { let _ = fs::remove_file(f); } };
    let vars = format!("const p = {}, A = p + '#' + {}, B = p + '#' + {}, other = {}, parent = {}, parent2 = {}, cross = {}, hold = {hold};",
        json!(p), json!(a), json!(bb), json!(other), json!(parent), json!(parent2), json!(cross));
    let pre = format!("{vars} const w = ms => new Promise(o => setTimeout(o, ms)), until = async (f, ms) => {{ for (let t = 0; t < ms; t += 200) {{ if (f()) return true; await w(200); }} return false; }}, q = s => document.querySelector('#b-dyn ' + s), r = {{}}; \
        const box = async (k, how) => {{ bsel.work = 'n:' + k; tkHow[k] = how; render(); await w(400); }}, toastHas = t => [...document.querySelectorAll('#toasts *')].some(x => x.textContent.includes(t));");
    let js = |body: &str| format!("(async () => {{ {pre} {body} return r; }})()");
    let merge = |r: &mut Value, x: Value| if let Some(o) = x.as_object() { for (k, v) in o { r[k] = v.clone(); } } else { r["error"] = x; };
    let mut r = ask_for(app, panel::LABEL, &js("$('#toasts').replaceChildren(); await bridge.refreshWork(); \
        r.listed = await until(() => [A, B].every(k => st.todo.some(x => x[1].path + '#' + x[0].iid === k)) && !!codeDir(p) && !reading(), 40000); \
        showTab('sessions'); await w(400); r.noTodo = !items.some(x => x.grp === 'todo' || x.id === 'next' || x.id.startsWith('i:') || x.id === 'todo:status') && !document.querySelector('#b-list [data-grp=todo], #scopes [data-proj], #scopes [data-wfilter]'); \
        r.looseShown = items.some(x => x.kind === 'session' && ['loose', 'rest'].includes(x.grp)); await w(hold); \
        showTab('work'); await box(A, 'full'); const found = codeDir(p); r.foundShown = q('.tk-dir code')?.title === found && !!q('.tk-dir .badge'); \
        r.how = !!q('[data-tk-how][value=full]:checked') && !!q('[data-tk-how][value=quick]') && !!q('.tk-dir [data-tk-dir]') && !!q('.tk-dir [data-tk-clone]') && q('.tk-dir + .note')?.textContent.includes(p); \
        q('[data-tk-prep]').click(); r.prepared = await until(() => !!q('.tk-prompt'), 20000); navigator.clipboard.writeText = async () => {}; q('[data-tk-cli]').click(); \
        r.flow = await until(() => tickets.list.some(f => f.path === p && f.iid === +A.split('#')[1] && !f.cleaned), 20000); const f = tickets.list.find(f => f.path + '#' + f.iid === A); r.flowDir = f?.dir; r.flowKey = f?.key; \
        bsel.work = 'f:' + f.key; render(); await w(300); q('[data-tk-again]').click(); await w(400); q('[data-tk-prep]')?.click(); \
        r.exists = await until(() => !!q('.note.err [data-sel-flow]') && q('[data-tk-go]')?.disabled === true, 20000); await w(hold); \
        const lf = bridge.linkFolder; bridge.linkFolder = (x, d, of) => lf(x, other, of); q('[data-tk-dir]').click(); \
        r.changed = await until(() => state.settings.workDirs?.[p]?.dir === other && state.settings.workDirs[p].how === 'chosen', 15000); bridge.linkFolder = lf; \
        r.reprepared = await until(() => tkPrep[A]?.prep?.repo === other && q('.tk-dir code')?.title === other && codeDir(p) === other, 20000); \
        r.flowKept = tickets.list.find(x => x.key === f.key)?.dir === f.dir; r.flowsNote = q('.tk-dir + .note')?.textContent.includes(T('tk.folder.flows', { n: 1 })); await w(hold); \
        await save({ workCode: { ...state.settings.workCode, [cross]: p } }); const cx = st.todo.find(x => x[1].path === cross); \
        if (cx) { await box(cross + '#' + cx[0].iid, 'full'); } r.crossUses = !!cx && q('.tk-dir code')?.title === other; \
        await box(A, 'full'); const cl = bridge.cloneProject; bridge.cloneProject = (x, d, of) => cl(x, parent, of); $('#toasts').replaceChildren(); q('[data-tk-clone]').click(); await w(300); \
        r.cloneBusy = q('[data-tk-clone]')?.disabled === true && q('[data-tk-clone]')?.textContent === T('work.clone.busy'); \
        const t0 = performance.now(); showTab('sessions'); showTab('work'); r.responsiveMs = Math.round(performance.now() - t0); r.responsive = r.responsiveMs < 500 && wkBusy.has(p); await w(hold); \
        r.cloned = await until(() => state.settings.workDirs?.[p]?.how === 'cloned', 30000); r.cloneDir = state.settings.workDirs?.[p]?.dir; \
        r.recloned = await until(() => tkPrep[A]?.prep?.repo === r.cloneDir && q('.tk-dir code')?.title === r.cloneDir, 20000); await w(hold); \
        $('#toasts').replaceChildren(); q('[data-tk-clone]').click(); r.existsRefused = await until(() => toastHas(T('work.clone.exists', { dir: home(r.cloneDir) })), 15000) && state.settings.workDirs[p].dir === r.cloneDir; \
        bridge.cloneProject = cl;"), 120 + 6 * hold / 1000);
    let clone_dir = r["cloneDir"].as_str().unwrap_or("").to_string();
    r["oldFolderKept"] = json!(std::path::Path::new(other).join("README.md").is_file());
    r["flowWorktreeKept"] = json!(r["flowDir"].as_str().is_some_and(|d| std::path::Path::new(d).is_dir()));
    // a clone that fails: an error, nothing remembered, no folder left
    flag("clone-fail", true);
    let x = ask_for(app, panel::LABEL, &js("const cl = bridge.cloneProject; bridge.cloneProject = (x, d, of) => cl(x, parent2, of); $('#toasts').replaceChildren(); await box(A, 'full'); \
        const before = state.settings.workDirs[p].dir; q('[data-tk-clone]').click(); r.failShown = await until(() => toastHas(T('work.clone.fail', { e: '' })) && [...document.querySelectorAll('#toasts *')].some(t => t.textContent.includes('not allowed')), 20000); \
        r.nothingKept = state.settings.workDirs[p].dir === before; bridge.cloneProject = cl; await w(hold);"), 40);
    merge(&mut r, x);
    flag("clone-fail", false);
    r["noPartial"] = json!(!std::path::Path::new(parent2).join(p.rsplit('/').next().unwrap_or(p)).exists());
    // the remembered folder deleted: Start asks where the code is again, the backend's prepare too
    let _ = fs::remove_dir_all(&clone_dir);
    let x = ask_for(app, panel::LABEL, &js("await bridge.refreshWork(); r.goneAsks = await until(() => !codeDir(p), 20000); await box(B, 'full'); r.whereShown = !!q('[data-wk-link]') && !!q('[data-wk-clone]') && !q('[data-tk-prep]'); \
        const pr = await bridge.ticketPrep(p, +B.split('#')[1], p); r.prepWhere = pr?.reason === 'where'; await w(hold); \
        r.relinked = !!(await bridge.linkFolder(p, other)).ok && await until(() => codeDir(p) === other, 15000); \
        await box(B, 'quick'); const go = q('[data-start][data-dir]'); r.quickBox = !!go && go.dataset.dir === other && !q('.tk-prompt') && !q('[data-tk-prep]'); await w(hold); \
        navigator.clipboard.writeText = async () => {}; go.click(); \
        r.dialog = await until(() => $('#st-dialog').open, 15000) && $('#st-dir').title === other && $('#st-text').value.startsWith('#' + B.split('#')[1] + ' '); await w(hold); \
        r.prompt = $('#st-text').value; $('#st-go').click(); r.waiting = await until(() => waiting.some(x => x.dir === other && x.iid === +B.split('#')[1]), 10000); \
        await w(400); bsel.work = 'n:' + B; render(); await w(300); r.waitShown = !!q('[data-cancel]'); await w(hold);"), 60 + 3 * hold / 1000);
    merge(&mut r, x);
    r["opened"] = json!(OPENED.lock().unwrap().iter().any(|u| u.starts_with(&format!("claude://code/new?folder={}&q=", crate::work::enc(other)))));
    // the new session Claude would start in that folder, through the installed hook: it takes the start by its first prompt
    let title = claude_session(port, dir, "9b2e4c1a-0d3f-4e5a-8b6c-7d8e9f0a1b2c", other, r["prompt"].as_str().unwrap_or(""));
    r["hookRun"] = json!(if title.contains(&format!("-{}]", bb)) { "ok" } else { "no name" });
    let x = ask_for(app, panel::LABEL, &js("showTab('work'); r.inWork = await until(() => workItems().some(x => x.id === 'q:' + B && x.grp === 'fquick'), 20000); \
        r.noStartItem = !workItems().some(x => x.id === 'n:' + B); bsel.work = 'q:' + B; render(); await w(400); r.withSession = !!q('.rows [data-sel]'); await w(hold); \
        showTab('sessions'); await w(400); const t = items.find(x => x.id === 't:' + B); if (t) { bsel.sessions = t.id; render(); await w(300); } \
        const v = q('[data-tab-go=work]'); r.viewButton = !!v && v.textContent === T('day.viewWork'); v?.click(); await w(400); \
        r.viewInWork = scope === 'work' && bsel.work === 'q:' + B && cur?.id === 'q:' + B; await w(hold); \
        tkAgain.add(B); await box(B, 'quick'); const go = q('[data-start]'); r.liveResume = !!go && !go.dataset.dir && go.textContent === T('work.resume') && !!q('.note') && document.querySelector('#b-dyn').textContent.includes(T('tk.quick.live')); tkAgain.delete(B); await w(hold);"), 40 + 3 * hold / 1000);
    merge(&mut r, x);
    let keys = ["listed", "noTodo", "looseShown", "foundShown", "how", "prepared", "flow", "exists", "changed", "reprepared", "flowKept", "flowsNote", "crossUses", "cloneBusy", "responsive", "cloned", "recloned", "existsRefused",
        "oldFolderKept", "flowWorktreeKept", "failShown", "nothingKept", "noPartial", "goneAsks", "whereShown", "prepWhere", "relinked", "quickBox", "dialog", "waiting", "waitShown", "opened", "inWork", "noStartItem", "withSession", "viewButton", "viewInWork", "liveResume"];
    r["failedChecks"] = json!(keys.iter().filter(|k| r[**k] != true).collect::<Vec<_>>());
    r["pass"] = json!(keys.iter().all(|k| r[*k] == true) && r["hookRun"] == "ok");
    r
}

// A made-up Claude session, through the installed hook.js: it starts in `cwd` and sends `prompt` first. When Bobix
// answers that prompt with a session title, it is written to the transcript as Claude Code writes /rename's
// ("custom-title"), and the next event shows it. → the title Bobix gave, or "".
fn claude_session(port: u16, dir: &std::path::Path, sid: &str, cwd: &str, prompt: &str) -> String {
    let hook = hooks::hook_target(&paths::home());
    let transcript = dir.join(format!("{sid}.jsonl"));
    let _ = fs::write(&transcript, "");
    let ev = |e: Value| { let mut x = json!({ "session_id": sid, "cwd": cwd, "transcript_path": transcript }); for (k, v) in e.as_object().unwrap() { x[k] = v.clone(); } x.to_string() };
    run_hook(&hook, port, &ev(json!({ "hook_event_name": "SessionStart", "source": "startup" })));
    let out = run_hook_with_pid(&hook, port, &ev(json!({ "hook_event_name": "UserPromptSubmit", "prompt": prompt })));
    let title = serde_json::from_str::<Value>(&out).ok().and_then(|v| v["hookSpecificOutput"]["sessionTitle"].as_str().map(String::from)).unwrap_or_default();
    if !title.is_empty() { let _ = fs::write(&transcript, json!({ "type": "custom-title", "customTitle": title, "sessionId": sid }).to_string() + "\n"); }
    run_hook(&hook, port, &ev(json!({ "hook_event_name": "Stop" })));
    title
}

// BOBIX_SMOKE_NAME=<issue project>|<code project>|<its folder>|<another code project>|<its folder> (with BOBIX_HOME set;
// not in CI): #184. A session started for an issue gets Bobix's name, `<KEY of the issue's project>-<iid> <title>`, when
// the code is in another repository, in every start path: Quick (the code project's folder), Full started in Claude
// (Claude desktop opens the repository's main folder, or a worktree of its own there, not the flow's worktree), and
// Full by the terminal command (the flow's worktree). Two starts waiting in one folder each go to their own session;
// the same issue started in two code repositories too. A session you opened there yourself, one that was open before
// the start, and one after a cancel take no name.
fn name_step(app: &AppHandle, port: u16, dir: &std::path::Path, spec: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let v: Vec<&str> = spec.split('|').collect();
    let [a, b, bdir, c, cdir] = v.as_slice() else { return json!("bad BOBIX_SMOKE_NAME") };
    let hold = std::env::var("BOBIX_SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let pre = format!("const A = {}, B = {}, BD = {}, C = {}, CD = {}, hold = {hold}, w = ms => new Promise(o => setTimeout(o, ms)), \
        until = async (f, ms) => {{ for (let t = 0; t < ms; t += 200) {{ if (f()) return true; await w(200); }} return false; }}, r = {{}}; \
        const issue = n => st.todo.map(x => x[0]).find(i => i.iid === n && i.url.includes('/' + A + '/')); \
        const prompt = n => BOARD.brief(issue(n), 'Made-up description.'); \
        const quick = (n, d) => bridge.startIssue(A, n, prompt(n), d); \
        const full = async (n, code, how) => {{ const p = await bridge.ticketPrep(A, n, code); return bridge.ticketStart(A, n, prompt(n), p.models ? {{ size: 'small' }} : null, how, code, true); }}; \
        const waits = (d, n) => waiting.some(x => x.dir === d && x.iid === n);",
        json!(a), json!(b), json!(bdir), json!(c), json!(cdir));
    let js = |body: &str| format!("(async () => {{ {pre} {body} return r; }})()");
    // a session open in the code folder before any start: it is not new there, so it takes none
    let hook = hooks::hook_target(&paths::home());
    let old = |e: Value| { let mut x = json!({ "session_id": "aa000000-0000-4000-8000-000000000409", "cwd": bdir, "transcript_path": dir.join("old.jsonl") }); for (k, v) in e.as_object().unwrap() { x[k] = v.clone(); } x.to_string() };
    let _ = fs::write(dir.join("old.jsonl"), "");
    run_hook(&hook, port, &old(json!({ "hook_event_name": "SessionStart", "source": "startup" })));
    let mut r = ask_for(app, panel::LABEL, &js("await bridge.refreshWork(); r.listed = await until(() => [401, 402, 403, 404, 405, 406, 407, 408, 409].every(n => issue(n)) && !reading(), 40000); \
        r.linked = !!(await bridge.linkFolder(B, BD, A)).ok && !!(await bridge.linkFolder(C, CD, A)).ok; await save({ workCode: { [A]: B } }); \
        r.key = (work.local || []).concat(work.away || []).find(g => g.path === A)?.key; \
        for (const n of [401, 405, 406, 408, 409]) await quick(n, BD); \
        r.started = [await full(402, B, 'desktop'), await full(403, B, 'desktop'), await full(404, B, 'cli'), await full(407, B, 'desktop'), await full(407, C, 'desktop')].every(x => x?.ok === true); \
        r.waiting = await until(() => waits(BD, 401) && waits(BD, 405) && waits(BD, 406) && waits(BD, 408), 10000); \
        await bridge.cancelNew(BD, 408); r.cancelled = await until(() => !waits(BD, 408) && waits(BD, 401), 10000); \
        r.prompts = Object.fromEntries([401, 402, 403, 404, 405, 406, 407, 408, 409].map(n => [n, prompt(n)])); \
        r.dirs = Object.fromEntries(tickets.list.filter(f => f.path === A).map(f => [f.iid + '>' + (f.code || A), f.dir]));"), 120);
    let (prompts, dirs, key) = (r["prompts"].clone(), r["dirs"].clone(), r["key"].as_str().unwrap_or("?").to_string());
    let p = |n: i64| prompts[n.to_string()].as_str().unwrap_or("").to_string();
    let named = |t: &str, n: i64| t.starts_with(&format!("[{key}-{n}] "));
    let flow = |k: &str| dirs[k].as_str().unwrap_or("").to_string();
    let t = |sid: &str, cwd: &str, prompt: &str| claude_session(port, dir, sid, cwd, prompt);
    // a session you open yourself in the code folder while starts wait there: no name, the starts still wait
    r["mine"] = json!(t("bb000000-0000-4000-8000-000000000001", bdir, "Tidy the README"));
    r["oldTakesNone"] = json!(run_hook_with_pid(&hook, port, &old(json!({ "hook_event_name": "UserPromptSubmit", "prompt": p(409) }))).trim().is_empty());
    // Quick, issue in A, code in B: the session in B's folder
    r["quick401"] = json!(t("bb000000-0000-4000-8000-000000000401", bdir, &p(401)));
    // Full in Claude: Claude desktop opens B's main folder, or a worktree of its own under it
    r["full402"] = json!(t("bb000000-0000-4000-8000-000000000402", bdir, &p(402)));
    let own = format!("{bdir}/.claude/worktrees/brave-otter");
    let _ = fs::create_dir_all(&own);
    r["full403"] = json!(t("bb000000-0000-4000-8000-000000000403", &own, &p(403)));
    // Full by the terminal command: the flow's worktree
    r["full404"] = json!(t("bb000000-0000-4000-8000-000000000404", &flow(&format!("404>{b}")), &p(404)));
    // two starts waiting in B's folder: each session takes the one its prompt names
    r["quick406"] = json!(t("bb000000-0000-4000-8000-000000000406", bdir, &p(406)));
    r["quick405"] = json!(t("bb000000-0000-4000-8000-000000000405", bdir, &p(405)));
    // the same issue started in two code repositories: in C's folder first, then B's
    r["full407c"] = json!(t("bb000000-0000-4000-8000-000000000471", cdir, &p(407)));
    r["full407b"] = json!(t("bb000000-0000-4000-8000-000000000472", bdir, &p(407)));
    // cancelled before its session came: no name
    r["cancelled408"] = json!(t("bb000000-0000-4000-8000-000000000408", bdir, &p(408)));
    r["quick409"] = json!(t("bb000000-0000-4000-8000-000000000409", bdir, &p(409)));
    let s = |k: &str| r[k].as_str().unwrap_or("").to_string();
    r["checks"] = json!({ "mine": s("mine").is_empty(), "oldTakesNone": r["oldTakesNone"] == true, "quick401": named(&s("quick401"), 401), "full402": named(&s("full402"), 402),
        "full403": named(&s("full403"), 403), "full404": named(&s("full404"), 404), "quick405": named(&s("quick405"), 405), "quick406": named(&s("quick406"), 406),
        "full407c": named(&s("full407c"), 407), "full407b": named(&s("full407b"), 407), "cancelled408": s("cancelled408").is_empty(), "quick409": named(&s("quick409"), 409) });
    let x = ask_for(app, panel::LABEL, &js("showTab('sessions'); await w(1500); const t = list.map(s => s.title || ''); r.key = (work.local || []).concat(work.away || []).find(g => g.path === A)?.key; \
        r.shown = [401, 402, 403, 404, 405, 406, 407, 409].every(n => t.some(x => x.startsWith('[' + r.key + '-' + n + '] '))) && !t.some(x => x.includes('-408]')); \
        r.noneLeft = waiting.length === 0; r.sessionTitles = t.filter(x => x.startsWith('[')); await w(hold);"), 30);
    if let Some(o) = x.as_object() { for (k, v) in o { r[k] = v.clone(); } }
    let ok = r["checks"].as_object().is_some_and(|c| c.values().all(|v| v == true)) && ["listed", "linked", "started", "waiting", "cancelled", "shown", "noneLeft"].iter().all(|k| r[*k] == true);
    r["pass"] = json!(ok);
    r
}

// BOBIX_SMOKE_FOLLOW=<issue project>|<its folder>|<another code project>|<its folder>|<phase: 1 or 2> (with BOBIX_HOME
// set; not in CI): #188, a Full start opened in Claude desktop is followed wherever Claude opens its session. Against a
// stand-in glab (issues 501 to 510 of the issue project, `mr create`) and two made-up clones whose `origin` is a local
// bare repository. Sessions are pretend ones through the real hook.js; a "worktree Claude made" is a real git worktree
// under the folder's .claude/worktrees, cut from the flow's worktree as Claude desktop does.
// Phase 1: the link Bobix opens names a folder Claude desktop keeps; a session in the named folder, in a worktree
// Claude made (the flow works there: plan approved from Bobix, work log read, Open MR pushes that branch, Clean up
// leaves that folder) and in the main folder with uncommitted files (the flow keeps its worktree; nothing of the main
// folder reaches the ticket's branch); two starts waiting in one repository; one issue in two repositories; a session
// you open yourself; a start removed before its session; the terminal command; a plan answered in Claude itself; a
// worktree deleted by hand (what is not pushed is still said and pushed). Phase 2 is a second run on the same data
// folder: starts wait again after the restart, a flow whose plan waited is picked up, and the notes read in Vietnamese.
fn follow_step(app: &AppHandle, b: &Arc<Bobix>, dir: &std::path::Path, spec: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let v: Vec<&str> = spec.split('|').collect();
    let [a, adir, bp, bdir, phase] = v.as_slice() else { return json!("bad BOBIX_SMOKE_FOLLOW") };
    let (port, hold) = (b.port, std::env::var("BOBIX_SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0));
    let pre = format!("const A = {}, AD = {}, B = {}, BD = {}, hold = {hold}, w = ms => new Promise(o => setTimeout(o, ms)), \
        until = async (f, ms) => {{ for (let t = 0; t < ms; t += 200) {{ if (f()) return true; await w(200); }} return false; }}, r = {{}}; \
        const issue = n => st.todo.map(x => x[0]).find(i => i.iid === n && i.url.includes('/' + A + '/')); \
        const fl = k => tickets.list.find(f => f.key === k); \
        const show = async k => {{ showTab('work'); bsel.work = 'f:' + k; render(); await w(400); }}; \
        const note = () => document.querySelector('#b-dyn .tk-place')?.textContent.trim() || ''; \
        const full = async (n, code, how) => {{ const p = await bridge.ticketPrep(A, n, code); const pr = BOARD.flowPrompt(issue(n), 'Made-up description.', p.models, p.rules); \
          const x = await bridge.ticketStart(A, n, pr, {{ size: 'small' }}, how, code, true); return {{ ok: x?.ok === true, key: x?.key, dir: x?.dir, prompt: pr }}; }};",
        json!(a), json!(adir), json!(bp), json!(bdir));
    let js = |body: &str| format!("(async () => {{ {pre} {body} return r; }})()");
    let hook = hooks::hook_target(&paths::home());
    let flow = |key: &str| serde_json::from_str::<Value>(&b.snapshot()).ok().and_then(|v| v["tickets"]["list"].as_array()?.iter().find(|f| f["key"] == key).cloned()).unwrap_or(Value::Null);
    let wait = |key: &str, f: &dyn Fn(&Value) -> bool| { let until = Instant::now() + Duration::from_secs(15); loop { let x = flow(key); if f(&x) || Instant::now() > until { return x; } thread::sleep(Duration::from_millis(200)); } };
    let git = |d: &str, args: &[&str]| Command::new("git").arg("-C").arg(d).args(args)
        .envs([("GIT_AUTHOR_NAME", "Demo Dev"), ("GIT_AUTHOR_EMAIL", "demo@example.test"), ("GIT_COMMITTER_NAME", "Demo Dev"), ("GIT_COMMITTER_EMAIL", "demo@example.test")])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let sid = |n: u32| format!("cc000000-0000-4000-8000-{n:012}");
    let ev = |n: u32, cwd: &str, e: Value| { let mut x = json!({ "session_id": sid(n), "cwd": cwd, "transcript_path": dir.join(format!("{}.jsonl", sid(n))) }); for (k, v) in e.as_object().unwrap() { x[k] = v.clone(); } x.to_string() };
    // a session Claude starts: SessionStart, its first prompt, the end of its turn → the name Bobix gave it
    let session = |n: u32, cwd: &str, prompt: &str| claude_session(port, dir, &sid(n), cwd, prompt);
    // a plan that waits for an answer: the hook process stays until Bobix answers it (or is stopped: Claude moved on)
    let ask_plan = |n: u32, cwd: &str, plan: &str| {
        let mut c = Command::new("node").arg(&hook).arg(port.to_string()).env("CLAUDE_PID", std::process::id().to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
        if let Some(mut i) = c.stdin.take() { let _ = i.write_all(ev(n, cwd, json!({ "hook_event_name": "PermissionRequest", "tool_name": "ExitPlanMode", "tool_input": { "plan": plan } })).as_bytes()); }
        Some(c)
    };
    // what the session does in the folder it works in: a commit on that folder's branch, and the work log says done
    let work_done = |d: &str, file: &str| {
        let _ = fs::write(std::path::Path::new(d).join(file), "export const subtract = (a, b) => a - b;\n");
        git(d, &["add", file]);
        git(d, &["commit", "-q", "-m", "Add the fix"]);
        let log = std::path::Path::new(d).join(".bobix-log/work-log.md");
        let t = fs::read_to_string(&log).unwrap_or_default().replace("Status: planning", "Status: done").replace("## Steps\n", "## Steps\n- Read the code\n- Added the fix\n")
            .replace("## Assumptions\n", "## Assumptions\n- Whole numbers only\n").replace("## Checks\n", "## Checks\n- `npm test`\n```\nok 3 tests passed\n```\n- Review: /code-review, no issues\n").replace("## Result\n", "## Result\nDone: the fix is in.\n");
        let _ = fs::write(&log, t);
        git(d, &["rev-parse", "HEAD"])
    };
    let remote = |d: &str, branch: &str| git(d, &["ls-remote", "origin", &format!("refs/heads/{branch}")]).split_whitespace().next().unwrap_or("").to_string();
    let state_file = paths::data_dir().join("follow-phase1.json");
    let s = |v: &Value, k: &str| v[k].as_str().unwrap_or("").to_string();
    let mut r = json!({});
    let mut checks = serde_json::Map::new();
    let mut check = |k: &str, ok: bool| { checks.insert(k.into(), json!(ok)); };

    if *phase == "2" {
        let p1: Value = fs::read_to_string(&state_file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let (k501, k508, k505) = (format!("{a}#501"), format!("{a}#508"), format!("{a}#505"));
        // the start that waited when Bobix closed waits again; the flow whose plan waited is stopped until it speaks
        let x = ask_for(app, panel::LABEL, &js(&format!("await bridge.refreshWork(); r.waits = await until(() => waiting.some(x => x.iid === 508), 20000); r.st508 = fl({})?.status; await w(6000); r.st501 = fl({})?.status;", json!(k508), json!(k501))), 60);
        check("restart: the waiting start waits again", x["waits"] == true && x["st508"] == "starting");
        check("restart: a flow whose session is not heard yet is stopped, not lost", x["st501"] == "stopped");
        let theirs = format!("{adir}/.claude/worktrees/quiet-finch");
        git(adir, &["worktree", "add", "-q", "-b", "claude/quiet-finch", &theirs, &git(&s(&p1["dirs"], "508"), &["rev-parse", "HEAD"])]);
        let t = session(508, &theirs, &s(&p1["prompts"], "508"));
        let f = wait(&k508, &|f| f["place"] == "claude");
        check("restart: its session takes it in a worktree Claude made", t.contains("-508]") && f["status"] == "planning" && f["dir"] == theirs.as_str() && f["branch"] == "claude/quiet-finch");
        // 501's session speaks again: its plan was approved in Claude while Bobix was closed
        let d501 = s(&p1["dirs"], "501");
        run_hook(&hook, port, &ev(501, &d501, json!({ "hook_event_name": "PostToolUse", "tool_name": "ExitPlanMode" })));
        check("restart: the flow is followed again and runs", wait(&k501, &|f| f["status"] == "running")["status"] == "running");
        let head = work_done(&d501, "subtract.js");
        run_hook(&hook, port, &ev(501, &d501, json!({ "hook_event_name": "Stop" })));
        let done = wait(&k501, &|f| f["status"] == "done");
        let x = ask_for(app, panel::LABEL, &js(&format!("const k = {}; await show(k); const m = await bridge.ticketMr(k, '', 'Closes #501'); r.mr = m; r.branch = fl(k)?.branch;", json!(k501))), 60);
        check("named folder: done, and Open MR pushes the ticket's branch", done["status"] == "done" && x["mr"]["ok"] == true && !head.is_empty() && remote(adir, x["branch"].as_str().unwrap_or("-")) == head);
        // the notes in Vietnamese, then English again (held for screenshots of the real window)
        let x = ask_for(app, panel::LABEL, &js(&format!("await save({{ language: 'vi' }}); await w(600); await show({}); r.viClaude = note(); r.viClaudeOk = note().startsWith(I18N.t('vi', 'tk.place.claude.t')); await w(hold); \
            await show({}); r.viMain = note(); r.viMainOk = note().startsWith(I18N.t('vi', 'tk.place.main.t')) && note().includes(I18N.t('vi', 'tk.place.mixed', {{ n: 2 }})); await w(hold); \
            await save({{ language: 'en' }}); await w(600); await show({}); r.enMainOk = note().startsWith(I18N.t('en', 'tk.place.main.t')); await w(hold);", json!(k508), json!(k505), json!(k505))), 60 + 3 * hold / 1000);
        check("the notes read in Vietnamese and in English", x["viClaudeOk"] == true && x["viMainOk"] == true && x["enMainOk"] == true);
        r["vi"] = json!([x["viClaude"], x["viMain"]]);
        let ok = checks.values().all(|v| v == true);
        r["checks"] = Value::Object(checks);
        r["pass"] = json!(ok);
        return r;
    }

    // ---- phase 1 ----
    // a worktree an older Bobix made for 506 in the old place (where a link opens the main folder), kept on its branch
    // with work not committed: Start moves it to the new place, its work with it
    let old506 = format!("{adir}/.claude/worktrees/bobix-506");
    git(adir, &["worktree", "add", "-q", "-b", "506-speed-up-the-search-page", &old506]);
    let _ = fs::write(std::path::Path::new(&old506).join("half-done.txt"), "not committed\n");
    let x = ask_for(app, panel::LABEL, &js("await bridge.refreshWork(); r.listed = await until(() => [501, 502, 503, 504, 505, 506, 507, 508, 509, 510].every(n => issue(n)) && !reading(), 40000); \
        r.linked = !!(await bridge.linkFolder(A, AD)).ok && !!(await bridge.linkFolder(B, BD, A)).ok; r.flows = {}; \
        for (const n of [501, 502, 503, 504, 505, 506, 508, 510]) r.flows[n] = await full(n, A, 'desktop'); \
        r.flows['507'] = await full(507, A, 'desktop'); r.flows['507b'] = await full(507, B, 'desktop'); r.flows['509'] = await full(509, A, 'cli'); \
        r.waiting = await until(() => [501, 502, 503, 504, 505, 506, 507, 508, 510].every(n => waiting.some(x => x.iid === n)), 10000); showTab('work'); await w(hold);"), 180 + hold / 1000);
    let fx = |n: &str, k: &str| x["flows"][n][k].as_str().unwrap_or("").to_string();
    let (dirs, prompts): (Value, Value) = (json!(x["flows"].as_object().map(|o| o.iter().map(|(n, f)| (n.clone(), f["dir"].clone())).collect::<serde_json::Map<_, _>>())), json!(x["flows"].as_object().map(|o| o.iter().map(|(n, f)| (n.clone(), f["prompt"].clone())).collect::<serde_json::Map<_, _>>())));
    let _ = fs::write(&state_file, json!({ "dirs": dirs, "prompts": prompts }).to_string());
    check("issues listed, folders linked, 11 Full starts made", x["listed"] == true && x["linked"] == true && x["waiting"] == true && x["flows"].as_object().is_some_and(|o| o.len() == 11 && o.values().all(|f| f["ok"] == true)));
    // the link Bobix opens: the flow's worktree, in a folder Claude desktop keeps (not under .claude/worktrees)
    let opened: Vec<String> = OPENED.lock().unwrap().clone();
    r["opened"] = json!(opened.iter().map(|u| u.split("&q=").next().unwrap_or("").to_string()).collect::<Vec<_>>());
    check("each start's link names its own worktree, outside .claude/worktrees", ["501", "502", "503", "504", "505", "506", "507", "507b", "508", "510"].iter().all(|n| {
        let d = fx(n, "dir");
        !d.is_empty() && !d.contains(".claude") && d.contains("/.bobix/worktrees/") && opened.iter().any(|u| u.starts_with(&format!("claude://code/new?folder={}&q=", crate::work::enc(&d))))
    }));
    check("the prompt names the worktree and what to do elsewhere", fx("501", "prompt").contains(&format!("`{}`, on branch `501-", fx("501", "dir"))) && fx("501", "prompt").contains("a folder under `.claude/worktrees`), stay in that worktree"));
    check("the worktrees' folder ignores itself in the main folder", git(adir, &["status", "--porcelain"]).is_empty());
    check("a worktree kept in the old place is moved to the new one, its work with it", !std::path::Path::new(&old506).exists() && fx("506", "dir").contains("/.bobix/worktrees/")
        && std::path::Path::new(&fx("506", "dir")).join("half-done.txt").is_file());

    // a session you open yourself in the main folder while the starts wait: it takes no flow and no name
    let mine = session(1, adir, "Tidy the README, please");
    check("a session you open yourself takes no flow", mine.is_empty() && ["501", "502", "503", "504", "505"].iter().all(|n| flow(&fx(n, "key"))["status"] == "starting"));

    // 501: Claude desktop opens the named folder (the flow's worktree)
    let t = session(501, &fx("501", "dir"), &fx("501", "prompt"));
    let f = wait(&fx("501", "key"), &|f| f["status"] == "planning");
    check("named folder: the flow takes its session", t.contains("-501]") && f["session"] == sid(501).as_str() && f["place"] == "" && f["dir"] == fx("501", "dir").as_str());

    // 502: Claude desktop makes a worktree of its own, cut from the flow's worktree
    let theirs = format!("{adir}/.claude/worktrees/brave-otter");
    git(adir, &["worktree", "add", "-q", "-b", "claude/brave-otter", &theirs, &git(&fx("502", "dir"), &["rev-parse", "HEAD"])]);
    let t = session(502, &theirs, &fx("502", "prompt"));
    let k502 = fx("502", "key");
    let f = wait(&k502, &|f| f["place"] == "claude");
    check("Claude's worktree: the flow works there", t.contains("-502]") && f["status"] == "planning" && f["session"] == sid(502).as_str() && f["dir"] == theirs.as_str() && f["branch"] == "claude/brave-otter" && f["own"] == fx("502", "dir").as_str());
    check("Claude's worktree: the work log is there, and git ignores it", std::path::Path::new(&theirs).join(".bobix-log/work-log.md").is_file() && git(&theirs, &["status", "--porcelain"]).is_empty());
    // its plan is approved from Bobix
    let held = ask_plan(502, &theirs, "1. Add subtract next to add\n\n## Assumptions\n- Whole numbers only");
    let asked = wait(&k502, &|f| f["status"] == "approve")["status"] == "approve";
    let x2 = ask_for(app, panel::LABEL, &js(&format!("const k = {}; await show(k); r.note = note(); r.noteOk = note().startsWith(I18N.t('en', 'tk.place.claude.t')) && note().includes('claude/brave-otter'); r.plan = !!document.querySelector('#b-dyn .report'); await w(hold); \
        const ok = document.querySelector('#b-dyn [data-tk-ok]'); r.button = !!ok; ok?.click(); await w(800);", json!(k502))), 30 + hold / 1000);
    let answer = held.and_then(|c| c.wait_with_output().ok()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    check("Claude's worktree: Work says where it runs", x2["noteOk"] == true);
    check("Claude's worktree: the plan is approved from Bobix", asked && x2["button"] == true && x2["plan"] == true && answer.contains("\"behavior\":\"allow\"") && wait(&k502, &|f| f["status"] == "running")["status"] == "running");
    // it works to the end there: the log is read, Open MR pushes that folder's branch, Clean up leaves that folder
    let head = work_done(&theirs, "subtract.js");
    run_hook(&hook, port, &ev(502, &theirs, json!({ "hook_event_name": "Stop" })));
    let done = wait(&k502, &|f| f["status"] == "done");
    let x3 = ask_for(app, panel::LABEL, &js(&format!("const k = {}; await show(k); const d = await bridge.ticketDetail(k); r.steps = d?.parsed?.steps; r.ran = d?.parsed?.ran; await w(hold); \
        r.mr = await bridge.ticketMr(k, '', d?.mrBody || 'Closes #502'); await w(500); r.clean = await bridge.ticketClean(k); await w(500); r.cleaned = fl(k)?.cleaned;", json!(k502))), 60 + hold / 1000);
    check("Claude's worktree: the work log is read", done["status"] == "done" && x3["steps"] == json!(["Read the code", "Added the fix"]) && x3["ran"] == 1);
    check("Claude's worktree: Open MR pushes the branch the session worked on", x3["mr"]["ok"] == true && !head.is_empty() && remote(adir, "claude/brave-otter") == head && remote(adir, "502-*").is_empty());
    check("Claude's worktree: Clean up leaves Claude's folder and removes Bobix's own", x3["clean"]["ok"] == true && x3["clean"]["left"] == theirs.as_str() && x3["cleaned"] == true
        && std::path::Path::new(&theirs).is_dir() && !std::path::Path::new(&fx("502", "dir")).exists());

    // 503: Claude desktop opens the repository's main folder, which has changes of yours that are not committed
    let _ = fs::write(std::path::Path::new(adir).join("notes-of-mine.txt"), "half-written notes\n");
    let _ = fs::write(std::path::Path::new(adir).join("README.md"), "# Web shop\n\nAn edit of mine, not committed.\n");
    let t = session(503, adir, &fx("503", "prompt"));
    let k503 = fx("503", "key");
    let f = wait(&k503, &|f| f["place"] == "main");
    check("main folder: the flow takes the session and keeps its worktree", t.contains("-503]") && f["status"] == "planning" && f["session"] == sid(503).as_str() && f["dir"] == fx("503", "dir").as_str() && f["own"] == "" && f["mixed"] == 2);
    // the session does what the prompt says: it works in the flow's worktree. Its events still come from the main folder
    let log503 = std::path::Path::new(&fx("503", "dir")).join(".bobix-log/work-log.md");
    let _ = fs::write(&log503, fs::read_to_string(&log503).unwrap_or_default().replace("Status: planning", "Status: done"));
    run_hook(&hook, port, &ev(503, adir, json!({ "hook_event_name": "Stop" })));
    let none = wait(&k503, &|f| f["status"] == "done")["status"] == "done";
    let x4 = ask_for(app, panel::LABEL, &js(&format!("const k = {}; await show(k); r.note = note(); r.noteOk = note().startsWith(I18N.t('en', 'tk.place.main.t')) && note().includes('2 changed files'); await w(hold); r.empty = await bridge.ticketMr(k, '', 'Closes #503');", json!(k503))), 40 + hold / 1000);
    check("main folder: Work says so, and how many files of yours were not committed", x4["noteOk"] == true);
    check("main folder: a branch with nothing committed is not pushed", none && x4["empty"]["error"] == "nocommits" && remote(adir, &s(&f, "branch")).is_empty());
    let head = work_done(&fx("503", "dir"), "subtract.js");
    run_hook(&hook, port, &ev(503, adir, json!({ "hook_event_name": "Stop" })));
    let x5 = ask_for(app, panel::LABEL, &js(&format!("const k = {}; r.mr = await bridge.ticketMr(k, '', 'Closes #503');", json!(k503))), 40);
    let pushed = git(adir, &["ls-tree", "-r", "--name-only", &head]);
    check("main folder: Open MR pushes the ticket's branch", x5["mr"]["ok"] == true && !head.is_empty() && remote(adir, &s(&f, "branch")) == head);
    check("main folder: your uncommitted files stay yours (not lost, not on the ticket's branch)", !pushed.contains("notes-of-mine.txt") && pushed.contains("subtract.js")
        && crate::tickets::uncommitted(adir) == 2 && fs::read_to_string(std::path::Path::new(adir).join("README.md")).is_ok_and(|t| t.contains("An edit of mine")));

    // 504 and 505: two starts waiting in one repository; 505's session comes first (main folder), then 504's (Claude's worktree)
    let t5 = session(505, adir, &fx("505", "prompt"));
    // 504's: Claude cuts its worktree from the main folder's HEAD, where a commit of yours is not pushed yet
    let _ = fs::write(std::path::Path::new(adir).join("local-notes.txt"), "mine\n");
    git(adir, &["add", "local-notes.txt"]);
    git(adir, &["commit", "-q", "-m", "My own work, not pushed"]);
    let theirs4 = format!("{adir}/.claude/worktrees/calm-heron");
    git(adir, &["worktree", "add", "-q", "-b", "claude/calm-heron", &theirs4]);
    let t4 = session(504, &theirs4, &fx("504", "prompt"));
    let (f4, f5) = (wait(&fx("504", "key"), &|f| f["place"] == "claude"), wait(&fx("505", "key"), &|f| f["place"] == "main"));
    let x45 = ask_for(app, panel::LABEL, &js(&format!("await show({}); r.note = note(); r.ok = note().includes(I18N.t('en', 'tk.place.carried.1', {{ ...fw(fl({})), n: 1, b: 'claude/calm-heron' }})); await w(hold);", json!(fx("504", "key")), json!(fx("504", "key")))), 20 + hold / 1000);
    check("Claude's worktree cut from the main folder: your unpushed commit is counted and said", f4["carried"] == 1 && x45["ok"] == true);
    check("two starts in one repository: each session takes its own flow", t4.contains("-504]") && t5.contains("-505]") && f4["session"] == sid(504).as_str() && f5["session"] == sid(505).as_str() && f4["dir"] == theirs4.as_str() && f5["dir"] == fx("505", "dir").as_str());
    // 505's plan is answered in Claude itself (the hook is stopped: Claude moved on), not in Bobix
    if let Some(mut c) = ask_plan(505, adir, "1. Do it") {
        let asked = wait(&fx("505", "key"), &|f| f["status"] == "approve")["status"] == "approve";
        let _ = c.kill();
        let _ = c.wait();
        run_hook(&hook, port, &ev(505, adir, json!({ "hook_event_name": "PostToolUse", "tool_name": "ExitPlanMode" })));
        check("a plan approved in Claude itself: the flow runs", asked && wait(&fx("505", "key"), &|f| f["status"] == "running")["status"] == "running");
    }

    // 506: the start is removed before its session comes; a session with its prompt then takes nothing
    let k506 = fx("506", "key");
    let x6 = ask_for(app, panel::LABEL, &js(&format!("const k = {}; r.check = await bridge.ticketRemove(k, 'check'); r.rm = await bridge.ticketRemove(k, 'keep'); await w(500); r.gone = !fl(k) && !waiting.some(x => x.iid === 506);", json!(k506))), 30);
    let t6 = session(506, adir, &fx("506", "prompt"));
    check("removed before its session: nothing takes it", x6["rm"]["ok"] == true && x6["gone"] == true && t6.is_empty() && flow(&k506).is_null());

    // 507: one issue started in two repositories: the session in B's main folder, then the one in A's worktree
    let tb = session(5072, bdir, &fx("507b", "prompt"));
    let ta = session(5071, &fx("507", "dir"), &fx("507", "prompt"));
    let (fa, fb) = (wait(&fx("507", "key"), &|f| f["status"] == "planning"), wait(&fx("507b", "key"), &|f| f["place"] == "main"));
    let x7 = ask_for(app, panel::LABEL, &js("showTab('sessions'); await w(800); r.under = ((model.issues || []).find(i => i.project === A && i.iid === 507)?.sessions || []).map(x => x.id + ' ' + x.why);"), 20);
    check("one issue in two repositories: each session takes the flow where it came up", ta.contains("-507]") && tb.contains("-507]") && fa["session"] == sid(5071).as_str() && fb["session"] == sid(5072).as_str() && fx("507", "key") != fx("507b", "key"));
    // the session in the other repository is listed under the issue it works on, not under "without a ticket"
    check("one issue in two repositories: both sessions are listed under the issue", [5071, 5072].iter().all(|n| x7["under"].as_array().is_some_and(|a| a.iter().any(|x| x == &json!(format!("{} started", sid(*n)))))));

    // 509: the terminal command (a session in the flow's worktree); then its worktree is deleted by hand
    let k509 = fx("509", "key");
    let t9 = session(509, &fx("509", "dir"), &fx("509", "prompt"));
    let f9 = wait(&k509, &|f| f["status"] == "planning");
    let head = work_done(&fx("509", "dir"), "subtract.js");
    run_hook(&hook, port, &ev(509, &fx("509", "dir"), json!({ "hook_event_name": "Stop" })));
    check("terminal command: the flow takes its session and reads its log", t9.contains("-509]") && f9["session"] == sid(509).as_str() && f9["place"] == "" && wait(&k509, &|f| f["status"] == "done")["status"] == "done");
    let _ = fs::remove_dir_all(fx("509", "dir"));
    let x9 = ask_for(app, panel::LABEL, &js(&format!("const k = {}; r.first = await bridge.ticketClean(k); r.mr = await bridge.ticketMr(k, '', 'Closes #509'); r.second = await bridge.ticketClean(k); await w(500); r.cleaned = fl(k)?.cleaned;", json!(k509))), 60);
    check("worktree deleted by hand: Clean up still says what is not pushed", x9["first"]["error"] == "unpushed" && x9["first"]["n"] == 1);
    check("worktree deleted by hand: Open MR still pushes the branch, then Clean up", x9["mr"]["ok"] == true && remote(adir, &s(&f9, "branch")) == head && x9["second"]["ok"] == true && x9["cleaned"] == true);

    // 510: its worktree is deleted by hand while the start waits; Remove says so, and the issue starts again
    let k510 = fx("510", "key");
    let _ = fs::remove_dir_all(fx("510", "dir"));
    let x10 = ask_for(app, panel::LABEL, &js(&format!("const k = {}; r.check = await bridge.ticketRemove(k, 'check'); r.rm = await bridge.ticketRemove(k, 'keep'); await w(500); r.again = await full(510, A, 'cli'); await w(500); r.st = fl(k)?.status;", json!(k510))), 60);
    check("worktree deleted by hand while waiting: said, removed, and started again", x10["check"]["exists"] == false && x10["rm"]["ok"] == true && x10["again"]["ok"] == true && x10["st"] == "starting" && std::path::Path::new(&fx("510", "dir")).join(".bobix-log/work-log.md").is_file());

    // 510 again: its session starts in the flow's worktree and takes it there (named by the worktree's branch), with a
    // prompt you edited (it no longer names the issue). A later session elsewhere whose prompt names it takes nothing:
    // no name, and the flow keeps its session and its folder.
    let (k510, d510) = (k510.clone(), x10["again"]["dir"].as_str().unwrap_or("").to_string());
    let t1 = session(5101, &d510, "Round the totals, please");
    let theirs10 = format!("{adir}/.claude/worktrees/late-wren");
    git(adir, &["worktree", "add", "-q", "-b", "claude/late-wren", &theirs10]);
    let t2 = session(5102, &theirs10, x10["again"]["prompt"].as_str().unwrap_or(""));
    thread::sleep(Duration::from_millis(1500));
    let f10 = flow(&k510);
    check("a second session cannot move a flow that has its session", t1.contains("-510] ") && t2.is_empty() && f10["session"] == sid(5101).as_str() && f10["dir"] == d510.as_str() && f10["place"] == "");
    // while a start waits in this repository, a new session on an issue's branch is still named by that branch
    let named = format!("{adir}/.claude/worktrees/504-show-stock");
    git(adir, &["worktree", "add", "-q", "-b", "504-show-stock", &named]);
    let t3 = session(5041, &named, "Look at the stock badge");
    check("while a start waits, a session on an issue's branch is still named", t3.contains("-504] "));

    // 501's plan waits when Bobix closes (phase 2 goes on from here); 508 still waits for its session
    let _left_waiting = ask_plan(501, &fx("501", "dir"), "1. Add subtract\n\n## Assumptions\n- Whole numbers only");
    check("before the restart: a plan waits, a start waits", wait(&fx("501", "key"), &|f| f["status"] == "approve")["status"] == "approve" && flow(&fx("508", "key"))["status"] == "starting");
    let _ = ask_for(app, panel::LABEL, &js("showTab('work'); await w(hold);"), 10 + hold / 1000);
    let ok = checks.values().all(|v| v == true);
    r["checks"] = Value::Object(checks);
    r["pass"] = json!(ok);
    r
}

fn run_hook_with_pid(hook: &PathBuf, port: u16, input: &str) -> String {
    let Ok(mut child) = Command::new("node").arg(hook).arg(port.to_string()).env("CLAUDE_PID", std::process::id().to_string())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return String::new() };
    if let Some(mut i) = child.stdin.take() { let _ = i.write_all(input.as_bytes()); }
    child.wait_with_output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

// BOBIX_SMOKE_READS=<stand-in state dir> (with BOBIX_HOME set; not in CI): #178, reading the issues again, against stand-in
// glab and gh that read slowly (3 s), find nothing, hang or are down while `slow`, `none`, `gl-hang` or `gh-down` is in
// that dir. The filter shows "Reading your issues…" in place of the list at once (within 100 ms) and looks busy; a second
// change before the first read ends wins (the first answer is never shown); the page arrows, Refresh, the repo picker's
// Show issues and its Read again say so too;
// one host down shows the others' issues and names it; one hanging does too, in about 10 s (its projects are read at
// once, 10 s each); one hanging and one down stop reading and name both; none says so.
fn reads_step(app: &AppHandle, sdir: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let hold = std::env::var("BOBIX_SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let flag = |k: &str, on: bool| { let f = std::path::Path::new(sdir).join(k); if on { let _ = fs::write(f, "1"); } else { let _ = fs::remove_file(f); } };
    let pre = format!("const hold = {hold}, w = ms => new Promise(o => setTimeout(o, ms)), until = async (f, ms) => {{ for (let t = 0; t < ms; t += 50) {{ if (f()) return true; await w(50); }} return false; }}, r = {{}}; \
        const loadingNow = () => !!document.querySelector('#b-list [data-bsel=\"tk:loading\"]') && !document.querySelector('#b-list [data-bsel^=\"n:\"]'); \
        const status = () => items.find(x => x.id === 'todo:status')?.sub || '';");
    let js = |body: &str| format!("(async () => {{ {pre} {body} return r; }})()");
    let merge = |r: &mut Value, x: Value| if let Some(o) = x.as_object() { for (k, v) in o { r[k] = v.clone(); } } else { r["error"] = x; };
    let mut r = ask_for(app, panel::LABEL, &js("showTab('work'); await bridge.refreshWork(); r.ready = await until(() => !reading() && work.status === 'ok' && st.todo.length > 0, 40000); await save({ workFilter: 'all' }); await until(() => !reading(), 20000);"), 60);
    flag("slow", true);
    let x = ask_for(app, panel::LABEL, &js("const sel = document.querySelector('#scopes [data-wfilter]'); const seen = []; const tick = setInterval(() => seen.push({ reading: reading(), filter: work.filter, mine: st.todo.filter(x => x[0].mine).length, assigned: st.todo.filter(x => x[0].assignees?.length).length, n: st.todo.length }), 50); \
        let t0 = performance.now(); sel.value = 'mine'; sel.dispatchEvent(new Event('change', { bubbles: true })); r.filterMs = Math.round(performance.now() - t0); r.filterLoading = loadingNow() && r.filterMs < 100; \
        r.busy = document.querySelector('#scopes [data-wfilter]')?.getAttribute('aria-busy') === 'true' && document.querySelector('#scopes .pager')?.getAttribute('aria-busy') === 'true'; await w(Math.max(hold, 300)); \
        t0 = performance.now(); showTab('sessions'); showTab('work'); r.responsive = performance.now() - t0 < 500; \
        await w(700); const s2 = document.querySelector('#scopes [data-wfilter]'); s2.value = 'unassigned'; s2.dispatchEvent(new Event('change', { bubbles: true })); \
        r.ended = await until(() => !reading(), 20000); clearInterval(tick); await w(200); \
        r.newestWins = work.filter === 'unassigned' && st.todo.length > 0 && st.todo.every(x => !x[0].assignees?.length); \
        r.neverOld = seen.filter(x => !x.reading).every(x => x.filter === 'all' ? true : x.assigned === 0) && !seen.some(x => !x.reading && x.filter !== 'all' && x.n > 0 && x.mine === x.n); \
        r.samples = seen.length; r.listBack = !!document.querySelector('#b-list [data-bsel^=\"n:\"]'); await w(hold); \
        t0 = performance.now(); document.querySelector('#b-list [data-act=refresh]')?.click(); r.refreshLoading = loadingNow() && performance.now() - t0 < 100; await until(() => !reading(), 20000); \
        await askIssues(() => save({ workFilter: 'all' })); r.allBack = await until(() => !reading() && work.filter === 'all' && work.more, 20000); \
        const nx = document.querySelector('#scopes .pager [data-wpage]:last-child'); t0 = performance.now(); nx?.click(); r.pageLoading = !!nx && loadingNow() && performance.now() - t0 < 100; \
        r.pageEnds = await until(() => !reading() && work.page === 2, 20000); await w(hold); document.querySelector('#scopes .pager [data-wpage]')?.click(); await until(() => !reading() && work.page === 1, 20000); \
        bsel.work = 'repos'; render(); await until(() => rp.list && !rp.busy && !!document.querySelector('#b-dyn [data-rp-load=fresh]'), 20000); document.querySelector('#b-dyn [data-rp-load=fresh]').click(); await w(50); \
        r.readAgainBusy = rp.busy && document.querySelector('#b-dyn [aria-busy=true] .rp-h') !== null && document.querySelector('#b-dyn').textContent.includes(T('work.repos.loading')); await until(() => !rp.busy, 20000); await w(300); \
        const off = [...document.querySelectorAll('#b-dyn [data-rp-tick]:checked')].find(x => x.value !== 'acme/web-shop'); off?.click(); await w(100); \
        t0 = performance.now(); document.querySelector('#b-dyn [data-rp-save]').click(); r.showIssuesLoading = !!off && loadingNow() && performance.now() - t0 < 100; \
        r.showIssuesEnds = await until(() => !reading(), 20000); const back = [...document.querySelectorAll('#b-dyn [data-rp-tick]')].find(x => x.value === off?.value); \
        if (back && !back.checked) { back.click(); await w(100); document.querySelector('#b-dyn [data-rp-save]')?.click(); await until(() => !reading(), 20000); }"), 150);
    merge(&mut r, x);
    flag("slow", false);
    flag("gh-down", true);
    let x = ask_for(app, panel::LABEL, &js("await askIssues(() => bridge.refreshWork()); await until(() => !reading(), 20000); await w(300); \
        r.oneDown = work.status === 'ok' && st.todo.length > 0 && st.todo.every(x => x[0].url.includes('gitlab.example.com')) && status().includes('github.com'); bsel.work = 'todo:status'; render(); await w(Math.max(hold, 200));"), 40);
    merge(&mut r, x);
    flag("gh-down", false); flag("gl-hang", true); // GitLab never answers, GitHub does: its issues show in about 10 s
    let x = ask_for(app, panel::LABEL, &js("const t0 = performance.now(); await askIssues(() => bridge.refreshWork()); await until(() => !reading(), 40000); r.oneHangMs = Math.round(performance.now() - t0); await w(300); \
        r.oneHang = r.oneHangMs < 13000 && st.todo.length > 0 && st.todo.every(x => x[0].url.includes('github.com')) && status().includes('gitlab.example.com'); bsel.work = 'todo:status'; render(); await w(Math.max(hold, 200));"), 60);
    merge(&mut r, x);
    flag("gh-down", true); // and GitHub down too: reading ends, both named
    let x = ask_for(app, panel::LABEL, &js("const t0 = performance.now(); await askIssues(() => bridge.refreshWork()); r.allDownEnds = await until(() => !reading(), 40000); r.allDownMs = Math.round(performance.now() - t0); await w(300); \
        r.allDown = r.allDownEnds && status().includes('github.com') && status().includes('gitlab.example.com') && !document.querySelector('#b-list [data-bsel=\"tk:loading\"]'); bsel.work = 'todo:status'; render(); await w(Math.max(hold, 200));"), 60);
    merge(&mut r, x);
    flag("gh-down", false); flag("gl-hang", false); flag("none", true);
    let x = ask_for(app, panel::LABEL, &js("await askIssues(() => bridge.refreshWork()); await until(() => !reading(), 20000); await w(300); \
        r.noneSaid = st.todo.length === 0 && status() === noneText() && status() !== T('work.loading'); bsel.work = 'todo:status'; render(); await w(Math.max(hold, 200));"), 40);
    merge(&mut r, x);
    flag("none", false);
    let _ = ask_for(app, panel::LABEL, "(async () => { await bridge.refreshWork(); return true; })()", 10);
    let keys = ["ready", "filterLoading", "busy", "responsive", "ended", "newestWins", "neverOld", "listBack", "refreshLoading", "allBack", "pageLoading", "pageEnds", "readAgainBusy", "showIssuesLoading", "showIssuesEnds", "oneDown", "oneHang", "allDownEnds", "allDown", "noneSaid"];
    r["failedChecks"] = json!(keys.iter().filter(|k| r[**k] != true).collect::<Vec<_>>());
    r["pass"] = json!(keys.iter().all(|k| r[*k] == true));
    r
}

fn ticket_secs() -> u64 { if std::env::var_os("BOBIX_SMOKE_TICKET").is_some() { std::env::var("BOBIX_SMOKE_TICKET_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(900) } else { 0 } }
fn ticket(app: &AppHandle, b: &Arc<Bobix>, dir: &std::path::Path, key: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let mut r = json!({});
    let flow = || serde_json::from_str::<Value>(&b.snapshot()).ok().and_then(|v| v["tickets"]["list"].as_array()?.iter().find(|f| f["key"] == key).cloned()).unwrap_or(Value::Null);
    let wait = |secs: u64, f: &dyn Fn(&Value) -> bool| { let until = Instant::now() + Duration::from_secs(secs); loop { let x = flow(); if f(&x) || Instant::now() > until { return x; } thread::sleep(Duration::from_millis(500)); } };
    let k = json!(key);
    // BOBIX_SMOKE_HOLD=<ms>: stay that long on the screens a person would look at (for screenshots of the real window)
    let hold = std::env::var("BOBIX_SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    // <key> may name the code's repository after ">" ("demo/tracker#3>demo/service"): picked in the Start box first
    r["start"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)), k = {k}, ik = k.split('>')[0], code = k.split('>')[1]; \
        const until = async f => {{ for (let t = 0; t < 30000; t += 100) {{ if (f()) return true; await w(100); }} return false; }}; \
        navigator.clipboard.writeText = async () => {{}}; const go = bridge.ticketStart; bridge.ticketStart = async (...a) => (window.__tkStart = await go(...a)); await bridge.refreshWork(); showTab('work'); \
        if (!await until(() => items.some(x => x.id === 'n:' + ik))) return {{ error: 'issue not listed', items: items.map(x => x.id), work: work.status, local: (work.local || []).map(g => g.path + ' ' + g.issues.length), todo: st.todo.length, problems: work.problems }}; \
        bsel.work = 'n:' + ik; render(); await w(300); if (code) {{ const sel = document.querySelector('#b-dyn [data-tk-code]'); sel.value = code; sel.dispatchEvent(new Event('change', {{ bubbles: true }})); await w(300); await w({hold}); }} \
        document.querySelector('#b-dyn [data-tk-prep]').click(); \
        if (!await until(() => document.querySelector('#b-dyn .tk-prompt'))) return {{ error: 'no Start box', code: tkPrep[ik] }}; await w({hold}); \
        const prompt = tkPrep[ik].prompt, disk = document.querySelector('#b-dyn .note, #b-dyn .risk') && tkPrep[ik].prep.room, claudeWarn = !!document.querySelector('#b-dyn [data-tk-claude]'), claudeDir = tkPrep[ik].prep.repo; \
        let land = null; if (claudeWarn) {{ document.querySelector('#b-dyn [data-tk-claude]').click(); await until(() => scope === 'skills' && !aud.busy && aud.dir === claudeDir); await w(300); \
          land = {{ scope, item: bsel.skills, dir: aud.dir === claudeDir, picker: document.querySelector('#scopes [data-skdir]')?.value === claudeDir, add: !!document.querySelector('#b-bar [data-sk=testsAdd]') }}; await w({hold}); \
          showTab('work'); bsel.work = 'n:' + ik; render(); await w(300); }} \
        document.querySelector('#b-dyn [data-tk-cli]').click(); \
        if (!await until(() => tickets.list.some(f => f.key === k))) return {{ error: 'not started', keys: tickets.list.map(f => f.key) }}; \
        return {{ prompt: prompt.length, rules: prompt.includes('EnterPlanMode'), room: disk, selected: bsel.work, claudeWarn, claudeDir, land, code: tkCode(ik) }}; }})()"), 40 + 2 * hold / 1000);
    let f = flow();
    let fdir = f["dir"].as_str().unwrap_or("").to_string();
    r["worktree"] = json!(std::path::Path::new(&fdir).join(".bobix-log/work-log.md").is_file());
    // the line to paste, with the prompt (the panel copied it; here it is written for the e2e driver)
    let p = ask(app, panel::LABEL, "window.__tkStart && window.__tkStart.cmd");
    let _ = fs::write(dir.join("ticket-start.json"), json!({ "dir": fdir, "cmd": p, "key": key, "start": r["start"] }).to_string());
    if f.is_null() { r["pass"] = json!(false); return r; } // Start did not start (no disk, no folder…): nothing to wait for
    let f = wait(ticket_secs() / 2, &|f| f["status"] == "approve");
    r["plan"] = json!(f["plan"].as_str().map_or(0, |p| p.len()));
    r["planHasAssumptions"] = json!(f["plan"].as_str().is_some_and(|p| p.to_lowercase().contains("assum")));
    r["approve"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const k = {k}, w = ms => new Promise(o => setTimeout(o, ms)); showTab('work'); bsel.work = 'f:' + k; render(); await w(300); \
        const b = document.querySelector('#b-dyn [data-tk-ok]'); if (!b) return {{ error: 'no Approve button' }}; b.click(); await w(800); showTab('overview'); return true; }})()"), 10);
    // running: a new assumption puts the mark on the pet (the overlay draws it) while Work is not on screen
    let (until, mut marked, mut drawn) = (Instant::now() + Duration::from_secs(ticket_secs() / 2), false, Value::Null);
    let f = loop {
        let f = flow();
        let sid = f["session"].as_str().unwrap_or("").to_string();
        if !marked && serde_json::from_str::<Value>(&b.snapshot()).ok().is_some_and(|v| v["sessions"].as_array().is_some_and(|a| a.iter().any(|s| s["id"] == sid.as_str() && s["mark"] == true))) {
            marked = true;
            thread::sleep(Duration::from_millis(600));
            drawn = ask(app, overlay::LABEL, "document.querySelectorAll('.pet .mk').length");
        }
        if matches!(f["status"].as_str(), Some("done" | "blocked")) || Instant::now() > until { break f; }
        thread::sleep(Duration::from_millis(500));
    };
    r["status"] = f["status"].clone();
    r["parsed"] = f["parsed"].clone();
    r["marked"] = json!(marked);
    r["markDrawn"] = drawn;
    r["session"] = f["session"].clone();
    // Checks on the done flow (✓ n checks, or the nudge), then Review (its terminal line): a second session in the
    // worktree, run by the e2e driver from <dir>/ticket-review.json, notes what it found under ## Checks
    r["checksBadge"] = ask(app, panel::LABEL, &format!("(async () => {{ showTab('work'); bsel.work = 'f:' + {k}; render(); await new Promise(o => setTimeout(o, 300)); \
        return [...document.querySelectorAll('#b-dyn .b-sec h3 .badge')].map(b => b.textContent).join(' | ') || null; }})()"));
    r["review"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const go = bridge.ticketReview; bridge.ticketReview = async (...a) => (window.__tkReview = await go(...a)); \
        showTab('work'); bsel.work = 'f:' + {k}; render(); await new Promise(o => setTimeout(o, 300 + {hold})); document.querySelector('#b-dyn [data-tk-review-cli]').click(); \
        for (let t = 0; t < 10000 && !window.__tkReview; t += 100) await new Promise(o => setTimeout(o, 100)); return !!window.__tkReview?.cmd; }})()"), 15 + hold / 1000);
    let rc = ask(app, panel::LABEL, "window.__tkReview && window.__tkReview.cmd");
    let _ = fs::write(dir.join("ticket-review.json"), json!({ "dir": fdir, "cmd": rc, "key": key }).to_string());
    let f = wait(ticket_secs() / 3, &|f| f["log"].as_str().is_some_and(|l| l.contains("Review (second session)")));
    r["reviewNoted"] = json!(f["log"].as_str().is_some_and(|l| l.contains("Review (second session)")));
    r["checks"] = f["parsed"]["checks"].clone();
    // Open MR: Clean up first says the commits are not pushed (and does nothing); then Open MR… shows what is sent,
    // and Push and open MR pushes the branch and runs glab mr create (a stand-in in the e2e run); the flow keeps the link
    r["mr"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const k = {k}, w = ms => new Promise(o => setTimeout(o, ms)), q = s => document.querySelector('#b-dyn ' + s); \
        const until = async (f, ms) => {{ for (let t = 0; t < ms; t += 200) {{ if (f()) return true; await w(200); }} return false; }}; \
        const seen = s => {{ const e = q(s)?.getBoundingClientRect(), p = $('#b-detail').getBoundingClientRect(); return !!e && e.top >= p.top - 1 && e.top < p.bottom; }}; \
        showTab('work'); bsel.work = 'f:' + k; render(); await w(300); const r = {{}}; \
        q('[data-tk-clean]').click(); r.unpushed = await until(() => tkUnpushed[k] > 0, 10000) ? tkUnpushed[k] : 0; await w(300); r.warnSeen = seen('.tk-unpushed'); await w({hold}); \
        $('#b-detail').scrollTop = 0; q('[data-tk-mr-open]').click(); await until(() => q('[data-tk-mr-body]'), 5000); await w(300); r.formSeen = seen('.tk-mr'); await w({hold}); const body = q('[data-tk-mr-body]').value; \
        r.bodyHasPlan = body.includes('## Plan'); r.bodyHasChecks = body.includes('## Checks'); r.closes = /^Closes (\\S+)?#\\d+\\n/.test(body); r.closesLine = body.split('\\n')[0]; \
        q('[data-tk-mr-go]').click(); r.opened = await until(() => tickets.list.find(f => f.key === k)?.mr, 90000); \
        r.url = tickets.list.find(f => f.key === k)?.mr || null; r.link = !!document.querySelector('#b-dyn [data-url*=\"merge_requests\"], #b-dyn [data-url*=\"/pull/\"]'); \
        r.row = [...document.querySelectorAll('#b-list .b-it[aria-selected=\"true\"] .badge')].map(b => b.textContent).join(' | '); return r; }})()"), 120 + 2 * hold / 1000);
    // CI: Check CI reads the MR's pipeline (red at first in the e2e run): the flow shows it with Fix CI and the Overview
    // lists "CI red"; once the run turns it green (it waits for <dir>/ticket-ci.json), Check CI shows it passed
    r["ci"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const k = {k}, w = ms => new Promise(o => setTimeout(o, ms)), q = s => document.querySelector('#b-dyn ' + s), f = () => tickets.list.find(x => x.key === k); \
        showTab('work'); bsel.work = 'f:' + k; render(); await w(300); q('[data-tk-ci]').click(); for (let t = 0; t < 20000 && !f()?.ci?.status; t += 200) await w(200); await w(300); \
        const r = {{ status: f()?.ci?.status || null, fix: !!q('[data-copy*=\"CI failed on \"]'), badge: q('h3 .badge.err')?.textContent || null, stuck: stuckList().some(x => x.kind === 'cired' && x.key === k) }}; \
        q('.tk-warn')?.scrollIntoView({{ block: 'center' }}); await w({hold}); showTab('overview'); bsel.overview = 'stuck:cired:' + k; render(); await w({hold}); return r; }})()"), 30 + 2 * hold / 1000);
    let _ = fs::write(dir.join("ticket-ci.json"), json!({ "key": key, "status": r["ci"]["status"] }).to_string());
    r["ciGreen"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const k = {k}, w = ms => new Promise(o => setTimeout(o, ms)), f = () => tickets.list.find(x => x.key === k); \
        for (let t = 0; t < 60000 && f()?.ci?.status !== 'passed'; t += 2500) {{ await bridge.ticketCi(k); await w(2500); }} return f()?.ci?.status || null; }})()"), 70);
    // CLAUDE.md: the Start box said this project has none, and its "Add a Tests line" landed on the project's CLAUDE.md
    // item (above); the line is added there (after the confirm that shows it), then taken out again
    r["claudeMd"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)), q = s => document.querySelector('#b-dyn ' + s); \
        const until = async (g, ms) => {{ for (let t = 0; t < ms; t += 100) {{ if (g()) return true; await w(100); }} return false; }}; \
        showTab('skills'); await loadAudit(); await w(300); \
        const r = {{ on: bsel.skills, before: aud.claude?.test ?? null, suggest: aud.testCmd }}; q('[data-sk=testsAdd]').click(); \
        await until(() => $('#sk-dialog').open, 5000); r.lines = $('#sk-dialog .b-code').textContent.includes('Tests: `npm test`'); await w({hold}); $('#sk-dialog [data-sk-yes]').click(); \
        r.added = await until(() => aud.claude?.test === true && !!aud.claude.undo, 8000); await w(200 + {hold}); \
        q('[data-sk=testsRm]').click(); r.removed = await until(() => aud.claude && aud.claude.test === false, 8000); return r; }})()"), 30 + 2 * hold / 1000);
    // the log's own view, then Haiku's summary and the cleanup, clicked in Work
    r["finish"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const k = {k}, w = ms => new Promise(o => setTimeout(o, ms)), q = s => document.querySelector('#b-dyn ' + s); \
        const until = async (f, ms) => {{ for (let t = 0; t < ms; t += 200) {{ if (f()) return true; await w(200); }} return false; }}; \
        showTab('work'); bsel.work = 'f:' + k; render(); await w(500); const r = {{ assumptionsShown: document.querySelectorAll('#b-dyn .tk-as li').length }}; \
        q('[data-tk-sum]')?.click(); r.summary = await until(() => tickets.list.find(f => f.key === k)?.summary, 150000); \
        q('[data-tk-clean]')?.click(); r.cleaned = await until(() => tickets.list.find(f => f.key === k)?.cleaned, 20000); \
        r.toast = [...document.querySelectorAll('.toast')].map(t => t.textContent).join(' | '); return r; }})()"), 200);
    let after = flow();
    r["kept"] = json!(after["log"].as_str().is_some_and(|l| !l.is_empty()));
    r["worktreeGone"] = json!(!std::path::Path::new(&fdir).exists());
    r["pass"] = json!(r["start"]["rules"] == true && r["worktree"] == true && r["plan"].as_u64().unwrap_or(0) > 0 && r["approve"] == true
        && r["status"] == "done" && r["parsed"]["steps"].as_array().is_some_and(|a| !a.is_empty()) && r["finish"]["summary"] == true
        && r["parsed"]["ran"].as_u64().unwrap_or(0) > 0 && r["review"] == true && r["reviewNoted"] == true
        && r["mr"]["unpushed"].as_u64().unwrap_or(0) > 0 && r["mr"]["warnSeen"] == true && r["mr"]["formSeen"] == true && r["mr"]["opened"] == true
        && r["mr"]["row"].as_str().is_some_and(|x| x.contains('✓')) && r["mr"]["bodyHasChecks"] == true
        && r["ci"]["status"] == "failed" && r["ci"]["fix"] == true && r["ci"]["stuck"] == true && r["ciGreen"] == "passed"
        && r["start"]["claudeWarn"] == true && r["start"]["land"]["scope"] == "skills" && r["start"]["land"]["item"] == "aud:claude"
        && r["start"]["land"]["dir"] == true && r["start"]["land"]["picker"] == true && r["start"]["land"]["add"] == true
        && r["claudeMd"]["on"] == "aud:claude" && r["claudeMd"]["added"] == true && r["claudeMd"]["removed"] == true);
    r
}

// The skills audit through the panel's bridge, on a scratch skill under BOBIX_HOME (never the person's own): listed
// with its use, removed, undone; and the CLAUDE.md rule block of the session's folder, read only.
fn skills_audit(app: &AppHandle, cwd: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let c = paths::home().join(".claude");
    let _ = fs::create_dir_all(c.join("skills/smoke-skill"));
    let _ = fs::write(c.join("skills/smoke-skill/SKILL.md"), "---\nname: smoke-skill\ndescription: A skill for the smoke run\n---\n");
    let _ = fs::create_dir_all(c.join("projects/smoke"));
    let line = json!({ "type": "assistant", "timestamp": hooks::iso(crate::skill_audit::now_ms()), "message": { "content": [{ "type": "tool_use", "name": "Skill", "input": { "skill": "smoke-skill" } }] } });
    let _ = fs::write(c.join("projects/smoke/s.jsonl"), line.to_string() + "\n");
    ask(app, panel::LABEL, &format!("(async () => {{ const r = {{}}, row = a => a.skills.find(s => s.name === 'smoke-skill'); \
        const a = await bobix.skillsAudit(null), s = row(a); r.uses = s?.uses; \
        const rm = await bobix.changeSkill(null, s.key, 'remove'); r.removed = rm.ok && !row(await bobix.skillsAudit(null)); \
        r.undone = (await bobix.undoSkill(rm.id)).ok && !!row(await bobix.skillsAudit(null)); \
        const rules = await bobix.routingRules({cwd}); r.rules = typeof rules.text === 'string' && rules.text.startsWith('## When to split'); \
        return r; }})()", cwd = json!(cwd)))
}

// The same changes as the person's clicks in the panel's Skills screen, on the scratch skill and the session's folder:
// Disable then Undo (the toast), Remove (after its confirm) then Undo, the CLAUDE.md block added (after the confirm
// that shows its lines) then removed. Each Undo puts the files back as they were (skill_audit.rs tests that bytewise).
fn skills_panel(app: &AppHandle, sid: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    ask_for(app, panel::LABEL, &format!("(async () => {{ const r = {{}}, w = ms => new Promise(o => setTimeout(o, ms)); \
        const until = async f => {{ for (let t = 0; t < 8000; t += 100) {{ if (f()) return true; await w(100); }} return false; }}; \
        const bar = q => document.querySelector('#b-bar ' + q), yes = async () => {{ await until(() => $('#sk-dialog').open); $('#sk-dialog [data-sk-yes]').click(); }}; \
        const undo = t => [...document.querySelectorAll('.toast')].find(x => x.textContent.includes(t))?.querySelector('button').click(); \
        showTab('skills'); audDir = list.find(s => s.id === {sid:?})?.cwd; await loadAudit(); \
        r.audit = await until(() => aud.a && items.some(x => x.label === 'smoke-skill')); \
        const id = items.find(x => x.label === 'smoke-skill')?.id, back = () => !skBackups.length && items.some(x => x.id === id); \
        bsel.skills = id; render(); bar('[data-sk=disable]').click(); \
        r.disable = await until(() => skBackups[0]?.kind === 'disable' && !items.some(x => x.id === id)); undo('smoke-skill'); r.undoDisable = await until(back); \
        bsel.skills = id; render(); bar('[data-sk=remove]').click(); await yes(); \
        r.remove = await until(() => skBackups[0]?.kind === 'remove' && !items.some(x => x.id === id)); undo('smoke-skill'); r.undoRemove = await until(back); \
        bsel.skills = 'aud:rules'; render(); bar('[data-sk=rulesAdd]').click(); await until(() => $('#sk-dialog').open); \
        r.lines = $('#sk-dialog .b-code').textContent.includes('When to split, which model'); await yes(); \
        r.rulesAdd = await until(() => aud.rules?.present && !!aud.rules.undo); \
        bsel.skills = 'aud:rules'; render(); bar('[data-sk=rulesRm]').click(); await yes(); \
        r.rulesRemove = await until(() => aud.rules && !aud.rules.present && !skBackups.length); \
        r.err = aud.err?.text || aud.fail || null; return r; }})()"), 40)
}

// The value of a page expression (awaited), through the bridge like the pets' own calls.
fn ask(app: &AppHandle, label: &str, expr: &str) -> Value { ask_for(app, label, expr, 5) }

fn ask_for(app: &AppHandle, label: &str, expr: &str, secs: u64) -> Value {
    let Some(w) = app.get_webview_window(label) else { return json!("no window") };
    *PAGE.lock().unwrap() = None;
    let _ = w.eval(format!("(async () => {{ let v; try {{ v = await ({expr}); }} catch (e) {{ v = {{ error: String(e) }}; }} \
        window.__TAURI_INTERNALS__.invoke('smoke_page', {{ data: {{ v }} }}); }})()"));
    let until = Instant::now() + Duration::from_secs(secs);
    while PAGE.lock().unwrap().is_none() && Instant::now() < until { thread::sleep(Duration::from_millis(50)); }
    PAGE.lock().unwrap().take().map(|v| v["v"].clone()).unwrap_or(json!("no answer"))
}

// Drags the pet 300 px toward the middle of the screen (so the zone never clamps it) and lets go. What the drag did is
// where the pet is when the button comes up (read before the pet's own handler, which puts it back into the line, which
// then spreads out evenly again and slides it on): it must have followed the mouse over 200 px, and be back in the line
// (no longer held) after. Screen coordinates come from the window's position (WebKitGTK may not know screenX without a
// window manager); the page counts the pointer presses it gets, so a failure says whether the mouse reached it.
fn drag(app: &AppHandle, sid: &str) -> Value {
    let Some(win) = app.get_webview_window(overlay::LABEL) else { return json!({ "error": "no window" }) };
    let at = ask(app, overlay::LABEL, &format!("(() => {{ window.__ev = {{}}; for (const t of ['pointerdown', 'pointermove', 'pointerup', 'pointercancel', 'lostpointercapture']) addEventListener(t, () => {{ __ev[t] = (__ev[t] || 0) + 1; }}, true); \
        addEventListener('pointerup', () => {{ __ev.dropX = pets.get({sid:?})?.x; }}, true); \
        const p = pets.get({sid:?}); if (!p) return null; const b = p.el.querySelector('.spr').getBoundingClientRect(); \
        return {{ x: b.left + b.width / 2, y: b.top + b.height / 2, petX: Math.round(p.x), w: innerWidth, screenX, screenY }}; }})()"));
    let (Some(cx), Some(cy), Some(px), Some(w)) = (at["x"].as_f64(), at["y"].as_f64(), at["petX"].as_f64(), at["w"].as_f64()) else { return json!({ "error": "no pet", "at": at }) };
    let f = win.scale_factor().unwrap_or(1.0);
    let pos = win.inner_position().unwrap_or_default();
    let (x, y) = ((pos.x as f64 / f + cx).round() as i64, (pos.y as f64 / f + cy).round() as i64);
    let dir = if px > w / 2.0 { -1 } else { 1 };
    let xdo = |a: &[String]| { let a: Vec<&str> = a.iter().map(String::as_str).collect(); crate::procs::exec("xdotool", &a, 5, &[]); };
    xdo(&["mousemove".into(), x.to_string(), y.to_string()]);
    thread::sleep(Duration::from_millis(400));
    xdo(&["mousedown".into(), "1".into()]);
    thread::sleep(Duration::from_millis(100));
    for i in 1..=15 { xdo(&["mousemove".into(), (x + dir * i * 20).to_string(), y.to_string()]); thread::sleep(Duration::from_millis(40)); }
    xdo(&["mouseup".into(), "1".into()]);
    thread::sleep(Duration::from_millis(800));
    let after = ask(app, overlay::LABEL, &format!("({{ x: Math.round(pets.get({sid:?}).x), held: !!LINEUP.held, events: window.__ev }})"));
    let dropped = after["events"]["dropX"].as_f64();
    json!({ "at": [x, y], "page": at, "from": px, "dropped": dropped.map(f64::round), "to": after["x"], "held": after["held"], "events": after["events"],
        "moved": dropped.map(|a| ((a - px) * dir as f64).round()) })
}

// Moves the mouse onto a pet in full sight, once the line has stopped sliding from the drag before (a sliding pet
// would leave the point before the mouse gets there; walking it stays under it). Screen coordinates come from the window's position (WebKitGTK may not know
// screenX without a window manager); the page counts the pointer events it gets, so a failure says whether the mouse
// reached it.
fn hover(app: &AppHandle) -> Value {
    let Some(win) = app.get_webview_window(overlay::LABEL) else { return json!({ "error": "no window" }) };
    let until = Instant::now() + Duration::from_secs(4);
    let settled = loop {
        let sliding = ask(app, overlay::LABEL, "LINEUP.vis.some(e => Math.abs(e.slide || 0) > .5) || !!LINEUP.held");
        if sliding == false { break true; }
        if Instant::now() > until { break false; }
        thread::sleep(Duration::from_millis(100));
    };
    let at = ask(app, overlay::LABEL, "(() => { window.__ev = {}; for (const t of ['pointerover', 'pointermove']) addEventListener(t, () => { __ev[t] = (__ev[t] || 0) + 1; }, true); \
        const p = [...pets.values()].find(q => !q.el.classList.contains('line-off') && (q.edgeFade ?? 1) > .9); if (!p) return null; \
        const b = p.el.querySelector('.spr').getBoundingClientRect(); return { id: p.s.id, x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()");
    let (Some(cx), Some(cy), Some(id)) = (at["x"].as_f64(), at["y"].as_f64(), at["id"].as_str()) else { return json!({ "error": "no pet in sight", "at": at }) };
    let f = win.scale_factor().unwrap_or(1.0);
    let pos = win.inner_position().unwrap_or_default();
    let (x, y) = ((pos.x as f64 / f + cx).round() as i64, (pos.y as f64 / f + cy).round() as i64);
    crate::procs::exec("xdotool", &["mousemove", &x.to_string(), &y.to_string()], 5, &[]);
    thread::sleep(Duration::from_millis(600));
    let a = ask(app, overlay::LABEL, &format!("({{ hover: !!pets.get({id:?})?.hover, x: pets.get({id:?})?.x, events: window.__ev }})"));
    thread::sleep(Duration::from_millis(500));
    let b = ask(app, overlay::LABEL, &format!("pets.get({id:?})?.x"));
    let stopped = a["x"].as_f64().zip(b.as_f64()).is_some_and(|(p, q)| (p - q).abs() < 0.5);
    crate::procs::exec("xdotool", &["mousemove", "0", "0"], 5, &[]);
    json!({ "at": [x, y], "settled": settled, "hover": a["hover"], "stopped": stopped, "events": a["events"] })
}

// The hook's stdout: what Claude Code would read.
fn run_hook_out(hook: &PathBuf, port: u16, input: &str) -> String {
    let Ok(mut child) = Command::new("node").arg(hook).arg(port.to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return String::new() };
    if let Some(mut i) = child.stdin.take() { let _ = i.write_all(input.as_bytes()); }
    child.wait_with_output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

fn run_hook(hook: &PathBuf, port: u16, input: &str) -> String {
    let child = Command::new("node").arg(hook).arg(port.to_string()).env("CLAUDE_PID", std::process::id().to_string())
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn();
    let mut child = match child { Ok(c) => c, Err(e) => return format!("error: {e}") };
    if let Some(mut i) = child.stdin.take() { let _ = i.write_all(input.as_bytes()); }
    match child.wait_with_output() {
        Ok(o) if o.status.success() => "ok".into(),
        Ok(o) => format!("error: {} {}", o.status, String::from_utf8_lossy(&o.stderr).chars().take(200).collect::<String>()),
        Err(e) => format!("error: {e}"),
    }
}

// Opening a session from wherever it shows (#215): a Claude desktop session is opened from the Overview row's button,
// from the command bar (Enter; Shift+Enter shows details instead), from the pet card and from the tray menu. Each must
// reach open_session, which in a smoke run notes the claude:// link it would open; a session that has ended opens as well.
fn open_step(app: &AppHandle, b: &Arc<Bobix>) -> Value {
    let (id, link) = ("smoke-open", "claude://code/continue?session=local_smoke-open");
    let send = |name: &str, extra: Value| {
        let mut e = json!({ "session_id": id, "cwd": "/smoke/open-app", "hook_event_name": name, "bobix_host_session": "local_smoke-open" });
        for (k, v) in extra.as_object().unwrap() { e[k] = v.clone(); }
        b.on_event(e, false);
    };
    send("SessionStart", json!({ "source": "startup" }));
    send("UserPromptSubmit", json!({ "prompt": "Open me from everywhere" }));
    // A new Claude desktop session is held back from the lists until its parent link is known (up to state::LINK_HOLD_MS).
    let held = || b.core.lock().unwrap().sessions.get(id).is_some_and(|s| crate::state::linking(s, crate::state::now()));
    let t = Instant::now();
    while held() && t.elapsed() < Duration::from_secs(20) { thread::sleep(Duration::from_millis(200)); }
    thread::sleep(Duration::from_millis(1500)); // the lists get it
    let hits = || OPENED.lock().unwrap().iter().filter(|u| *u == link).count();
    let mut r = json!({});
    let until = |f: &dyn Fn() -> bool| { let t = Instant::now(); while !f() && t.elapsed() < Duration::from_secs(8) { thread::sleep(Duration::from_millis(100)); } f() };
    // The Overview: the session's row has an Open button; one click opens it.
    r["overview"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)); showTab('overview'); \
        for (let t = 0; t < 8000 && !document.querySelector('#b-list [data-open-id={id:?}]'); t += 100) await w(100); \
        const bt = document.querySelector('#b-list [data-open-id={id:?}]'); const r = {{ button: !!bt, label: bt?.getAttribute('aria-label') || '' }}; \
        if (bt) {{ const ev = new KeyboardEvent('keydown', {{ key: 'Enter', bubbles: true, cancelable: true }}); bt.focus(); bt.dispatchEvent(ev); r.enterKept = !ev.defaultPrevented; }} /* Enter on the button must not run the selected row's action */ \
        bt?.click(); return r; }})()"), 15);
    r["overviewOpened"] = json!(until(&|| hits() == 1));
    // The command bar: Enter opens, Shift+Enter shows the details and opens nothing more.
    r["search"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)); const s = list.find(x => x.id === {id:?}); \
        openCmdk(); $('#cmdk-q').value = s.title || s.name; $('#cmdk-q').dispatchEvent(new Event('input')); await w(200); \
        const r = {{ found: cmdItems.length > 0 && !!document.querySelector('#cmdk-res [data-ci-alt]') }}; \
        $('#cmdk').dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', bubbles: true }})); await w(400); r.closed = !$('#cmdk').open; return r; }})()"), 15);
    r["searchOpened"] = json!(until(&|| hits() == 2));
    r["searchDetails"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)); const s = list.find(x => x.id === {id:?}); \
        showTab('overview'); bsel.sessions = null; openCmdk(); $('#cmdk-q').value = s.title || s.name; $('#cmdk-q').dispatchEvent(new Event('input')); await w(200); \
        $('#cmdk').dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', shiftKey: true, bubbles: true }})); await w(600); \
        return scope === 'sessions' || bsel[scope] === 's:' + {id:?}; }})()"), 15);
    thread::sleep(Duration::from_millis(500));
    r["searchDetailsOpenedNothing"] = json!(hits() == 2);
    // The pet card: hover it, press its Open session button.
    r["card"] = ask_for(app, overlay::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)); \
        for (let t = 0; t < 8000 && !pets.get({id:?}); t += 100) await w(100); const p = pets.get({id:?}); if (!p) return {{ error: 'no pet' }}; \
        let bt; /* point at the pet as the mouse does (another pet under a real pointer would end its hover), and keep doing so until the card is there */ \
        for (let t = 0; t < 8000 && !(bt = p.el.querySelector('.card [data-open]')); t += 100) {{ const b = p.el.querySelector('.spr').getBoundingClientRect(); mouse.x = b.left + b.width / 2; mouse.y = b.top + b.height / 2; mouseMoved = true; p.hoverUntil = Date.now() + 8000; wake(); await w(100); }} \
        const r = {{ button: !!bt, label: bt?.getAttribute('aria-label') || '', why: bt ? '' : [p.el.className, p.hover, p.hoverUntil, Date.now(), mouse.x, mouse.y, document.hidden, p.s.state, !!p.el.querySelector('.card')].join(' | ') }}; bt?.dispatchEvent(new PointerEvent('pointerdown', {{ bubbles: true, button: 0 }})); return r; }})()"), 15);
    r["cardOpened"] = json!(until(&|| hits() == 3));
    // The tray menu item of a session that needs you.
    crate::tray::on_menu(app, &format!("open:{id}"));
    r["trayOpened"] = json!(until(&|| hits() == 4));
    // Ended: a terminal session reopens as the command that resumes it.
    let ended = "smoke-ended-terminal";
    for (n, extra) in [("SessionStart", json!({ "source": "startup" })), ("SessionEnd", json!({}))] {
        let mut e = json!({ "session_id": ended, "cwd": "/smoke/ended-app", "hook_event_name": n });
        for (k, v) in extra.as_object().unwrap() { e[k] = v.clone(); }
        b.on_event(e, false);
    }
    let gone = b.open_session(ended, true, false);
    r["ended"] = json!(gone["ok"] == false && gone["reason"] == "ended" && gone["cmd"] == format!("claude --resume {ended}"));
    send("SessionEnd", json!({}));
    let mut pass = ["overviewOpened", "searchOpened", "searchDetailsOpenedNothing", "cardOpened", "trayOpened", "ended"].iter().all(|k| r[k] == true);
    pass = pass && r["overview"]["button"] == true && r["overview"]["enterKept"] == true && r["search"]["found"] == true && r["search"]["closed"] == true && r["searchDetails"] == true && r["card"]["button"] == true
        && ["overview", "card"].iter().all(|k| r[k]["label"].as_str().is_some_and(|l| l.contains("Open me from everywhere") || l.contains("open-app")));
    r["pass"] = json!(pass);
    r
}
