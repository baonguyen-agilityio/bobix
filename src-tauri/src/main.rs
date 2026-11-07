// Bobix on Tauri: the Rust core (hook events, session state, settings, local server) and the pets overlay.
// Port of src/main.js, part by part; what is not ported yet still runs only in the Electron build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod js;
mod links;
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

    // A hook event. Returns what the hook waits for, if anything (answers and session titles come later).
    pub fn on_event(self: &Arc<Self>, e: Value, _wait: bool) -> Option<server::Reply> {
        let now = state::now();
        let ev = e.get("hook_event_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let (id, notice, since) = {
            let mut c = self.core.lock().unwrap();
            c.last_event_at = Some(now);
            match state::apply(&mut c.sessions, &e, now) {
                None => return None,
                Some(Applied::Removed) => (None, None, 0),
                Some(Applied::Updated { id, notice }) => {
                    let s = c.sessions.get_mut(&id).unwrap();
                    state::pin_project(s);
                    find_parent(s, now);
                    let since = s.since;
                    (Some(id), notice, since)
                }
            }
        };
        let is_stop = ev == "Stop";
        let b = self.clone();
        let finish = move || {
            {
                let mut c = b.core.lock().unwrap();
                if let Some(s) = id.as_ref().and_then(|id| c.sessions.get_mut(id)) {
                    // A new prompt may land inside the Stop delay: then this Stop is stale (no old answer, no "finished" ping).
                    let stale = is_stop && (s.state != "done" || s.since != since);
                    if !stale { state::refresh(s, is_stop, state::now()); }
                }
            }
            b.broadcast();
            if let Some(n) = notice { println!("[bobix] notice {} {}", n.kind, n.id); } // notifications come with the hooks work
        };
        if is_stop {
            // let Claude flush the final answer to the transcript first
            tauri::async_runtime::spawn(async move { tokio::time::sleep(Duration::from_millis(300)).await; finish(); });
        } else { finish(); }
        None
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
    json!({ "settings": c.settings, "version": app.package_info().version.to_string(), "platform": platform(), "lastEventAt": c.last_event_at })
}

#[tauri::command]
fn rects(r: Vec<Vec<f64>>, b: B, app: AppHandle) {
    let r: Vec<[f64; 4]> = r.into_iter().filter(|x| x.len() == 4 && x.iter().all(|v| v.is_finite())).map(|x| [x[0], x[1], x[2], x[3]]).collect();
    b.core.lock().unwrap().rects = r;
    overlay::show_or_hide(&app, &b);
}

#[tauri::command]
fn pet_click(id: String) {
    println!("[bobix] pet click {id}"); // focus / panel come with those parts of the port
}

#[tauri::command]
fn pet_set(id: String, mode: String, b: B) {
    let changed = state::set_pet(&mut b.core.lock().unwrap().sessions, &id, &mode);
    if changed { b.broadcast(); }
}

#[tauri::command]
fn answer(id: String, choice: String) -> bool {
    let _ = (id, choice); // permission answers come with the hooks work
    false
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

pub const COMMANDS: &[&str] = &["state", "rects", "pet_click", "pet_set", "answer", "test_event", "page_error", "smoke_page"];

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
        core: Mutex::new(Core { sessions: Sessions::new(), settings, rects: vec![], interactive: false, last_event_at: None, page_errors: vec![] }),
        tx, token: token.clone(), port, app: Default::default(),
    });

    tauri::Builder::default()
        .manage(b.clone())
        .invoke_handler(tauri::generate_handler![state, rects, pet_click, pet_set, answer, test_event, page_error, smoke_page])
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
