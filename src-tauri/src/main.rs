// Bobix on Tauri: the Rust core (hook events, session state, settings, local server), the pets overlay, the panel and
// the tray. Port of src/main.js, part by part; what is not ported yet (pet pictures) still runs only in the Electron build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod answers;
mod autostart;
mod feedback;
mod hooks;
mod i18n;
mod js;
mod links;
mod notify;
mod overlay;
mod panel;
mod paths;
mod procs;
mod project;
mod server;
mod settings;
mod smoke;
mod state;
mod tray;
mod update;
mod work;

use serde_json::{json, Value};
use state::{Applied, Sessions};
use std::{sync::{Arc, Mutex}, thread, time::Duration};
use tauri::{ipc::CapabilityBuilder, AppHandle, Manager, RunEvent};
use tokio::sync::broadcast;

pub const BRIDGE: &str = include_str!("../bridge.js");
const STALE_MS: u64 = 12 * 3600 * 1000;

pub struct Core {
    pub sessions: Sessions,
    pub settings: Value,
    pub rects: Vec<[f64; 4]>,
    pub interactive: bool,
    pub last_event_at: Option<u64>,
    pub page_errors: Vec<String>,
    pub pending: std::collections::HashMap<String, answers::Pending>, // held permission requests by answer id
    pub node_path: Option<String>,
    pub work: work::Work,
    pub update: Option<(String, String)>, // a newer release: (version, its page)
    pub update_told: Option<String>,       // the version a notification was shown for
    pub main_errors: Vec<String>,          // last errors of the app itself, for feedback
    pub save_gen: u64,                     // settings saves are debounced: only the newest one writes
}

pub struct Bobix {
    pub core: Mutex<Core>,
    pub tx: broadcast::Sender<String>,
    pub token: String,
    pub port: u16,
    pub app: std::sync::OnceLock<AppHandle>,
}

impl Bobix {
    // What the pages get on /events: the same JSON as snapshot() in src/main.js. Pet pictures come later.
    pub fn snapshot(&self) -> String {
        let c = self.core.lock().unwrap();
        let (view, issue_of) = c.work.view(&c.settings, &c.sessions);
        let by_host: std::collections::HashMap<&str, &str> = c.sessions.values()
            .filter_map(|s| s.host_session.as_deref().map(|h| (h, s.id.as_str()))).collect();
        let pics = &c.settings["petImages"];
        let list: Vec<Value> = c.sessions.values().map(|s| {
            let mut v = serde_json::to_value(s).unwrap_or_default();
            let project = s.project.clone().unwrap_or_default();
            let pic = &pics[&project];
            v["parent"] = s.parent_host.as_deref().and_then(|p| by_host.get(p)).map(|id| json!(id)).unwrap_or(Value::Null);
            v["project"] = json!(project);
            v["image"] = Value::Null;
            v["pixel"] = json!(pic.is_object());
            v["whole"] = json!(pic["opts"]["whole"] == true);
            v["issue"] = issue_of.get(&s.id).cloned().unwrap_or(Value::Null);
            v
        }).collect();
        let waiting: Vec<Value> = c.work.new_for.iter().map(|(dir, n)| json!({ "dir": dir, "path": n["path"], "iid": n["iid"] })).collect();
        json!({ "work": view, "waiting": waiting, "sessions": list }).to_string()
    }

    pub fn broadcast(self: &Arc<Self>) {
        let _ = self.tx.send(self.snapshot());
        if let Some(app) = self.app.get() { overlay::wake(app, self); }
    }

    // A hook event. Returns what the hook waits for: a held permission request's answer, or a new session's name.
    pub fn on_event(self: &Arc<Self>, e: Value, wait: bool) -> Option<server::Reply> {
        let now = state::now();
        let ev = e.get("hook_event_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let start = ev == "SessionStart";
        let untitled = !e["session_title"].as_str().is_some_and(|t| !t.is_empty());
        let source = e["source"].as_str().unwrap_or("");
        let (id, notice, since, reply, git_dir, gitlab) = {
            let mut c = self.core.lock().unwrap();
            c.last_event_at = Some(now);
            let Core { sessions, pending, settings, work, .. } = &mut *c;
            let gitlab = settings["gitlab"] == true;
            match state::apply(sessions, &e, now) {
                None => return None,
                Some(Applied::Removed) => (None, None, 0, None, None, gitlab),
                Some(Applied::Updated { id, notice }) => {
                    let s = sessions.get_mut(&id).unwrap();
                    state::pin_project(s);
                    find_parent(s, now);
                    let git_dir = git_due(work, s, ev == "Stop" || start, now);
                    let reply = if wait && ev == "PermissionRequest" { answers::wait(self, pending, settings, s, &e) }
                        // A session that starts on an issue (New in the Work tab, or its branch) gets its name from Bobix,
                        // the way /rename names it; one that has a name keeps it.
                        else if start && source == "startup" && untitled && work.new_for.contains_key(&s.cwd) { Some(take_issue(work, settings, s)) }
                        else if start && ["startup", "resume", "fork"].contains(&source) && untitled { self.title_for(work.hosts(settings), &s.cwd) }
                        else { None };
                    (Some(id), notice, s.since, reply, git_dir, gitlab)
                }
            }
        };
        if let Some(d) = git_dir { refresh_git(self, d); }
        if (start || ev == "SessionEnd") && gitlab {
            let b = self.clone(); // Claude writes the session's name a moment after
            thread::spawn(move || { thread::sleep(Duration::from_secs(3)); read_claude_list(&b); b.broadcast(); });
        }
        let is_stop = ev == "Stop";
        let b = self.clone();
        let finish = move || {
            let stale = {
                let mut c = b.core.lock().unwrap();
                match id.as_ref().and_then(|id| c.sessions.get_mut(id)) {
                    Some(s) => {
                        // A new prompt may land inside the Stop delay: then this Stop is stale (no old answer, no "finished" ping).
                        let stale = is_stop && (s.state != "done" || s.since != since);
                        if !stale { state::refresh(s, is_stop, state::now()); }
                        stale
                    }
                    None => is_stop,
                }
            };
            b.broadcast();
            if let Some(n) = notice.filter(|_| !stale) { b.notify(n.kind, &n.id); }
        };
        if is_stop {
            // let Claude flush the final answer to the transcript first
            tauri::async_runtime::spawn(async move { tokio::time::sleep(Duration::from_millis(300)).await; finish(); });
        } else { finish(); }
        reply
    }

    // "<name> needs you" / "<name> finished", as the settings allow. A click opens the session; on macOS a held
    // request can be answered from the notification.
    fn notify(self: &Arc<Self>, kind: &str, id: &str) {
        let (note, aid) = {
            let c = self.core.lock().unwrap();
            let Some(s) = c.sessions.get(id) else { return };
            let st = &c.settings;
            if st["paused"] == true || (kind == "waiting" && st["notifyWaiting"] != true) || (kind == "done" && st["notifyDone"] != true) { return; }
            let l = lang(st);
            let name = if s.title.is_empty() { &s.name } else { &s.title };
            let aid = if kind == "waiting" && cfg!(target_os = "macos") { s.answer.as_ref().and_then(|a| a["id"].as_str()).map(String::from) } else { None };
            let body = if kind == "waiting" { s.ask.clone() } else if s.summary.is_empty() { s.quest.clone() } else { s.summary.clone() };
            (notify::Note { title: i18n::t(&l, &format!("notify.{kind}"), &[("name", name)]), body, sound: st["sound"] == true,
                buttons: aid.as_ref().map(|_| (i18n::t(&l, "ask.allow", &[]), i18n::t(&l, "ask.deny", &[]))) }, aid)
        };
        let (b, id) = (self.clone(), id.to_string());
        notify::show(note, move |r| match (r, aid) {
            (notify::Reply::Allow, Some(a)) => { answers::answer(&b, &a, "allow"); }
            (notify::Reply::Deny, Some(a)) => { answers::answer(&b, &a, "deny"); }
            _ => { b.open_session(&id, true, true); }
        });
    }

    // The name of a session that starts on an issue's branch ("<iid>-…") of a repository of that GitLab project, when
    // the issue is one assigned to you: "[KEY-iid] title". Nothing is guessed from timing.
    fn title_for(self: &Arc<Self>, hosts: Vec<String>, dir: &str) -> Option<server::Reply> {
        if hosts.is_empty() || !std::path::Path::new(dir).is_absolute() { return None; }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (b, dir) = (self.clone(), dir.to_string());
        thread::spawn(move || {
            let out = (|| {
                let (g, remotes) = work::git_status(&dir)?;
                let project = work::Work::path_in(&remotes, &hosts)?;
                let iid = work::issue_of_branch(g["branch"].as_str()?)?;
                let c = b.core.lock().unwrap();
                let title = c.work.find_issue(&project, iid)?["title"].as_str().unwrap_or("").to_string();
                let name = work::session_title_for(&work::key_of(&c.settings, &project), iid, &title);
                Some(json!({ "hookSpecificOutput": { "hookEventName": "SessionStart", "sessionTitle": name } }))
            })();
            let _ = tx.send(out);
        });
        Some(server::Reply::Output(rx))
    }

    // Open the session itself: in Claude desktop jump straight to it, else bring its terminal / IDE forward. Falls back
    // to its details in the panel. { ok } or { ok: false, reason: nopid | wayland | xdotool | notfound, app, folder } so
    // the panel can say why. tell: show the reason in the panel (pet clicks); the panel's own Open button shows it itself.
    pub fn open_session(self: &Arc<Self>, id: &str, prefer_focus: bool, tell: bool) -> Value {
        let (host, pid, cwd) = {
            let c = self.core.lock().unwrap();
            let Some(s) = c.sessions.get(id) else { return json!({ "ok": false, "reason": "notfound" }) };
            (s.host_session.clone(), s.claude_pid, s.cwd.clone())
        };
        let (mut reason, mut app) = ("notfound", String::new());
        if prefer_focus {
            if let Some(h) = host {
                let url = format!("claude://code/continue?session={}", h); // the id is [A-Za-z0-9_-] only (links::is_host_id)
                if tauri_plugin_opener::open_url(url, None::<&str>).is_ok() { return json!({ "ok": true }); }
            } else if let Some(pid) = pid {
                let chain = procs::ancestors(pid);
                if procs::focus_pids(&chain) { return json!({ "ok": true }); }
                app = procs::host_app(&procs::process_names(&chain)).unwrap_or_default();
                // A CLI in the Claude app's own terminal: the app's link brings its window forward, also on Wayland.
                if app == "Claude" && tauri_plugin_opener::open_url("claude://", None::<&str>).is_ok() { return json!({ "ok": true }); }
                if cfg!(target_os = "linux") { reason = if wayland() { "wayland" } else if procs::has_xdotool() { "notfound" } else { "xdotool" }; }
            } else {
                reason = "nopid"; // hooks run without Node.js (curl / PowerShell): no process to start from
            }
        }
        // Say why, and where the session is, so you can switch to it yourself.
        let fail = prefer_focus.then(|| json!({ "reason": reason, "app": app, "folder": cwd }));
        if let Some(a) = self.app.get() { panel::show(a, self, Some(id.to_string()), None, fail.clone().filter(|_| tell)); }
        let mut out = fail.unwrap_or_else(|| json!({ "reason": reason }));
        out.as_object_mut().unwrap().shift_insert(0, "ok".into(), json!(false));
        out
    }

    // A pretend session walks through start → work → done so you can see pets without Claude.
    pub fn test_event(self: &Arc<Self>) {
        let id = format!("bobix-test-{}", radix36(state::now()));
        let cwd = paths::home().join("bobix-test").to_string_lossy().into_owned();
        let title = i18n::t(&lang(&self.core.lock().unwrap().settings), "test.title", &[]);
        let b = self.clone();
        let send = move |name: &str, extra: Value| {
            let mut e = json!({ "session_id": id, "cwd": cwd, "hook_event_name": name });
            for (k, v) in extra.as_object().unwrap() { e[k] = v.clone(); }
            b.on_event(e, false);
        };
        send("SessionStart", json!({ "source": "test" }));
        send("UserPromptSubmit", json!({ "prompt": title }));
        // 1.5 s, 6 s and 60 s after the start, as in src/main.js
        thread::spawn(move || {
            for (wait, name, extra) in [(1500, "PreToolUse", json!({ "tool_name": "Read", "tool_input": { "file_path": "README.md" } })),
                (4500, "Stop", json!({})), (54000, "SessionEnd", json!({}))] {
                thread::sleep(Duration::from_millis(wait));
                send(name, extra);
            }
        });
    }
}

fn radix36(mut n: u64) -> String {
    let mut s = Vec::new();
    loop { s.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize]); n /= 36; if n == 0 { break; } }
    s.reverse();
    String::from_utf8(s).unwrap()
}

// The language the person reads: settings, else the system's.
pub fn lang(settings: &Value) -> String {
    match settings["language"].as_str() {
        Some(l @ ("en" | "vi")) => l.into(),
        _ => sys_locale().filter(|l| l.to_lowercase().starts_with("vi")).map(|_| "vi".into()).unwrap_or_else(|| "en".into()),
    }
}
fn sys_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .or_else(|| if cfg!(target_os = "macos") { std::process::Command::new("defaults").args(["read", "-g", "AppleLocale"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None })
}

// Side session of another Claude desktop session? That app writes the link into its session file some time after the
// session starts, so look again every 5 s, for up to 10 minutes.
fn find_parent(s: &mut state::Session, now: u64) -> bool {
    let Some(host) = s.host_session.clone() else { return false };
    if s.parent_host.is_some() || s.link_tries >= 120 || now.saturating_sub(s.link_at) < 5000 { return false; }
    s.link_at = now;
    s.link_tries += 1;
    s.parent_host = links::parent_of(&host, &links::sessions_dir());
    s.parent_host.is_some()
}

// Look once at what each new claudePid is (see state::gone).
fn check_pid(b: &Arc<Bobix>, id: String, pid: u32) {
    let b = b.clone();
    thread::spawn(move || {
        let name = procs::process_names(&[pid]).pop().unwrap_or_default();
        if let Some(s) = b.core.lock().unwrap().sessions.get_mut(&id) {
            if s.claude_pid == Some(pid) && state::is_claude_proc(&name) { s.pid_is_claude = Some(pid); }
        }
    });
}

// Every 5 s: forget gone and stale sessions, notice an interrupted turn (Esc / a denied permission runs no hook),
// link side sessions to their parents.
fn tick(b: &Arc<Bobix>) {
    let now = state::now();
    let mut changed = false;
    let mut to_check = Vec::new();
    {
        let mut c = b.core.lock().unwrap();
        for id in state::gone(&c.sessions, procs::is_alive) { c.sessions.shift_remove(&id); changed = true; }
        c.sessions.retain(|_, s| { let keep = now.saturating_sub(s.updated) <= STALE_MS; changed |= !keep; keep });
        for s in c.sessions.values_mut() {
            if let Some(pid) = s.claude_pid { if s.pid_checked != Some(pid) { s.pid_checked = Some(pid); to_check.push((s.id.clone(), pid)); } }
            if (s.state == "working" || s.state == "waiting") && state::refresh(s, false, now) { changed = true; }
            if find_parent(s, now) { changed = true; }
        }
    }
    for (id, pid) in to_check { check_pid(b, id, pid); }
    if changed { b.broadcast(); }
}

// ---------- work items: git status, GitLab issues ----------
// Git: per folder, read on a session's events and by the 30 s timer, at most every 30 s (on Stop and SessionStart at
// once). GitLab: only while its switch is on, from every GitLab the person's own glab is signed in to.

// Is the folder's git status due? Gives the session the cached one meanwhile. Called with the core locked.
fn git_due(work: &mut work::Work, s: &mut state::Session, force: bool, now: u64) -> Option<String> {
    if !std::path::Path::new(&s.cwd).is_absolute() { return None; }
    if work.git_cache.len() > 300 { work.git_cache.clear(); }
    let c = work.git_cache.entry(s.cwd.clone()).or_default();
    if c.git.is_some() && s.git.is_none() { s.git = c.git.clone(); }
    if c.busy || (!force && now.saturating_sub(c.at) < 30_000) { return None; }
    c.busy = true;
    Some(s.cwd.clone())
}

// Reads the folder's status and gives it to every session there; a folder outside git keeps what it had.
fn refresh_git(b: &Arc<Bobix>, dir: String) {
    let b = b.clone();
    thread::spawn(move || {
        let g = work::git_status(&dir);
        let changed = {
            let mut c = b.core.lock().unwrap();
            let Core { work, sessions, .. } = &mut *c;
            let e = work.git_cache.entry(dir.clone()).or_default();
            e.busy = false;
            e.at = state::now();
            let Some((git, remotes)) = g else { return };
            let was = e.git.replace(git.clone());
            work.remotes_of.insert(git["top"].as_str().unwrap_or("").to_string(), remotes);
            let mut changed = was.as_ref() != Some(&git);
            for x in sessions.values_mut().filter(|x| x.cwd == dir) {
                if x.git.as_ref() != Some(&git) { changed = true; x.git = Some(git.clone()); }
            }
            changed
        };
        if changed { b.broadcast(); }
    });
}

// The first session created in a folder New waits on takes that issue and its name.
fn take_issue(work: &mut work::Work, settings: &Value, s: &mut state::Session) -> server::Reply {
    let n = work.new_for.shift_remove(&s.cwd).unwrap_or_default();
    let (path, iid) = (n["path"].as_str().unwrap_or("").to_string(), n["iid"].as_i64().unwrap_or(0));
    s.started = Some(json!({ "path": path, "iid": iid }));
    let name = work::session_title_for(&work::key_of(settings, &path), iid, n["title"].as_str().unwrap_or(""));
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = tx.send(Some(json!({ "hookSpecificOutput": { "hookEventName": "SessionStart", "sessionTitle": name } })));
    server::Reply::Output(rx)
}

fn read_claude_list(b: &Bobix) {
    let list = links::claude_sessions(&links::sessions_dir());
    b.core.lock().unwrap().work.claude_list = list;
}

// GitLab issues, read again (in the background). Does nothing while GitLab is off, or while a read runs.
fn refresh_issues(b: &Arc<Bobix>) {
    let b = b.clone();
    thread::spawn(move || {
        {
            let mut c = b.core.lock().unwrap();
            if c.settings["gitlab"] != true { c.work.issues = json!({ "status": "off" }); drop(c); return b.broadcast(); }
            if c.work.issues_busy { return; }
            c.work.issues_busy = true;
            if c.work.issues["status"] == "off" { c.work.issues = json!({ "status": "loading" }); drop(c); b.broadcast(); }
        }
        let glab = { let g = b.core.lock().unwrap().work.glab_path.clone(); g.or_else(|| find_bin("glab")) };
        let hosts = glab.as_deref().and_then(work::glab_hosts);
        let at = state::now();
        let next = match (&glab, hosts) {
            (_, None) => json!({ "status": "noglab", "at": at }), // look again next time: it may be installed by then
            (_, Some(h)) if h.is_empty() => json!({ "status": "auth", "hosts": h, "at": at }),
            (Some(g), Some(h)) => {
                let rs: Vec<Value> = h.iter().map(|x| work::fetch_issues(g, x)).collect();
                let failed: Vec<Value> = rs.iter().zip(&h).filter(|(r, _)| r["ok"] != true)
                    .map(|(r, host)| json!({ "host": host, "detail": r.get("detail").unwrap_or(&r["reason"]) })).collect();
                if failed.len() == h.len() { json!({ "status": "error", "hosts": h, "at": at, "failed": failed }) } else {
                    let list: Vec<Value> = rs.iter().flat_map(|r| r["issues"].as_array().cloned().unwrap_or_default()).collect();
                    json!({ "status": "ok", "hosts": h, "at": at, "failed": failed, "list": list })
                }
            }
            (None, Some(_)) => unreachable!(),
        };
        {
            let mut c = b.core.lock().unwrap();
            c.work.glab_path = if next["status"] == "noglab" { None } else { glab };
            c.work.issues_busy = false;
            c.work.issues = if c.settings["gitlab"] == true { next } else { json!({ "status": "off" }) }; // switched off meanwhile
        }
        read_claude_list(&b);
        b.broadcast();
    });
}

// The panel's button on an issue: Resume opens the session working on it, exactly like a click on its pet; New opens a
// new Claude desktop session in the project's folder (the link Claude documents, no prompt sent).
fn start_issue(b: &Arc<Bobix>, path: &str, iid: i64) -> Value {
    let (g, i, id) = {
        let c = b.core.lock().unwrap();
        let (view, _) = c.work.view(&c.settings, &c.sessions);
        let Some(g) = view["local"].as_array().and_then(|l| l.iter().find(|g| g["path"] == path)).cloned() else { return json!({ "ok": false, "reason": "gone" }) };
        let Some(i) = g["issues"].as_array().and_then(|l| l.iter().find(|i| i["iid"].as_i64() == Some(iid))).cloned() else { return json!({ "ok": false, "reason": "gone" }) };
        let id = i["sessions"].as_array().into_iter().flatten().filter_map(|x| c.sessions.get(x.as_str()?)).max_by_key(|s| s.updated).map(|s| s.id.clone());
        (g, i, id)
    };
    if let Some(id) = id {
        let mut r = b.open_session(&id, true, false);
        r.as_object_mut().unwrap().shift_insert(0, "mode".into(), json!("resume"));
        return r;
    }
    if let Some(r) = i["resume"].as_str() { // named after the issue in Claude desktop, not seen by Bobix since it started
        let ok = tauri_plugin_opener::open_url(format!("claude://code/continue?session={}", work::enc(r)), None::<&str>).is_ok();
        return if ok { json!({ "ok": true, "mode": "resume" }) } else { json!({ "ok": false, "mode": "resume", "reason": "notfound" }) };
    }
    let dir = g["dir"].as_str().unwrap_or("").to_string();
    b.core.lock().unwrap().work.new_for.insert(dir.clone(), json!({ "path": path, "iid": iid, "title": i["title"] }));
    b.broadcast();
    match tauri_plugin_opener::open_url(format!("claude://code/new?folder={}", work::enc(&dir)), None::<&str>) {
        Ok(_) => json!({ "ok": true, "mode": "new", "dir": dir }),
        Err(e) => json!({ "ok": false, "mode": "new", "reason": e.to_string() }),
    }
}

// ---------- settings ----------
fn save_now(b: &Bobix) {
    let mut st = b.core.lock().unwrap().settings.clone();
    // A dev run on another port (BOBIX_PORT) keeps the file's own port.
    if std::env::var_os("BOBIX_PORT").is_some() { st["port"] = settings::load(&paths::settings_file())["port"].clone(); }
    if let Err(e) = settings::save(&paths::settings_file(), &st) { b.log_err(format!("settings: {e}")); }
}

fn port_free(port: u16) -> bool { std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() }

// A change from the panel or the tray: checked (settings::sanitize), applied, saved 300 ms later (sliders send a stream
// of changes). A new port is used only once it is free and the hooks point at it; then Bobix starts again on it.
pub fn update_settings(b: &Arc<Bobix>, patch: Value) -> Value {
    let (prev, next) = {
        let c = b.core.lock().unwrap();
        let mut merged = c.settings.clone();
        if let (Some(m), Some(p)) = (merged.as_object_mut(), patch.as_object()) { for (k, v) in p { m.insert(k.clone(), v.clone()); } }
        let next = settings::sanitize(&merged, &c.settings);
        (c.settings.clone(), next)
    };
    let with_error = |e: String| { let mut v = prev.clone(); v["error"] = json!(e); v };
    let changed = |k: &str| prev[k] != next[k];
    if changed("port") {
        let port = next["port"].as_u64().unwrap_or(4747) as u16;
        if !port_free(port) { return with_error(format!("port {port} is busy")); }
        b.core.lock().unwrap().settings = next.clone();
        if has_hooks() {
            let r = install_at(b, port);
            if let Some(e) = r.get("error") { b.core.lock().unwrap().settings = prev.clone(); return with_error(e.as_str().unwrap_or("").into()); }
        }
        save_now(b);
        if let Some(app) = b.app.get() { app.restart(); }
        return next;
    }
    let gen = {
        let mut c = b.core.lock().unwrap();
        c.settings = next.clone();
        c.save_gen += 1;
        c.save_gen
    };
    let sb = b.clone();
    thread::spawn(move || { thread::sleep(Duration::from_millis(300)); if sb.core.lock().unwrap().save_gen == gen { save_now(&sb); } });
    if changed("autostart") { if let Err(e) = autostart::set(next["autostart"] == true) { b.log_err(format!("autostart: {e}")); } }
    if let Some(app) = b.app.get() {
        if ["display", "edge", "size"].iter().any(|k| changed(k)) { overlay::place(app, b); }
        if changed("petsVisible") { overlay::show_or_hide(app, b); overlay::wake(app, b); }
        if ["petsVisible", "paused", "language", "seenVersion"].iter().any(|k| changed(k)) { tray::build(app, b); }
    }
    answers::release_unseen(b);
    if changed("gitlab") { refresh_issues(b); }
    if next["checkUpdates"] == true && prev["checkUpdates"] != true { let ub = b.clone(); thread::spawn(move || check_for_update(&ub)); }
    if next["checkUpdates"] != true && b.core.lock().unwrap().update.take().is_some() { if let Some(app) = b.app.get() { tray::build(app, b); } }
    if let Some(app) = b.app.get() { for w in [overlay::LABEL, panel::LABEL] { panel::emit(app, w, "settings", &next); } }
    next
}

// ---------- new version ----------
// Once at start and then daily: ask GitHub for the latest release. A newer one shows a notification once per version
// and a tray item that opens its release page.
fn check_for_update(b: &Arc<Bobix>) {
    let Some(app) = b.app.get() else { return };
    if b.core.lock().unwrap().settings["checkUpdates"] != true { return; }
    let current = app.package_info().version.to_string();
    let Some((v, url)) = update::check_latest(&current) else { return };
    let (tell, l) = {
        let mut c = b.core.lock().unwrap();
        if c.update.as_ref().is_some_and(|u| u.0 == v) { return; }
        c.update = Some((v.clone(), url.clone()));
        let tell = c.update_told.as_deref() != Some(v.as_str());
        c.update_told = Some(v.clone());
        (tell, lang(&c.settings))
    };
    tray::build(app, b);
    if !tell { return; }
    let note = notify::Note { title: i18n::t(&l, "update.title", &[("v", &v)]), body: i18n::t(&l, "update.body", &[("v", &current)]), sound: true, buttons: None };
    notify::show(note, move |_| { let _ = tauri_plugin_opener::open_url(url, None::<&str>); });
}

// ---------- diagnostics and feedback ----------
// "Copy diagnostics" in Setup: everything needed to debug pets / opening sessions, no prompts, tokens or full paths.
fn diagnostics(b: &Arc<Bobix>) -> Value {
    let app = b.app.get();
    let (st, pets, errors, sessions, node) = {
        let c = b.core.lock().unwrap();
        let s: Vec<(String, u64, usize, bool, Option<u32>, bool, Option<Value>)> = c.sessions.values()
            .map(|s| (s.state.clone(), s.since, s.log.len(), !s.title.is_empty(), s.claude_pid, s.host_session.is_some(), s.diag.clone())).collect();
        (c.settings.clone(), c.rects.len(), c.page_errors.clone(), s, c.node_path.is_some())
    };
    let pick: serde_json::Map<String, Value> = ["language", "edge", "size", "opacity", "showIdle", "sleepHide", "onlyAttention", "petsVisible", "zoneStart", "zoneEnd", "display", "clickAction"]
        .iter().map(|k| (k.to_string(), st[*k].clone())).collect();
    let h = hooks::status(&paths::home());
    let rect = |p: tauri::PhysicalPosition<i32>, z: tauri::PhysicalSize<u32>, f: f64| json!({ "x": (p.x as f64 / f).round(), "y": (p.y as f64 / f).round(), "width": (z.width as f64 / f).round(), "height": (z.height as f64 / f).round() });
    let primary = app.and_then(|a| a.primary_monitor().ok().flatten()).and_then(|m| m.name().cloned());
    let displays: Vec<Value> = app.and_then(|a| a.available_monitors().ok()).unwrap_or_default().iter().map(|m| {
        let f = m.scale_factor();
        json!({ "id": m.name(), "bounds": rect(*m.position(), *m.size(), f), "workArea": rect(m.work_area().position, m.work_area().size, f), "scale": f, "primary": m.name().cloned() == primary })
    }).collect();
    let ov = app.and_then(|a| a.get_webview_window(overlay::LABEL)).map(|w| {
        let f = w.scale_factor().unwrap_or(1.0);
        json!({ "ready": true, "visible": w.is_visible().unwrap_or(false), "bounds": match (w.outer_position(), w.outer_size()) { (Ok(p), Ok(z)) => rect(p, z, f), _ => Value::Null },
            "alwaysOnTop": w.is_always_on_top().unwrap_or(false), "page": { "pets": pets }, "errors": errors })
    });
    let now = state::now();
    let sessions: Vec<Value> = sessions.into_iter().map(|(state, since, events, title, pid, host, diag)| {
        let chain = pid.map(procs::ancestors).unwrap_or_default();
        json!({ "state": state, "sinceMin": js::round(now.saturating_sub(since) as f64 / 60000.0), "events": events, "title": title, "claudePid": pid.is_some(), "hostSession": host,
            "hostShape": diag.as_ref().map(|d| d["hostShape"].clone()).unwrap_or(Value::Null), "env": diag.as_ref().map(|d| d["env"].clone()).unwrap_or(Value::Null),
            "chain": procs::process_names(&chain) })
    }).collect();
    json!({ "app": app.map(|a| a.package_info().version.to_string()), "platform": platform(), "release": os_release(), "arch": arch(),
        "session": std::env::var("XDG_SESSION_TYPE").ok(), "wayland": wayland(), "ozone": Value::Null,
        "settings": pick, "hooks": { "mode": h["mode"], "complete": h["complete"], "port": h["port"], "stale": h["stale"], "outdated": h["outdated"], "error": h.get("error").cloned().unwrap_or(Value::Null) },
        "node": node, "displays": displays, "overlay": ov, "sessions": sessions })
}

// os.release() of Node: the kernel's release (macOS, Linux), Windows' version number.
fn os_release() -> String {
    if cfg!(windows) {
        let v = procs::exec("cmd", &["/c", "ver"], 5, &[]).out;
        return v.split("Version ").nth(1).map(|x| x.trim().trim_end_matches(']').to_string()).unwrap_or_default();
    }
    procs::exec("uname", &["-r"], 5, &[]).out.trim().to_string()
}
fn arch() -> &'static str { match std::env::consts::ARCH { "x86_64" => "x64", "aarch64" => "arm64", "x86" => "ia32", a => a } }

// What a bug report will say.
fn feedback_info_of(b: &Arc<Bobix>) -> feedback::Info {
    let d = diagnostics(b);
    let errs: Vec<String> = { let c = b.core.lock().unwrap(); c.page_errors.iter().chain(c.main_errors.iter()).cloned().collect() };
    feedback::summarize(&d, &errs, &paths::home().to_string_lossy())
}

impl Bobix {
    // An error of the app itself: printed, and kept for a bug report (redacted there).
    pub fn log_err(&self, m: String) {
        eprintln!("[bobix] {m}");
        let mut c = self.core.lock().unwrap();
        c.main_errors.push(m.chars().take(300).collect());
        if c.main_errors.len() > 20 { c.main_errors.remove(0); }
    }
}

// Another copy of Bobix was opened: show the panel; a different version is told to quit the running one first.
fn on_second_start(b: &Arc<Bobix>, version: String) {
    let Some(app) = b.app.get() else { return };
    panel::show(app, b, None, None, None);
    let mine = app.package_info().version.to_string();
    if version.is_empty() || version == mine { return; }
    let l = lang(&b.core.lock().unwrap().settings);
    notify::show(notify::Note { title: i18n::t(&l, "other.title", &[("v", &version)]), body: i18n::t(&l, "other.body", &[("v", &mine)]), sound: true, buttons: None }, |_| {});
}

// ---------- commands the pages call (window.bobix, see bridge.js) ----------
type B<'a> = tauri::State<'a, Arc<Bobix>>;

#[tauri::command]
fn state(b: B, app: AppHandle) -> Value {
    let c = b.core.lock().unwrap();
    let version = app.package_info().version.to_string();
    // The pets' screen (its work area, in points) and every screen to choose from (named, as the overlay finds them).
    let screen = overlay::target_monitor(&app, c.settings["display"].as_str().unwrap_or("primary")).map(|m| {
        let (a, f) = (m.work_area(), m.scale_factor());
        json!({ "width": (a.size.width as f64 / f).round(), "height": (a.size.height as f64 / f).round() })
    });
    let displays: Vec<Value> = app.available_monitors().unwrap_or_default().iter().enumerate().map(|(i, m)| {
        let (z, f) = (m.size(), m.scale_factor());
        let name = m.name().cloned().unwrap_or_else(|| format!("#{}", i + 1));
        json!({ "id": name, "label": format!("{name} ({}×{})", (z.width as f64 / f).round(), (z.height as f64 / f).round()) })
    }).collect();
    json!({ "settings": c.settings, "version": version, "news": settings::news_for(&c.settings, &version, &i18n::NEWS), "platform": platform(),
        "nodePath": c.node_path, "lastEventAt": c.last_event_at, "hooks": hooks::status(&paths::home()), "screen": screen, "displays": displays })
}

#[tauri::command]
async fn settings_set(patch: Value, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || update_settings(&b, patch)).await.map_err(|_| ())
}

// Back to defaults, except the language you read, what changes the system (port → hooks + restart, start at login) and
// the projects' pet pictures (chosen files, not settings: wiping them would turn pets default at their next event).
#[tauri::command]
async fn settings_reset(b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    let mut patch = settings::defaults();
    { let c = b.core.lock().unwrap(); for k in ["language", "port", "autostart", "petImages", "guideSeen", "seenVersion"] { patch[k] = c.settings[k].clone(); } }
    tauri::async_runtime::spawn_blocking(move || update_settings(&b, patch)).await.map_err(|_| ())
}

#[tauri::command]
fn reveal_settings() {
    if let Some(f) = hooks::status(&paths::home())["file"].as_str() { let _ = tauri_plugin_opener::reveal_item_in_dir(f); }
}

#[tauri::command]
async fn diagnostics_copy(b: B<'_>) -> Result<String, ()> {
    let b = b.inner().clone();
    let text = tauri::async_runtime::spawn_blocking(move || serde_json::to_string_pretty(&diagnostics(&b)).unwrap_or_default()).await.map_err(|_| ())?;
    copy_text(&text);
    Ok(text)
}

// The system clipboard. Linux (X11) keeps a copy only while its owner lives: the clipboard stays open for the app's life.
fn copy_text(text: &str) {
    static CLIP: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);
    let mut c = CLIP.lock().unwrap();
    if c.is_none() { *c = arboard::Clipboard::new().ok(); }
    if let Some(cb) = c.as_mut() { if let Err(e) = cb.set_text(text) { eprintln!("[bobix] clipboard: {e}"); } }
}

#[tauri::command]
async fn feedback_info(b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || { let i = feedback_info_of(&b); json!({ "lines": i.lines, "errs": i.errs }) }).await.map_err(|_| ())
}

// Opens GitHub's form, filled in; the person submits it there.
#[tauri::command]
async fn feedback_send(v: Value, b: B<'_>) -> Result<Value, ()> {
    let Some(title) = v["title"].as_str().filter(|t| !t.trim().is_empty()).map(String::from) else { return Ok(json!({ "ok": false })) };
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let idea = v["kind"] == "idea";
        let desc = js::head16(v["description"].as_str().unwrap_or(""), 5000).to_string();
        let info = (!idea).then(|| feedback_info_of(&b));
        let (url, truncated) = feedback::issue_url(update::REPO, idea, &title, &desc, info.as_ref());
        let _ = tauri_plugin_opener::open_url(url, None::<&str>);
        json!({ "ok": true, "truncated": truncated })
    }).await.map_err(|_| ())
}

// GitLab issues, read again now (only while GitLab is on).
#[tauri::command]
fn work_refresh(b: B) { refresh_issues(&b); }

// In the browser; only links to the issues Bobix read open.
#[tauri::command]
fn work_open(url: String, b: B) -> bool {
    let ok = { let c = b.core.lock().unwrap(); c.work.open_issue_ok(&c.settings, &url) };
    ok && tauri_plugin_opener::open_url(url, None::<&str>).is_ok()
}

// Stop waiting for a new session in that folder.
#[tauri::command]
fn work_cancel(dir: String, b: B) {
    let gone = b.core.lock().unwrap().work.new_for.shift_remove(&dir).is_some();
    if gone { b.broadcast(); }
}

#[tauri::command]
async fn work_start(path: String, iid: i64, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || start_issue(&b, &path, iid)).await.map_err(|_| ())
}

#[tauri::command]
fn rects(r: Vec<Vec<f64>>, b: B, app: AppHandle) {
    let r: Vec<[f64; 4]> = r.into_iter().filter(|x| x.len() == 4 && x.iter().all(|v| v.is_finite())).map(|x| [x[0], x[1], x[2], x[3]]).collect();
    b.core.lock().unwrap().rects = r;
    overlay::show_or_hide(&app, &b);
}

// A click on a pet: bring its session forward (Settings → click action "focus"). The panel comes with the panel work.
#[tauri::command]
fn pet_click(id: String, b: B) {
    let focus = b.core.lock().unwrap().settings["clickAction"] == "focus";
    let b = b.inner().clone();
    thread::spawn(move || { b.open_session(&id, focus, true); });
}

#[tauri::command]
async fn open_session(id: String, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || b.open_session(&id, true, false)).await.map_err(|_| ())
}

#[tauri::command]
fn pet_set(id: String, mode: String, b: B) {
    let changed = state::set_pet(&mut b.core.lock().unwrap().sessions, &id, &mode);
    if changed { answers::release_unseen(&b); b.broadcast(); }
}

#[tauri::command]
fn answer(id: String, choice: String, b: B) -> bool { answers::answer(&b, &id, &choice) }

#[tauri::command]
fn install_hooks(b: B) -> Value { install(&b) }

#[tauri::command]
fn uninstall_hooks() -> Value {
    match hooks::uninstall(&paths::home()) { Ok(bak) => json!({ "file": hooks::settings_path(&paths::home()), "backup": bak }), Err(e) => json!({ "error": e }) }
}

#[tauri::command]
fn test_event(b: B) -> Value { b.inner().test_event(); json!({}) }

#[tauri::command]
fn page_error(msg: String, b: B) {
    let mut c = b.core.lock().unwrap();
    c.page_errors.push(msg.chars().take(300).collect());
    if c.page_errors.len() > 20 { c.page_errors.remove(0); }
}

#[tauri::command]
fn smoke_page(data: Value) { smoke::page_result(data); }

pub const COMMANDS: &[&str] = &["state", "rects", "pet_click", "pet_set", "answer", "test_event", "page_error", "smoke_page", "install_hooks", "uninstall_hooks",
    "open_session", "settings_set", "settings_reset", "reveal_settings", "diagnostics_copy", "feedback_info", "feedback_send", "work_refresh", "work_open", "work_cancel", "work_start"];

pub fn install(b: &Bobix) -> Value { install_at(b, b.port) }

fn install_at(b: &Bobix, port: u16) -> Value {
    let node = b.core.lock().unwrap().node_path.clone();
    match hooks::install(port, node.as_deref(), &paths::home()) {
        Ok(r) => json!({ "file": r.file, "backup": r.backup, "mode": r.mode }),
        Err(e) => json!({ "error": e }),
    }
}

// Linux on Wayland (the session type, as src/main.js reads it).
pub fn wayland() -> bool {
    cfg!(target_os = "linux") && (std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland") || std::env::var_os("WAYLAND_DISPLAY").is_some())
}

// Where `name` is, as the person's shell finds it: `where` on Windows, a login shell (then an interactive one, where
// nvm and the like set PATH) elsewhere. The Dock and the Start menu start apps without the shell's PATH.
pub fn find_bin(name: &str) -> Option<String> {
    let ok = |l: &str| { let low = l.to_lowercase(); (low.ends_with(name) || low.ends_with(&format!("{name}.exe"))) && std::path::Path::new(l).exists() };
    let try_cmd = |cmd: &str, args: &[&str], first: bool| -> Option<String> {
        let out = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if !first { lines.reverse(); } // an interactive shell may print noise before the answer: last wins
        lines.into_iter().find(|l| ok(l)).map(String::from)
    };
    if cfg!(windows) { return try_cmd("where", &[name], true); }
    let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let q = format!("command -v {name}");
    try_cmd(&sh, &["-lc", &q], false).or_else(|| try_cmd(&sh, &["-lic", &q], false))
}

// At start: hooks from an older build, or with a node that is gone (nvm / Homebrew), are installed again; else the
// copy of hook.js is refreshed, so app updates reach it.
fn refresh_hooks(b: &Bobix) {
    let home = paths::home();
    let st = hooks::status(&home);
    let node_mode = st["mode"] == "node";
    let has_node = b.core.lock().unwrap().node_path.is_some();
    if st["outdated"] == true || (node_mode && st["stale"] == true && has_node) {
        let r = install(b);
        if let Some(e) = r.get("error") { eprintln!("[bobix] hook refresh: {e}"); }
    } else if node_mode && hooks::hook_target(&home).exists() {
        if let Err(e) = std::fs::write(hooks::hook_target(&home), hooks::HOOK_JS) { eprintln!("[bobix] hook refresh: {e}"); }
    }
}

pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") { "darwin" } else if cfg!(windows) { "win32" } else { "linux" }
}

// ponytail: stderr only; Electron shows an error box here, which comes with the dialog plugin.
fn fail(msg: String) -> ! {
    eprintln!("[bobix] {msg}");
    std::process::exit(1);
}

fn main() {
    let file = paths::settings_file();
    autostart::migrate_from_old_name(&file);
    let mut settings = settings::load(&file);
    // Start at login: point at this copy, not one from an old path. Run from a clone (a dev build), a missing login entry
    // of its own doesn't mean "off": the installed app shares this settings file and owns the setting, so leave it as stored.
    let on = autostart::get();
    if !tauri::is_dev() || on { settings["autostart"] = json!(on); }
    if on { if let Err(e) = autostart::set(true) { eprintln!("[bobix] autostart: {e}"); } }
    // Dev and smoke runs next to an installed Bobix: another port, never saved (save_now).
    if let Some(p) = std::env::var("BOBIX_PORT").ok().and_then(|p| p.parse::<u16>().ok()) { settings["port"] = json!(p); }
    let port = settings["port"].as_u64().unwrap_or(4747) as u16;
    let token = paths::ensure_token().unwrap_or_else(|e| fail(format!("token: {e}")));
    let (tx, _) = broadcast::channel(64);
    let b = Arc::new(Bobix {
        core: Mutex::new(Core { sessions: Sessions::new(), settings, rects: vec![], interactive: false, last_event_at: None, page_errors: vec![], pending: Default::default(),
            node_path: None, work: Default::default(), update: None, update_told: None, main_errors: vec![], save_gen: 0 }),
        tx, token: token.clone(), port, app: Default::default(),
    });

    let rb = b.clone();
    tauri::Builder::default()
        .manage(b.clone())
        .invoke_handler(tauri::generate_handler![state, rects, pet_click, pet_set, answer, test_event, page_error, smoke_page, install_hooks, uninstall_hooks, open_session,
            settings_set, settings_reset, reveal_settings, diagnostics_copy, feedback_info, feedback_send, work_refresh, work_open, work_cancel, work_start])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // no Dock icon, like LSUIElement
            let handle = app.handle().clone();
            let _ = b.app.set(handle.clone());
            let version = app.package_info().version.to_string();
            let first = { let c = b.core.lock().unwrap(); settings::first_start(&c.settings, &version, has_hooks()) };
            if let Some(f) = first {
                { let mut c = b.core.lock().unwrap(); for (k, v) in f.as_object().unwrap() { c.settings[k] = v.clone(); } }
                save_now(&b);
            }

            // The pages are served by our own server, a "remote" origin to Tauri: only they may call the commands.
            let mut cap = CapabilityBuilder::new("pages").remote(format!("http://127.0.0.1:{port}/*")).windows([overlay::LABEL, panel::LABEL]);
            for c in COMMANDS { cap = cap.permission(format!("allow-{}", c.replace('_', "-"))); }
            app.add_capability(cap)?;

            let (eb, sb, ob, assets) = (b.clone(), b.clone(), b.clone(), handle.clone());
            let ctx = Arc::new(server::Ctx {
                token: token.clone(),
                on_event: Box::new(move |e, wait| eb.on_event(e, wait)),
                snapshot: Box::new(move || sb.snapshot()),
                asset: Box::new(move |f| assets.asset_resolver().get(f.to_string()).map(|a| a.bytes)),
                tx: b.tx.clone(),
                on_show: Box::new(move |v| on_second_start(&ob, v)),
            });
            if let Err(e) = server::start(port, ctx) {
                // One Bobix at a time: the running one shows its panel (and hears this copy's version).
                if server::show_running(port, &token, &version) { std::process::exit(0); }
                fail(format!("Port {port} is busy ({e}). Is Bobix already running?\nChange \"port\" in {}", paths::settings_file().display()));
            }

            let hb = b.clone();
            thread::spawn(move || { let node = find_bin("node"); hb.core.lock().unwrap().node_path = node; refresh_hooks(&hb); });
            overlay::create(&handle, &b)?;
            tray::build(&handle, &b);
            overlay::track_mouse(handle.clone(), b.clone());
            let tb = b.clone();
            thread::spawn(move || loop { thread::sleep(Duration::from_secs(5)); tick(&tb); });
            // Screens change (plugged, resolution): no event for it, so look every 5 s.
            let (pb, ph) = (b.clone(), handle.clone());
            thread::spawn(move || {
                let key = |h: &AppHandle| h.available_monitors().map(|m| m.iter().map(|m| format!("{:?}{:?}", m.name(), m.work_area())).collect::<Vec<_>>().join("|")).unwrap_or_default();
                let mut last = key(&ph);
                loop { thread::sleep(Duration::from_secs(5)); let k = key(&ph); if k != last { last = k; overlay::place(&ph, &pb); } }
            });
            let gb = b.clone(); // every 30 s: git status of each session's folder (when due)
            thread::spawn(move || loop {
                thread::sleep(Duration::from_secs(30));
                let due: Vec<String> = { let mut c = gb.core.lock().unwrap(); let Core { work, sessions, .. } = &mut *c; let now = state::now();
                    sessions.values_mut().filter_map(|s| git_due(work, s, false, now)).collect() };
                for d in due { refresh_git(&gb, d); }
            });
            if let Ok(dir) = std::env::var("BOBIX_SMOKE") { smoke::run(handle, b.clone(), dir.into()); return Ok(()); }
            let ib = b.clone(); // does nothing while GitLab is off
            thread::spawn(move || loop { refresh_issues(&ib); thread::sleep(Duration::from_secs(300)); });
            let ub = b.clone();
            thread::spawn(move || { thread::sleep(Duration::from_secs(15)); loop { check_for_update(&ub); thread::sleep(Duration::from_secs(24 * 3600)); } });
            if !has_hooks() { panel::show(&handle, &b, None, Some("setup".into()), None); } // first run: help connect Claude Code
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while starting Bobix")
        .run(move |app, e| match e {
            // The Dock icon or the app opened again from Finder: the panel.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => panel::show(app, &rb, None, None, None),
            RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(), // keep running in the tray
            RunEvent::Exit => save_now(&rb),
            _ => { let _ = app; }
        });
}

fn has_hooks() -> bool { hooks::status(&paths::home())["events"].as_array().is_some_and(|a| !a.is_empty()) }
