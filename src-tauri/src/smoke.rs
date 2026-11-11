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

pub fn page_result(v: Value) { *PAGE.lock().unwrap() = Some(v); }

fn log(m: &str) { println!("[bobix] smoke: {m}"); }

pub fn run(app: AppHandle, b: Arc<Bobix>, dir: PathBuf) {
    thread::spawn(move || {
        let _ = fs::create_dir_all(&dir);
        let mut report = json!({ "platform": crate::platform(), "build": "tauri" });
        let write = |r: &Value| { let _ = fs::write(dir.join("report.json"), serde_json::to_string_pretty(r).unwrap()); };
        {
            let (dir, app) = (dir.clone(), app.clone());
            thread::spawn(move || {
                let extra = ticket_secs() + if std::env::var_os("BOBIX_SMOKE_CLONES").is_some() { 180 } else { 0 };
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
        if let Ok(key) = std::env::var("BOBIX_SMOKE_TICKET") { report["ticket"] = ticket(&app, &b, &dir, &key); }
        if let Ok(spec) = std::env::var("BOBIX_SMOKE_CLONES") { report["clones"] = clones_step(&app, &spec); }
        report["pageErrors"] = json!(b.core.lock().unwrap().page_errors); // both pages
        let pass = report["answer"] == "ok" && report["hooks"]["complete"] == true && report["visible"] == true && report["onTop"] == true && report["onScreen"] == true && report["pets"].as_u64().unwrap_or(0) >= 2
            && report["pageErrors"].as_array().is_some_and(|a| a.is_empty()) && report["realSession"].is_object() && report["realSession"]["usage"]["turns"] == 1
            && report["panel"]["size"] == 64 && report["panel"]["tabs"] == 6 && report["panel"]["selected"] == "settings" && report["panel"]["feedbackLines"] == 6
            && ["preview", "save", "served", "reset"].iter().all(|k| report["panel"]["picture"][k] == true) && report["panel"]["room"] == true && report["panel"]["habitSkills"] == true
            && (report.get("drag").is_none() || (report["drag"]["moved"].as_f64().unwrap_or(0.0) > 200.0 && report["drag"]["held"] == false))
            && (report.get("hover").is_none() || (report["hover"]["hover"] == true && report["hover"]["stopped"] == true))
            && report["skills"]["uses"] == 1 && report["skills"]["removed"] == true && report["skills"]["undone"] == true && report["skills"]["rules"] == true
            && ["audit", "disable", "undoDisable", "remove", "undoRemove", "lines", "rulesAdd", "rulesRemove"].iter().all(|k| report["skillsPanel"][k] == true)
            && (report.get("ticket").is_none() || report["ticket"]["pass"] == true)
            && (report.get("clones").is_none() || report["clones"]["pass"] == true)
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
fn clones_step(app: &AppHandle, spec: &str) -> Value {
    if std::env::var_os("BOBIX_HOME").is_none() { return json!("skipped: set BOBIX_HOME to a scratch folder"); }
    let v: Vec<&str> = spec.split('|').collect();
    let [found, link, wrong, right, clone, parent] = v.as_slice() else { return json!("bad BOBIX_SMOKE_CLONES") };
    if std::env::var("BOBIX_SMOKE_THEME").is_ok_and(|t| t == "dark") {
        if let Some(w) = app.get_webview_window(panel::LABEL) { let _ = w.set_theme(Some(tauri::Theme::Dark)); }
    }
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
        showTab('sessions'); const it = items.find(x => x.id.startsWith('i:' + {found} + '#')); if (it) {{ bsel.sessions = it.id; render(); }} await w(300); \
        r.via = !!document.querySelector('#b-dyn .note') && document.querySelector('#b-dyn').textContent.includes('Found'); await w({hold}); \
        showTab('settings'); bsel.settings = 'set:work'; render(); await w(400); \
        r.rows = document.querySelectorAll('[data-wk-forget]').length; document.querySelector('[data-row=workDirs]')?.scrollIntoView({{ block: 'center' }}); await w({hold}); \
        return r; }})()", found = j(found), link = j(link), wrong = j(wrong), right = j(right), clone = j(clone), parent = j(parent)), 120 + 3 * hold / 1000);
    r["pass"] = json!(["read", "found", "awayBefore", "awayButtons", "wrong", "linked", "cloned", "after"].iter().all(|k| r[*k] == true) && r["rows"].as_u64() >= Some(3));
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
    r["start"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)), k = {k}; \
        const until = async f => {{ for (let t = 0; t < 30000; t += 100) {{ if (f()) return true; await w(100); }} return false; }}; \
        navigator.clipboard.writeText = async () => {{}}; const go = bridge.ticketStart; bridge.ticketStart = async (...a) => (window.__tkStart = await go(...a)); await bridge.refreshWork(); showTab('work'); \
        if (!await until(() => items.some(x => x.id === 'n:' + k))) return {{ error: 'issue not listed', items: items.map(x => x.id), work: work.status, local: (work.local || []).map(g => g.path + ' ' + g.issues.length), todo: st.todo.length, problems: work.problems }}; \
        bsel.work = 'n:' + k; render(); document.querySelector('#b-dyn [data-tk-prep]').click(); \
        if (!await until(() => document.querySelector('#b-dyn .tk-prompt'))) return {{ error: 'no Start box' }}; await w({hold}); \
        const prompt = tkPrep[k].prompt, disk = document.querySelector('#b-dyn .note, #b-dyn .risk') && tkPrep[k].prep.room, claudeWarn = !!document.querySelector('#b-dyn [data-tk-claude]'), claudeDir = tkPrep[k].prep.repo; \
        document.querySelector('#b-dyn [data-tk-cli]').click(); \
        if (!await until(() => tickets.list.some(f => f.key === k))) return {{ error: 'not started' }}; \
        return {{ prompt: prompt.length, rules: prompt.includes('EnterPlanMode'), room: disk, selected: bsel.work, claudeWarn, claudeDir }}; }})()"), 40 + hold / 1000);
    let f = flow();
    let fdir = f["dir"].as_str().unwrap_or("").to_string();
    r["worktree"] = json!(std::path::Path::new(&fdir).join(".bobix-log/work-log.md").is_file());
    // the line to paste, with the prompt (the panel copied it; here it is written for the e2e driver)
    let p = ask(app, panel::LABEL, "window.__tkStart && window.__tkStart.cmd");
    let _ = fs::write(dir.join("ticket-start.json"), json!({ "dir": fdir, "cmd": p, "key": key, "start": r["start"] }).to_string());
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
        showTab('work'); bsel.work = 'f:' + k; render(); await w(300); const r = {{}}; \
        q('[data-tk-clean]').click(); r.unpushed = await until(() => tkUnpushed[k] > 0, 10000) ? tkUnpushed[k] : 0; \
        q('[data-tk-mr-open]').click(); await until(() => q('[data-tk-mr-body]'), 5000); q('.tk-mr').scrollIntoView({{ block: 'start' }}); await w({hold}); const body = q('[data-tk-mr-body]').value; \
        r.bodyHasPlan = body.includes('## Plan'); r.bodyHasChecks = body.includes('## Checks'); r.closes = body.startsWith('Closes #'); \
        q('[data-tk-mr-go]').click(); r.opened = await until(() => tickets.list.find(f => f.key === k)?.mr, 90000); \
        r.url = tickets.list.find(f => f.key === k)?.mr || null; r.link = !!document.querySelector('#b-dyn [data-url*=\"merge_requests\"]'); return r; }})()"), 120 + hold / 1000);
    // CI: Check CI reads the MR's pipeline (red at first in the e2e run): the flow shows it with Fix CI and the Overview
    // lists "CI red"; once the run turns it green (it waits for <dir>/ticket-ci.json), Check CI shows it passed
    r["ci"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const k = {k}, w = ms => new Promise(o => setTimeout(o, ms)), q = s => document.querySelector('#b-dyn ' + s), f = () => tickets.list.find(x => x.key === k); \
        showTab('work'); bsel.work = 'f:' + k; render(); await w(300); q('[data-tk-ci]').click(); for (let t = 0; t < 20000 && !f()?.ci?.status; t += 200) await w(200); await w(300); \
        const r = {{ status: f()?.ci?.status || null, fix: !!q('[data-copy*=\"CI failed on merge request\"]'), badge: q('h3 .badge.err')?.textContent || null, stuck: stuckList().some(x => x.kind === 'cired' && x.key === k) }}; \
        q('.tk-warn')?.scrollIntoView({{ block: 'center' }}); await w({hold}); showTab('overview'); bsel.overview = 'stuck:cired:' + k; render(); await w({hold}); return r; }})()"), 30 + 2 * hold / 1000);
    let _ = fs::write(dir.join("ticket-ci.json"), json!({ "key": key, "status": r["ci"]["status"] }).to_string());
    r["ciGreen"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const k = {k}, w = ms => new Promise(o => setTimeout(o, ms)), f = () => tickets.list.find(x => x.key === k); \
        for (let t = 0; t < 60000 && f()?.ci?.status !== 'passed'; t += 2500) {{ await bridge.ticketCi(k); await w(2500); }} return f()?.ci?.status || null; }})()"), 70);
    // CLAUDE.md: the Start box said this project has none; its Tests line is added from the Skills screen (after the
    // confirm that shows it), then taken out again
    let cdir = json!(r["start"]["claudeDir"]);
    r["claudeMd"] = ask_for(app, panel::LABEL, &format!("(async () => {{ const w = ms => new Promise(o => setTimeout(o, ms)), q = s => document.querySelector('#b-dyn ' + s); \
        const until = async (g, ms) => {{ for (let t = 0; t < ms; t += 100) {{ if (g()) return true; await w(100); }} return false; }}; \
        audDir = {cdir}; aud = {{ busy: true }}; bsel.skills = 'aud:claude'; showTab('skills'); await loadAudit(); bsel.skills = 'aud:claude'; render(); await w(300); \
        const r = {{ before: aud.claude?.test ?? null, suggest: aud.testCmd }}; q('[data-sk=testsAdd]').click(); \
        await until(() => $('#sk-dialog').open, 5000); r.lines = $('#sk-dialog .b-code').textContent.includes('Tests: `npm test`'); await w({hold}); $('#sk-dialog [data-sk-yes]').click(); \
        r.added = await until(() => aud.claude?.test === true && !!aud.claude.undo, 8000); bsel.skills = 'aud:claude'; render(); await w(200 + {hold}); \
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
        && r["mr"]["unpushed"].as_u64().unwrap_or(0) > 0 && r["mr"]["opened"] == true && r["mr"]["bodyHasChecks"] == true
        && r["ci"]["status"] == "failed" && r["ci"]["fix"] == true && r["ci"]["stuck"] == true && r["ciGreen"] == "passed"
        && r["start"]["claudeWarn"] == true && r["claudeMd"]["added"] == true && r["claudeMd"]["removed"] == true);
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
