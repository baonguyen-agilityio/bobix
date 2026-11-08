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
                thread::sleep(Duration::from_secs(60));
                let _ = fs::write(dir.join("report.json"), json!({ "watchdog": "gave up after 60 s" }).to_string());
                log("gave up after 60 s");
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
            json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Looking at the code." }], "usage": { "input_tokens": 10, "cache_read_input_tokens": 40000 } } })];
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
                answers::answer(&b, &id, "allow");
                let out = held.join().unwrap_or_default();
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
            report["realSession"] = c.sessions.get(sid).map(|s| json!({ "state": s.state, "title": s.title, "name": s.name, "claudePid": s.claude_pid, "hostSession": s.host_session }))
                .unwrap_or(json!("not received"));
            report["sessions"] = json!(c.sessions.len());
        }
        if let Some(w) = app.get_webview_window(overlay::LABEL) {
            let scale = w.scale_factor().unwrap_or(1.0);
            let (p, s) = (w.outer_position().unwrap_or_default().to_logical::<f64>(scale), w.outer_size().unwrap_or_default().to_logical::<f64>(scale));
            report["visible"] = json!(w.is_visible().unwrap_or(false));
            report["onTop"] = json!(w.is_always_on_top().unwrap_or(false));
            report["bounds"] = json!({ "x": p.x, "y": p.y, "width": s.width, "height": s.height });
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
                    }} catch (e) {{ r.error = String(e); }} window.__TAURI_INTERNALS__.invoke('smoke_page', {{ data: {{ panel: r }} }}); }})()", project = json!(project)));
                let until = Instant::now() + Duration::from_secs(10);
                while PAGE.lock().unwrap().is_none() && Instant::now() < until { thread::sleep(Duration::from_millis(100)); }
                report["panel"] = PAGE.lock().unwrap().take().map(|v| v["panel"].clone()).unwrap_or(json!("error: no answer from the panel"));
            }
        }
        thread::sleep(Duration::from_millis(500)); // the 300 ms save
        report["settingsSaved"] = json!(crate::settings::load(&paths::settings_file())["size"] == 64);
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
        // Linux: drag a pet with the real mouse (xdotool) and check it moved: the input shape lets the
        // mouse in over a pet. Not part of the verdict where xdotool is missing.
        if cfg!(target_os = "linux") && crate::procs::exec("xdotool", &["version"], 5, &[]).ok {
            // The pretend session's pet: finished by now, so it rests (a working pet walks away from the cursor).
            let rest = b.core.lock().unwrap().sessions.keys().find(|k| k.starts_with("bobix-test-")).cloned().unwrap_or_default();
            report["drag"] = drag(&app, &rest);
        }
        report["pageErrors"] = json!(b.core.lock().unwrap().page_errors); // both pages
        let pass = report["answer"] == "ok" && report["hooks"]["complete"] == true && report["visible"] == true && report["onTop"] == true && report["pets"].as_u64().unwrap_or(0) >= 2
            && report["pageErrors"].as_array().is_some_and(|a| a.is_empty()) && report["realSession"].is_object()
            && report["panel"]["size"] == 64 && report["panel"]["tabs"] == 3 && report["panel"]["selected"] == "settings" && report["panel"]["feedbackLines"] == 6
            && ["preview", "save", "served", "reset"].iter().all(|k| report["panel"]["picture"][k] == true)
            && (report.get("drag").is_none() || report["drag"]["moved"].as_f64().unwrap_or(0.0) > 200.0)
            && report["settingsSaved"] == true && report["tray"] == true && report["autostart"] == true && report["git"].is_object() && report["secondStart"] == true;
        report["pass"] = json!(pass);
        write(&report);
        log(&format!("done {report}"));
        let _ = fs::write(dir.join("done"), ""); // CI waits for this, then stops the app itself
        app.exit(if pass { 0 } else { 1 });
    });
}

// The value of a page expression (awaited), through the bridge like the pets' own calls.
fn ask(app: &AppHandle, label: &str, expr: &str) -> Value {
    let Some(w) = app.get_webview_window(label) else { return json!("no window") };
    *PAGE.lock().unwrap() = None;
    let _ = w.eval(format!("(async () => {{ let v; try {{ v = await ({expr}); }} catch (e) {{ v = {{ error: String(e) }}; }} \
        window.__TAURI_INTERNALS__.invoke('smoke_page', {{ data: {{ v }} }}); }})()"));
    let until = Instant::now() + Duration::from_secs(5);
    while PAGE.lock().unwrap().is_none() && Instant::now() < until { thread::sleep(Duration::from_millis(50)); }
    PAGE.lock().unwrap().take().map(|v| v["v"].clone()).unwrap_or(json!("no answer"))
}

// Drags the pet toward the middle of the screen (so the zone never clamps it), as scripts/smoke-drag.mjs did.
fn drag(app: &AppHandle, sid: &str) -> Value {
    let at = ask(app, overlay::LABEL, &format!("(() => {{ const p = pets.get({sid:?}); if (!p) return null; const b = p.el.querySelector('.spr').getBoundingClientRect(); \
        return {{ x: Math.round(screenX + b.left + b.width / 2), y: Math.round(screenY + b.top + b.height / 2), petX: Math.round(p.x), w: innerWidth }}; }})()"));
    let (Some(x), Some(y), Some(px), Some(w)) = (at["x"].as_i64(), at["y"].as_i64(), at["petX"].as_f64(), at["w"].as_f64()) else { return json!({ "error": "no pet", "at": at }) };
    let dir = if px > w / 2.0 { -1 } else { 1 };
    let xdo = |a: &[String]| { let a: Vec<&str> = a.iter().map(String::as_str).collect(); crate::procs::exec("xdotool", &a, 5, &[]); };
    xdo(&["mousemove".into(), x.to_string(), y.to_string()]);
    thread::sleep(Duration::from_millis(400));
    xdo(&["mousedown".into(), "1".into()]);
    thread::sleep(Duration::from_millis(100));
    for i in 1..=15 { xdo(&["mousemove".into(), (x + dir * i * 20).to_string(), y.to_string()]); thread::sleep(Duration::from_millis(40)); }
    xdo(&["mouseup".into(), "1".into()]);
    thread::sleep(Duration::from_millis(800));
    let after = ask(app, overlay::LABEL, &format!("Math.round(pets.get({sid:?}).x)"));
    json!({ "from": px, "to": after, "moved": after.as_f64().map(|a| (a - px) * dir as f64) })
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
