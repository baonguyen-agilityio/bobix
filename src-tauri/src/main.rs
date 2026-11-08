// Bobix on Tauri: the Rust core (hook events, session state, settings, local server) and the pets overlay.
// Port of src/main.js, part by part; what is not ported yet still runs only in the Electron build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod answers;
mod hooks;
mod i18n;
mod js;
mod links;
mod notify;
mod overlay;
mod paths;
mod procs;
mod project;
mod server;
mod settings;
mod smoke;
mod state;

use serde_json::{json, Value};
use state::{Applied, Sessions};
use std::{sync::{Arc, Mutex}, thread, time::Duration};
use tauri::{ipc::CapabilityBuilder, AppHandle, Manager};
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
}

pub struct Bobix {
    pub core: Mutex<Core>,
    pub tx: broadcast::Sender<String>,
    pub token: String,
    pub port: u16,
    pub app: std::sync::OnceLock<AppHandle>,
}

impl Bobix {
    // What the pages get on /events: the same JSON as snapshot() in src/main.js. The Work tab and pet pictures come later.
    pub fn snapshot(&self) -> String {
        let c = self.core.lock().unwrap();
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
            v["issue"] = Value::Null;
            v
        }).collect();
        json!({ "work": { "status": "off" }, "waiting": [], "sessions": list }).to_string()
    }

    pub fn broadcast(self: &Arc<Self>) {
        let _ = self.tx.send(self.snapshot());
        if let Some(app) = self.app.get() { overlay::wake(app, self); }
    }

    // A hook event. Returns what the hook waits for: a held permission request's answer (session titles come with
    // the Work tab).
    pub fn on_event(self: &Arc<Self>, e: Value, wait: bool) -> Option<server::Reply> {
        let now = state::now();
        let ev = e.get("hook_event_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let (id, notice, since, reply) = {
            let mut c = self.core.lock().unwrap();
            c.last_event_at = Some(now);
            let Core { sessions, pending, settings, .. } = &mut *c;
            match state::apply(sessions, &e, now) {
                None => return None,
                Some(Applied::Removed) => (None, None, 0, None),
                Some(Applied::Updated { id, notice }) => {
                    let s = sessions.get_mut(&id).unwrap();
                    state::pin_project(s);
                    find_parent(s, now);
                    let reply = if wait && ev == "PermissionRequest" { answers::wait(self, pending, settings, s, &e) } else { None };
                    (Some(id), notice, s.since, reply)
                }
            }
        };
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
            _ => { b.open_session(&id, true); }
        });
    }

    // Open the session itself: in Claude desktop jump straight to it, else bring its terminal / IDE forward.
    // { ok } or { ok: false, reason: nopid | wayland | xdotool | notfound, app, folder } so the panel can say why.
    // ponytail: the panel fallback (open it on the session with the reason) comes with the panel.
    pub fn open_session(self: &Arc<Self>, id: &str, prefer_focus: bool) -> Value {
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
        json!({ "ok": false, "reason": reason, "app": app, "folder": cwd })
    }

    // A pretend session walks through start → work → done so you can see pets without Claude.
    pub fn test_event(self: &Arc<Self>) {
        let id = format!("bobix-test-{}", radix36(state::now()));
        let cwd = paths::home().join("bobix-test").to_string_lossy().into_owned();
        let vi = { let c = self.core.lock().unwrap(); lang(&c.settings) == "vi" };
        let title = if vi { "Phiên thử" } else { "Test session" };
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

// ---------- commands the pages call (window.bobix, see bridge.js) ----------
type B<'a> = tauri::State<'a, Arc<Bobix>>;

#[tauri::command]
fn state(b: B, app: AppHandle) -> Value {
    let c = b.core.lock().unwrap();
    json!({ "settings": c.settings, "version": app.package_info().version.to_string(), "platform": platform(), "lastEventAt": c.last_event_at,
        "nodePath": c.node_path, "hooks": hooks::status(&paths::home()) })
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
    thread::spawn(move || { let r = b.open_session(&id, focus); if r["ok"] != true { println!("[bobix] open session: {}", r["reason"]); } });
}

#[tauri::command]
async fn open_session(id: String, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || b.open_session(&id, true)).await.map_err(|_| ())
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

pub const COMMANDS: &[&str] = &["state", "rects", "pet_click", "pet_set", "answer", "test_event", "page_error", "smoke_page", "install_hooks", "uninstall_hooks", "open_session"];

pub fn install(b: &Bobix) -> Value {
    let node = b.core.lock().unwrap().node_path.clone();
    match hooks::install(b.port, node.as_deref(), &paths::home()) {
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
    let mut settings = settings::load(&paths::settings_file());
    // Dev and smoke runs next to an installed Bobix: another port, not saved.
    if let Some(p) = std::env::var("BOBIX_PORT").ok().and_then(|p| p.parse::<u16>().ok()) { settings["port"] = json!(p); }
    let port = settings["port"].as_u64().unwrap_or(4747) as u16;
    let token = paths::ensure_token().unwrap_or_else(|e| fail(format!("token: {e}")));
    let (tx, _) = broadcast::channel(64);
    let b = Arc::new(Bobix {
        core: Mutex::new(Core { sessions: Sessions::new(), settings, rects: vec![], interactive: false, last_event_at: None, page_errors: vec![], pending: Default::default(), node_path: None }),
        tx, token: token.clone(), port, app: Default::default(),
    });

    tauri::Builder::default()
        .manage(b.clone())
        .invoke_handler(tauri::generate_handler![state, rects, pet_click, pet_set, answer, test_event, page_error, smoke_page, install_hooks, uninstall_hooks, open_session])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // no Dock icon, like LSUIElement
            let handle = app.handle().clone();
            let _ = b.app.set(handle.clone());

            // The pages are served by our own server, a "remote" origin to Tauri: only they may call the commands.
            let mut cap = CapabilityBuilder::new("pages").remote(format!("http://127.0.0.1:{port}/*")).windows([overlay::LABEL, "panel"]);
            for c in COMMANDS { cap = cap.permission(format!("allow-{}", c.replace('_', "-"))); }
            app.add_capability(cap)?;

            let (eb, sb, assets) = (b.clone(), b.clone(), handle.clone());
            let ctx = Arc::new(server::Ctx {
                token: token.clone(),
                on_event: Box::new(move |e, wait| eb.on_event(e, wait)),
                snapshot: Box::new(move || sb.snapshot()),
                asset: Box::new(move |f| assets.asset_resolver().get(f.to_string()).map(|a| a.bytes)),
                tx: b.tx.clone(),
            });
            if let Err(e) = server::start(port, ctx) {
                fail(format!("Port {port} is busy ({e}). Is Bobix already running?\nChange \"port\" in {}", paths::settings_file().display()));
            }

            let hb = b.clone();
            thread::spawn(move || { let node = find_bin("node"); hb.core.lock().unwrap().node_path = node; refresh_hooks(&hb); });
            overlay::create(&handle, &b)?;
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
            if let Ok(dir) = std::env::var("BOBIX_SMOKE") { smoke::run(handle, b.clone(), dir.into()); }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bobix");
}
