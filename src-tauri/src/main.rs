// Bobix on Tauri: the Rust core (hook events, session state, settings, local server), the pets overlay, the panel and
// the tray. Port of the 0.1 (Electron) app's main.js.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "256"] // serde_json's json! for the settings defaults (settings.rs) has grown past the default 128

mod answers;
mod autostart;
mod feedback;
mod hooks;
mod i18n;
mod js;
mod habits;
mod links;
mod model;
mod notify;
mod overlay;
mod panel;
mod paths;
mod petstore;
mod procs;
mod process;
mod project;
mod server;
mod settings;
mod skill_audit;
mod habit_skills;
mod room;
mod skills;
mod smoke;
mod state;
mod tickets;
mod tray;
mod update;
mod usage;
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
    pub habits: habits::Mem,               // the permission requests waiting, for the habit log (memory only)
    pub waits: Vec<state::Wait>,           // times Claude waited for your answer, the last 36 h (memory only)
    pub tickets: tickets::Flows,           // the ticket flow (#112): kept in the data folder, this machine only
    pub usage: Value,                      // usage::Store::meta: the price table's date, how many past sessions make the normal
}

pub struct Bobix {
    pub core: Mutex<Core>,
    pub tx: broadcast::Sender<String>,
    pub token: String,
    pub port: u16,
    pub app: std::sync::OnceLock<AppHandle>,
    pub usage: Mutex<usage::Store>,        // per-session usage tallies (their own lock: reading a transcript never holds the core)
}

// What the pages get on /events: the same JSON as snapshot() in the 0.1 (Electron) app's main.js.
// The work model goes along whole ("model"); the board, the pets and the report read their links from it.
fn snapshot_of(c: &Core) -> Value {
    let m = model::Model::build(&c.settings, &c.sessions, &c.work);
    let view = c.work.view(&c.settings, &m);
    let pics = &c.settings["petImages"];
    let list: Vec<Value> = c.sessions.values().zip(&m.sessions).map(|(s, x)| {
        let mut v = serde_json::to_value(s).unwrap_or_default();
        let project = s.project.clone().unwrap_or_default();
        let pic = &pics[&project];
        v["parent"] = x.parent.as_ref().map(|p| json!(p.to.id)).unwrap_or(Value::Null);
        v["linking"] = json!(state::linking(s, state::now())); // held back until its parent link is known
        v["project"] = json!(project);
        v["image"] = json!(petstore::served(pic));
        v["pixel"] = json!(pic.is_object());
        v["whole"] = json!(pic["opts"]["whole"] == true);
        v["mark"] = json!(c.tickets.marked(&s.id)); // a new assumption in its work log (the ticket flow)
        v["flow"] = json!(c.tickets.by_session(&s.id).map(|f| &f.key));
        v["issue"] = x.issue.as_ref().map(|i| {
            let it = m.issue(&i.to.project, i.to.iid);
            json!({ "iid": i.to.iid, "title": it.map_or(json!(""), |it| it.title.clone()), "url": it.map_or(json!(""), |it| it.url.clone()) })
        }).unwrap_or(Value::Null);
        v
    }).collect();
    let waiting: Vec<Value> = c.work.new_for.iter().map(|(dir, n)| json!({ "dir": dir, "path": n["path"], "iid": n["iid"] })).collect();
    let tickets = c.tickets.view(tickets::free_bytes(&paths::home()));
    json!({ "work": view, "waiting": waiting, "sessions": list, "model": m.json(), "waits": c.waits, "usage": c.usage, "tickets": tickets })
}

impl Bobix {
    pub fn snapshot(&self) -> String { snapshot_of(&self.core.lock().unwrap()).to_string() }

    pub fn broadcast(self: &Arc<Self>) {
        let _ = self.tx.send(self.snapshot());
        if let Some(app) = self.app.get() { overlay::wake(app, self); tray::badge(app, self, false); }
    }

    // A hook event. Returns what the hook waits for: a held permission request's answer, or a new session's name.
    pub fn on_event(self: &Arc<Self>, e: Value, wait: bool) -> Option<server::Reply> {
        let now = state::now();
        let ev = e.get("hook_event_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let start = ev == "SessionStart";
        let untitled = !e["session_title"].as_str().is_some_and(|t| !t.is_empty());
        let source = e["source"].as_str().unwrap_or("");
        let mut habit_lines = vec![];
        let (id, notice, since, reply, git_dir, gitlab) = {
            let mut c = self.core.lock().unwrap();
            c.last_event_at = Some(now);
            let Core { sessions, pending, settings, work, waits, habits: mem, tickets: flows, .. } = &mut *c;
            let gitlab = work::any_on(settings);
            let remember = habits::on(settings);
            let before = if remember { habits::before(sessions, &e) } else { None };
            match state::apply(sessions, &e, now) {
                None => return None,
                Some(Applied::Removed(mut s)) => {
                    state::keep_waits(waits, &mut s, now);
                    if remember { habit_lines = habits::events(mem, before.as_ref(), &e, None, now); }
                    (None, None, 0, None, None, gitlab)
                }
                Some(Applied::Updated { id, notice }) => {
                    let s = sessions.get_mut(&id).unwrap();
                    state::pin_project(s);
                    state::keep_waits(waits, s, now);
                    if remember {
                        if !mem.projects.contains_key(&id) { // its tracker project, once its git remote is known
                            let hosts = work.hosts(settings);
                            if let Some(p) = s.git.as_ref().and_then(|g| work.remotes_of.get(g["top"].as_str()?)).and_then(|r| model::path_in(r, &hosts)) { mem.projects.insert(id.clone(), p); }
                        }
                        habit_lines = habits::events(mem, before.as_ref(), &e, Some(s), now);
                    }
                    find_parent(s, now);
                    let git_dir = git_due(work, s, ev == "Stop" || start, now);
                    let tool = e["tool_name"].as_str().unwrap_or("");
                    if flows.on_event(&id, &s.cwd, &ev, tool, e["tool_input"]["plan"].as_str(), now) { flows.save(); }
                    let plan = tool == "ExitPlanMode" && flows.by_session(&id).is_some(); // the flow's one gate: held until answered
                    let reply = if wait && ev == "PermissionRequest" { answers::wait(self, pending, settings, s, &e, plan) }
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
        remember(habit_lines, now);
        if (start || ev == "SessionEnd") && gitlab {
            let b = self.clone(); // Claude writes the session's name a moment after
            thread::spawn(move || { thread::sleep(Duration::from_secs(3)); read_claude_list(&b); b.broadcast(); });
        }
        let is_stop = ev == "Stop";
        let b = self.clone();
        let finish = move || {
            let stale = {
                let mut c = b.core.lock().unwrap();
                let Core { sessions, waits, .. } = &mut *c;
                match id.as_ref().and_then(|id| sessions.get_mut(id)) {
                    Some(s) => {
                        // A new prompt may land inside the Stop delay: then this Stop is stale (no old answer, no "finished" ping).
                        let stale = is_stop && (s.state != "done" || s.since != since);
                        if !stale { let now = state::now(); state::refresh(s, is_stop, now); state::keep_waits(waits, s, now); }
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
            let dnd = st["dndUntil"].as_f64().unwrap_or(0.0) > state::now() as f64; // "do not disturb": no notification either
            if st["paused"] == true || dnd || (kind == "waiting" && st["notifyWaiting"] != true) || (kind == "done" && st["notifyDone"] != true) { return; }
            let l = lang(st);
            let name = if s.title.is_empty() { &s.name } else { &s.title };
            let aid = if kind == "waiting" && cfg!(target_os = "macos") { s.answer.as_ref().and_then(|a| a["id"].as_str()).map(String::from) } else { None };
            let body = if kind == "waiting" { s.ask.clone() } else if s.summary.is_empty() { s.quest.clone() } else { s.summary.clone() };
            (notify::Note { title: i18n::t(&l, &format!("notify.{kind}"), &[("name", name)]), body, sound: st["sound"] == true,
                buttons: aid.as_ref().map(|_| (i18n::t(&l, "ask.allow", &[]), i18n::t(&l, "ask.deny", &[]))), open: i18n::t(&l, "notify.open", &[]) }, aid)
        };
        let (b, id) = (self.clone(), id.to_string());
        notify::show(note, move |r| match (r, aid) {
            (notify::Reply::Allow, Some(a)) => { answers::answer(&b, &a, "allow"); }
            (notify::Reply::Deny, Some(a)) => { answers::answer(&b, &a, "deny"); }
            _ => { b.open_session(&id, true, true); }
        });
    }

    // The name of a session that starts on an issue's branch ("<iid>-…") of a repository of that project, when
    // the issue is one assigned to you: "[KEY-iid] title". Nothing is guessed from timing.
    fn title_for(self: &Arc<Self>, hosts: Vec<String>, dir: &str) -> Option<server::Reply> {
        if hosts.is_empty() || !std::path::Path::new(dir).is_absolute() { return None; }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (b, dir) = (self.clone(), dir.to_string());
        thread::spawn(move || {
            let out = (|| {
                let (g, remotes) = work::git_status(&dir)?;
                let project = model::path_in(&remotes, &hosts)?;
                let iid = model::issue_of_branch(g["branch"].as_str()?)?;
                let c = b.core.lock().unwrap();
                let title = c.work.find_issue(&c.settings, &project, iid)?["title"].as_str().unwrap_or("").to_string();
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
        // 1.5 s, 6 s and 60 s after the start, as in the 0.1 (Electron) app's main.js
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

// Side session of another Claude desktop session? See state::find_parent for when it looks.
fn find_parent(s: &mut state::Session, now: u64) -> bool {
    let found = state::find_parent(s, now, |h| links::parent_of(h, &links::sessions_dir()));
    // The real gap between a side session's first event and its parent link (no ids: the timing is what matters).
    if found && s.parent_host.is_some() { eprintln!("[bobix] side session linked {} ms after its first event ({} tries)", now.saturating_sub(s.seen_at), s.link_tries); }
    found
}

// Every 400 ms while a new Claude desktop session is held back (state::linking): look for its parent, and broadcast as
// soon as it is found or the hold ends, so its pet shows once, in the right place.
fn link_tick(b: &Arc<Bobix>) {
    let now = state::now();
    let mut changed = false;
    {
        let mut c = b.core.lock().unwrap();
        for s in c.sessions.values_mut() {
            if !s.held && !state::linking(s, now) { continue; }
            if state::linking(s, now) && find_parent(s, now) { changed = true; }
            let held = state::linking(s, now);
            if held != s.held { s.held = held; changed = true; }
        }
    }
    if changed { b.broadcast(); }
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
    let mut lines = vec![];
    {
        let mut c = b.core.lock().unwrap();
        let Core { sessions, settings, waits, habits: mem, .. } = &mut *c;
        let remember = habits::on(settings);
        let gone = state::gone(sessions, procs::is_alive);
        sessions.retain(|id, s| {
            let keep = !gone.contains(id) && now.saturating_sub(s.updated) <= STALE_MS;
            if !keep { state::leave_wait(s, now); state::keep_waits(waits, s, now); changed = true; if remember { lines.extend(habits::ended(mem, s, now)); } }
            keep
        });
        for s in sessions.values_mut() {
            if let Some(pid) = s.claude_pid { if s.pid_checked != Some(pid) { s.pid_checked = Some(pid); to_check.push((s.id.clone(), pid)); } }
            let (was, since) = (s.state.clone(), s.since);
            if (s.state == "working" || s.state == "waiting") && state::refresh(s, false, now) {
                state::keep_waits(waits, s, now);
                changed = true;
                if remember && was == "waiting" { lines.extend(habits::interrupted(mem, s, since, now)); }
            }
            if find_parent(s, now) { changed = true; }
        }
    }
    remember(lines, now);
    for (id, pid) in to_check { check_pid(b, id, pid); }
    if changed { b.broadcast(); } else if let Some(app) = b.app.get() { tray::badge(app, b, false); } // do not disturb may have ended
}

// Usage per session (usage.rs): transcripts are read under the usage lock only; the core is locked briefly before
// (which sessions, which files) and after (their views).
fn usage_tick(b: &Arc<Bobix>) {
    let (live, settings): (Vec<(String, String)>, Value) = {
        let c = b.core.lock().unwrap();
        (c.sessions.values().filter(|s| !s.transcript.is_empty()).map(|s| (s.id.clone(), s.transcript.clone())).collect(), c.settings.clone())
    };
    let (views, meta) = { let mut u = b.usage.lock().unwrap(); let v = u.tick(&live, &settings, state::now()); (v, u.meta()) };
    let changed = {
        let mut c = b.core.lock().unwrap();
        let mut changed = c.usage != meta;
        c.usage = meta;
        for s in c.sessions.values_mut() {
            let Some(v) = views.get(&s.id) else { continue };
            if &s.usage != v { s.usage = v.clone(); changed = true; }
        }
        changed
    };
    if changed { b.broadcast(); }
    if habit_skills::due(&settings, &paths::data_dir(), state::now()) { thread::spawn(|| { habit_scan(); }); }
}

// Habits become skills (habit_skills.rs): your own `claude -p`, found the way your shell finds it.
fn claude_bin() -> Result<String, String> {
    static BIN: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    BIN.get_or_init(|| find_bin("claude")).clone().ok_or_else(|| "claude is not installed (no `claude` on your PATH)".into())
}
fn habit_run(model: &str, tools: &str, prompt: &str) -> Result<String, String> {
    let secs = if model == "haiku" { 180 } else { 420 };
    habit_skills::claude(&claude_bin()?, &paths::data_dir().join("habit-skills/work"), model, tools, prompt, secs)
}

// ---------- the war room (room.rs) ----------

// Tailscale's CLI: where the shell finds it, else where the macOS app keeps it.
fn tailscale() -> Option<String> {
    static TS: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    TS.get_or_init(|| find_bin("tailscale").or_else(|| ["/usr/local/bin/tailscale", "/Applications/Tailscale.app/Contents/MacOS/Tailscale"].iter().find(|p| std::path::Path::new(p).exists()).map(|p| p.to_string()))).clone()
}
// This machine's tailnet address and its person, asked at most once a minute.
fn room_me() -> Option<(std::net::Ipv4Addr, room::Who)> {
    static ME: Mutex<Option<(u64, Option<(std::net::Ipv4Addr, room::Who)>)>> = Mutex::new(None);
    let now = room::now_ms();
    if let Some((at, v)) = ME.lock().unwrap().clone() { if now.saturating_sub(at) < 60_000 { return v; } }
    let ts = tailscale();
    let v = ts.as_deref().and_then(|ts| { let ip = room::my_ip(ts)?; Some((ip, room::whois(ts, &ip.to_string())?)) });
    *ME.lock().unwrap() = Some((now, v.clone()));
    v
}
// The room's server runs while this machine hosts an open room: on its tailnet address only.
fn room_serve(data: &std::path::Path) -> Value {
    if !room::any_open(data) { room::stop(); return json!({ "serving": null }); }
    if let Some(a) = room::running() { return json!({ "serving": a.to_string() }); }
    let (Some(ts), Some((ip, _))) = (tailscale(), room_me()) else { return json!({ "serving": null, "error": "Tailscale is not running" }) };
    let port = room::port();
    let host = std::sync::Arc::new(room::Host { data: data.to_path_buf(), host: format!("{ip}:{port}"), whois: Box::new(move |a| room::whois(&ts, a)), peer_ok: room::is_tailnet });
    match room::serve(std::net::IpAddr::V4(ip), port, host) { Ok(a) => json!({ "serving": a.to_string() }), Err(e) => json!({ "serving": null, "error": format!("port {port}: {e}") }) }
}
fn room_run(model: &str, tools: &str, prompt: &str) -> Result<String, String> {
    habit_skills::claude(&claude_bin()?, &paths::data_dir().join("rooms/work"), model, tools, prompt, 300)
}
fn room_act(act: &str, a: &Value) -> Value {
    let data = paths::data_dir();
    let s = |k: &str| a[k].as_str().unwrap_or("").to_string();
    let (id, now) = (s("id"), room::now_ms());
    let me = || room_me().map(|x| x.1).ok_or("Tailscale is not running, or this machine has no person on it");
    let fail = |e: &str| json!({ "ok": false, "error": e });
    let r = match act {
        "list" => {
            let m = room_me();
            let up = room_serve(&data);
            return json!({ "ok": true, "tailnet": { "cli": tailscale().is_some(), "ip": m.as_ref().map(|x| x.0.to_string()), "me": m.as_ref().map(|x| json!({ "login": x.1.login, "name": x.1.name, "node": x.1.node })),
                "port": room::port(), "serving": up["serving"], "error": up["error"] },
                "hosted": room::hosted(&data).iter().map(|r| { let mut v = r.clone(); v["record"] = json!(room::record_path(&data, r).is_file().then(|| room::record_path(&data, r))); v }).collect::<Vec<_>>(),
                "joined": room::joined(&data) });
        }
        "create" => match me() {
            Ok(w) => { let inv: Vec<String> = a["invite"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect(); room::create(&data, &w, &s("title"), &s("problem"), &inv, now) }
            Err(e) => fail(e),
        },
        "invite" | "uninvite" => room::invite(&data, &id, &s("login"), act == "invite"),
        "say" => match me() { Ok(w) => room::say(&data, &id, &w, &s("text"), now), Err(e) => fail(e) },
        "draft" => room::draft(&data, &id, &room_run),
        "save_draft" => room::save_draft(&data, &id, &a["draft"]),
        "decide" => room::decide(&data, &id, &a["draft"], now),
        "audit" => return json!({ "ok": true, "lines": room::audit_lines(&data, &id) }),
        "markdown" => return room::load(&data, &id).map_or_else(|| fail("no such room"), |r| json!({ "ok": true, "text": room::markdown(&r) })),
        "get" => room::remote_get(&data, &s("addr")),
        "rsay" => room::remote_say(&s("addr"), &s("text")),
        "leave" => room::leave(&data, &s("addr")),
        "open" => {
            let p = std::path::PathBuf::from(s("path"));
            // only a record Bobix saved
            if p.parent() != Some(room::records(&data).as_path()) || p.extension().is_none_or(|e| e != "html") || !p.is_file() { return fail("no such record"); }
            return match tauri_plugin_opener::open_path(&p, None::<&str>) { Ok(()) => json!({ "ok": true }), Err(e) => fail(&e.to_string()) };
        }
        _ => fail("unknown"),
    };
    if matches!(act, "create" | "decide") { room_serve(&data); }
    r
}
// The Room scope. act: list | create | invite | uninvite | say | draft | save_draft | decide | audit | markdown (host);
// get | rsay | leave (a room you joined); open (a saved record).
#[tauri::command]
async fn room(act: String, a: Value) -> Result<Value, ()> {
    tauri::async_runtime::spawn_blocking(move || room_act(&act, &a)).await.map_err(|_| ())
}
fn habit_scan() -> Value { habit_skills::scan(&paths::home(), &paths::data_dir(), skill_audit::now_ms(), &habit_run) }

// The Skills screen's "From your habits". act: view | scan | draft | approve | dismiss | restore | undo | remove; each
// change only on the person's click. Answers the view after it (and { ok: false, error } when the act failed).
#[tauri::command]
async fn habit_skills(act: String, id: Option<String>, text: Option<String>, v: Option<u64>, b: B<'_>) -> Result<Value, ()> {
    let st = b.core.lock().unwrap().settings.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (home, data, now, id) = (paths::home(), paths::data_dir(), skill_audit::now_ms(), id.unwrap_or_default());
        let r = match act.as_str() {
            "view" => json!({ "ok": true }),
            "scan" if !habit_skills::on(&st) => json!({ "ok": false, "error": "turn on the weekly scan first" }),
            "scan" => habit_scan(),
            "draft" if !habit_skills::on(&st) => json!({ "ok": false, "error": "turn on the weekly scan first" }),
            "draft" => habit_skills::draft(&home, &data, &id, now, &habit_run),
            "approve" => habit_skills::approve(&home, &data, &id, text.as_deref().unwrap_or(""), now),
            "dismiss" => habit_skills::dismiss(&data, &id),
            "restore" => habit_skills::restore(&home, &data, &id, v.unwrap_or(u64::MAX), now),
            "undo" => habit_skills::undo(&home, &data, &id, now),
            "remove" => habit_skills::remove(&home, &data, &id),
            _ => json!({ "ok": false, "error": "unknown" }),
        };
        let mut view = habit_skills::view(&home, &data, &st, now);
        view["result"] = r;
        view
    }).await.map_err(|_| ())
}

// The price table, opened in the person's editor (written from the built-in one first if it isn't there).
#[tauri::command]
fn usage_prices(b: B) {
    let f = { let mut u = b.usage.lock().unwrap(); u.prices(); usage::prices_file(&paths::data_dir()) };
    if let Err(e) = tauri_plugin_opener::open_path(&f, None::<&str>) { b.log_err(format!("prices: {e}")); }
}

// ---------- work items: git status, GitLab issues ----------
// Git: per folder, read on a session's events and by the 30 s timer, at most every 30 s (on Stop and SessionStart at
// once). GitLab / GitHub: each only while its switch is on, from every host the person's own glab / gh is signed in to.

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
        if !changed { return; }
        b.broadcast();
        // a session here now names an issue the board has not read: read the issues now, not at the next round
        let unread = { let c = b.core.lock().unwrap(); c.work.unread_link(&c.settings, &model::Model::build(&c.settings, &c.sessions, &c.work)) };
        if unread { refresh_issues(&b); }
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

// Issues from every tracker, read again (in the background).
fn refresh_issues(b: &Arc<Bobix>) { for f in work::FORGES { refresh_forge(b, f); } }

// One tracker's issues, read again. Does nothing while it is off, or while a read of it runs.
fn refresh_forge(b: &Arc<Bobix>, f: work::Forge) {
    let b = b.clone();
    thread::spawn(move || {
        {
            let mut c = b.core.lock().unwrap();
            let on = f.on(&c.settings);
            let feed = c.work.feed(f);
            if !on {
                if feed.issues["status"] == "off" { return; }
                feed.issues = json!({ "status": "off" });
                drop(c);
                return b.broadcast();
            }
            if feed.busy { return; }
            feed.busy = true;
            if feed.issues["status"] == "off" { feed.issues = json!({ "status": "loading" }); drop(c); b.broadcast(); }
        }
        let bin = { let g = b.core.lock().unwrap().work.feed(f).bin.clone(); g.or_else(|| find_bin(f.bin())) };
        // the issues sessions here work on: one not in your list is read on its own below
        let linked = { let c = b.core.lock().unwrap(); model::Model::build(&c.settings, &c.sessions, &c.work).linked() };
        let hosts = bin.as_deref().and_then(|x| f.hosts(x));
        let at = state::now();
        let (next, reviews) = match (&bin, hosts) {
            (_, None) => (json!({ "status": f.missing(), "at": at }), vec![]), // look again next time: it may be installed by then
            (_, Some(h)) if h.is_empty() => (json!({ "status": f.signed_out(), "hosts": h, "at": at }), vec![]),
            (Some(g), Some(h)) => {
                let rs: Vec<Value> = h.iter().map(|x| f.fetch(g, x)).collect();
                let failed: Vec<Value> = rs.iter().zip(&h).filter(|(r, _)| r["ok"] != true)
                    .map(|(r, host)| json!({ "host": host, "detail": r.get("detail").unwrap_or(&r["reason"]) })).collect();
                if failed.len() == h.len() { (json!({ "status": "error", "hosts": h, "at": at, "failed": failed }), vec![]) } else {
                    let mut list: Vec<Value> = rs.iter().flat_map(|r| r["issues"].as_array().cloned().unwrap_or_default()).collect();
                    // ponytail: 10 at most per read; more sessions on issues not assigned to you wait for the next read
                    let extra: Vec<&(String, String, i64)> = linked.iter().filter(|(host, p, iid)| h.contains(host)
                        && !list.iter().any(|i| i["project"] == p.as_str() && i["iid"].as_i64() == Some(*iid))).take(10).collect();
                    for (host, p, iid) in extra {
                        b.core.lock().unwrap().work.asked.insert(format!("{p}#{iid}"));
                        if let Some(i) = f.issue(g, host, p, *iid) { list.push(i); }
                    }
                    // what waits for review, and what was closed lately (the panel keeps today's: local midnight is its call)
                    let since = hooks::iso(at.saturating_sub(4 * 24 * 3600 * 1000)); // today, and the last working day for the stand-up
                    let reviews: Vec<model::Review> = h.iter().flat_map(|x| f.reviews(g, x)).collect();
                    let closed: Vec<Value> = h.iter().flat_map(|x| f.closed(g, x, &since)).collect();
                    (json!({ "status": "ok", "hosts": h, "at": at, "failed": failed, "list": list, "closed": closed }), reviews)
                }
            }
            (None, Some(_)) => unreachable!(),
        };
        {
            let mut c = b.core.lock().unwrap();
            let on = f.on(&c.settings);
            let feed = c.work.feed(f);
            feed.bin = if next["status"] == f.missing() { None } else { bin };
            feed.busy = false;
            feed.reviews = if on { reviews } else { vec![] };
            feed.issues = if on { next } else { json!({ "status": "off" }) }; // switched off meanwhile
        }
        read_claude_list(&b);
        b.broadcast();
    });
}

// The panel's button on an issue: Resume opens the session working on it, exactly like a click on its pet; New opens a
// new Claude desktop session in the project's folder (the link Claude documents, no prompt sent).
// `dir`: for a project no session has open here now (the habits' morning suggestion), the folder the habit log names;
// used only once its own git remote says it is that project.
fn start_issue(b: &Arc<Bobix>, path: &str, iid: i64, prompt: &str, dir: Option<&str>) -> Value {
    let gone = || json!({ "ok": false, "reason": "gone" });
    let (g, i, id, hosts) = {
        let c = b.core.lock().unwrap();
        let view = c.work.view(&c.settings, &model::Model::build(&c.settings, &c.sessions, &c.work));
        let g = view["local"].as_array().and_then(|l| l.iter().find(|g| g["path"] == path)).cloned();
        let i = match &g {
            Some(g) => g["issues"].as_array().and_then(|l| l.iter().find(|i| i["iid"].as_i64() == Some(iid))).cloned(),
            None => c.work.find_issue(&c.settings, path, iid).cloned(),
        };
        let Some(i) = i else { return gone() };
        let id = i["sessions"].as_array().into_iter().flatten().filter_map(|x| c.sessions.get(x.as_str()?)).max_by_key(|s| s.updated).map(|s| s.id.clone());
        (g, i, id, c.work.hosts(&c.settings))
    };
    let g = match g {
        Some(g) => g,
        None => {
            let Some(d) = dir.filter(|d| std::path::Path::new(d).is_absolute()) else { return gone() };
            if !work::git_status(d).is_some_and(|(_, remotes)| model::path_in(&remotes, &hosts).as_deref() == Some(path)) { return gone(); }
            json!({ "path": path, "dir": d })
        }
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
    match tauri_plugin_opener::open_url(work::new_session_link(&dir, prompt), None::<&str>) {
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
        if ["display", "edge", "size", "petMode"].iter().any(|k| changed(k)) { overlay::place(app, b); }
        if changed("petsVisible") { overlay::show_or_hide(app, b); overlay::wake(app, b); }
        if ["petsVisible", "paused", "language", "seenVersion"].iter().any(|k| changed(k)) { tray::build(app, b); }
        tray::badge(app, b, false);
    }
    answers::release_unseen(b);
    for f in work::FORGES { if changed(f.key()) { refresh_forge(b, f); } }
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
    let note = notify::Note { title: i18n::t(&l, "update.title", &[("v", &v)]), body: i18n::t(&l, "update.body", &[("v", &current)]), sound: true, buttons: None, open: i18n::t(&l, "notify.open", &[]) };
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
    let pick: serde_json::Map<String, Value> = ["language", "edge", "walkDir", "petMode", "dndUntil", "size", "opacity", "showIdle", "sleepHide", "onlyAttention", "petsVisible", "zoneStart", "zoneEnd", "display", "clickAction"]
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
        "session": std::env::var("XDG_SESSION_TYPE").ok(), "wayland": wayland(),
        "ozone": if cfg!(target_os = "linux") { json!(std::env::var("GDK_BACKEND").unwrap_or_else(|_| if wayland() { "wayland".into() } else { "x11".into() })) } else { Value::Null }, // how the overlay draws
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
    notify::show(notify::Note { title: i18n::t(&l, "other.title", &[("v", &version)]), body: i18n::t(&l, "other.body", &[("v", &mine)]), sound: true, buttons: None, open: i18n::t(&l, "notify.open", &[]) }, |_| {});
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
    { let c = b.core.lock().unwrap(); for k in ["language", "port", "autostart", "petImages", "guideSeen", "seenVersion", "tourSeen"] { patch[k] = c.settings[k].clone(); } }
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

// Issues, read again now (only from the trackers that are on).
#[tauri::command]
fn work_refresh(b: B) { refresh_issues(&b); }

// In the browser; only links to the issues Bobix read open.
#[tauri::command]
fn work_open(url: String, b: B) -> bool {
    let ok = { let c = b.core.lock().unwrap(); c.work.open_issue_ok(&c.settings, &url) };
    ok && tauri_plugin_opener::open_url(url, None::<&str>).is_ok()
}

// Habit lines go to Bobix's own data folder, off the hook's way.
fn remember(lines: Vec<Value>, now: u64) {
    if lines.is_empty() { return; }
    thread::spawn(move || { if let Err(e) = habits::write(&habits::dir(), &lines, now) { eprintln!("[bobix] habits: {e}"); } });
}

// "Bobix nhớ gì về bạn": everything kept (read only while it is on; the list and the size in Settings).
#[tauri::command]
async fn habits_read(b: B<'_>) -> Result<Value, ()> {
    let on = habits::on(&b.core.lock().unwrap().settings);
    tauri::async_runtime::spawn_blocking(move || {
        let mut v = if on { habits::read(&habits::dir(), state::now()) } else { json!({ "events": [], "bytes": 0, "days": 0, "dir": habits::dir().to_string_lossy() }) };
        v["on"] = json!(on);
        v
    }).await.map_err(|_| ())
}

// "Xoá hết": every day Bobix kept. Returns how many days went.
#[tauri::command]
async fn habits_clear() -> Result<usize, ()> {
    tauri::async_runtime::spawn_blocking(|| habits::clear(&habits::dir())).await.map_err(|_| ())
}

// Something done in the panel that habits look for (the stand-up, the report), while habits are on.
#[tauri::command]
fn habits_note(what: String, b: B) {
    if !habits::on(&b.core.lock().unwrap().settings) { return; }
    let now = state::now();
    remember(habits::ui(&what, now).into_iter().collect(), now);
}

// The pet's morning suggestion opens the board.
#[tauri::command]
fn show_board(b: B, app: AppHandle) { panel::show(&app, &b, None, Some("sessions".into()), None); }

// Stuck work: the page for a new pull / merge request from that branch, in the browser.
#[tauri::command]
fn work_new_review(project: String, branch: String, b: B) -> bool {
    let url = { let c = b.core.lock().unwrap(); work::new_review_url(&model::Model::build(&c.settings, &c.sessions, &c.work), &project, &branch) };
    url.is_some_and(|u| tauri_plugin_opener::open_url(u, None::<&str>).is_ok())
}

// Stop waiting for a new session in that folder.
#[tauri::command]
fn work_cancel(dir: String, b: B) {
    let gone = b.core.lock().unwrap().work.new_for.shift_remove(&dir).is_some();
    if gone { b.broadcast(); }
}

#[tauri::command]
async fn work_start(path: String, iid: i64, prompt: Option<String>, dir: Option<String>, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || start_issue(&b, &path, iid, prompt.as_deref().unwrap_or(""), dir.as_deref())).await.map_err(|_| ())
}

// ---------- the ticket flow (#112): Start in a worktree, approve the plan, the work log ----------
// An issue of a project a session here works in (its folder known), as Start reads it.
fn local_issue(c: &Core, path: &str, iid: i64) -> Option<(String, Value)> {
    let view = c.work.view(&c.settings, &model::Model::build(&c.settings, &c.sessions, &c.work));
    let g = view["local"].as_array()?.iter().find(|g| g["path"] == path)?;
    let i = g["issues"].as_array()?.iter().find(|i| i["iid"].as_i64() == Some(iid))?.clone();
    Some((g["dir"].as_str()?.to_string(), i))
}
// The repository's top folder (a session may work in a subfolder), its worktree for the issue, the branch, the disk.
fn ticket_place(b: &Bobix, path: &str, iid: i64) -> Result<(String, Value, std::path::PathBuf, String), Value> {
    let (dir, i) = { let c = b.core.lock().unwrap(); local_issue(&c, path, iid) }.ok_or_else(|| json!({ "ok": false, "reason": "gone" }))?;
    let repo = work::git_status(&dir).and_then(|(g, _)| g["top"].as_str().map(String::from)).ok_or_else(|| json!({ "ok": false, "reason": "gone" }))?;
    let wt = tickets::worktree_dir(&repo, iid);
    let branch = tickets::branch_for(iid, i["title"].as_str().unwrap_or(""));
    Ok((repo, i, wt, branch))
}

// For the Start box: where it will work and how the session is told to work. { ok, repo, dir, branch, rules, free, room, need, exists }
#[tauri::command]
async fn ticket_prep(path: String, iid: i64, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (repo, _, wt, branch) = match ticket_place(&b, &path, iid) { Ok(x) => x, Err(e) => return e };
        let free = tickets::free_bytes(std::path::Path::new(&repo));
        let exists = b.core.lock().unwrap().tickets.list.get(&tickets::key_of(&path, iid)).is_some_and(|f| !f.cleaned);
        json!({ "ok": true, "repo": repo, "dir": wt, "branch": branch, "rules": tickets::rules(&branch), "models": tickets::models_text(&json!({ "size": "small" })), "free": free, "room": tickets::room(free), "need": tickets::WORKTREE_BYTES, "exists": exists })
    }).await.map_err(|_| ())
}

// Start: a worktree on a new branch, the log's template in it, then a new Claude desktop session there with the prompt
// (how: desktop), or the line to paste in a terminal with the model and plan mode preset (how: cli). The first session
// created in that folder takes the issue (its name, as New does) and the flow.
#[tauri::command]
async fn ticket_start(path: String, iid: i64, prompt: String, models: Value, how: String, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (repo, i, wt, branch) = match ticket_place(&b, &path, iid) { Ok(x) => x, Err(e) => return e };
        let key = tickets::key_of(&path, iid);
        if b.core.lock().unwrap().tickets.list.get(&key).is_some_and(|f| !f.cleaned) { return json!({ "ok": false, "reason": "exists" }); }
        let free = tickets::free_bytes(std::path::Path::new(&repo));
        if free.is_some_and(|f| f < tickets::WORKTREE_BYTES) { return json!({ "ok": false, "reason": "disk", "free": free }); }
        let title = i["title"].as_str().unwrap_or("").to_string();
        let base = match tickets::create_worktree(&repo, &wt, &branch, &tickets::template(iid, &title)) { Ok(x) => x, Err(e) => return json!({ "ok": false, "reason": "git", "detail": e }) };
        let dir = wt.to_string_lossy().to_string();
        let models = if models.is_object() { models } else { json!({ "size": "small" }) };
        let cmd = tickets::cli_command(&dir, &tickets::main_model(&models), &prompt);
        {
            let mut c = b.core.lock().unwrap();
            c.tickets.list.shift_remove(&key);
            c.tickets.list.insert(key.clone(), tickets::Flow { key, path: path.clone(), iid, title: title.clone(), url: i["url"].as_str().unwrap_or("").into(), repo,
                dir: dir.clone(), branch, base, status: "starting".into(), models, at: state::now(), ..Default::default() });
            c.tickets.save();
            c.work.new_for.insert(dir.clone(), json!({ "path": path, "iid": iid, "title": title }));
        }
        b.broadcast();
        if how == "cli" { return json!({ "ok": true, "mode": "cli", "dir": dir, "cmd": cmd }); }
        match tauri_plugin_opener::open_url(work::new_session_link(&dir, &prompt), None::<&str>) {
            Ok(_) => json!({ "ok": true, "mode": "new", "dir": dir, "cmd": cmd }),
            Err(e) => json!({ "ok": false, "mode": "new", "reason": e.to_string(), "dir": dir, "cmd": cmd }),
        }
    }).await.map_err(|_| ())
}

// Only on your click: Haiku classifies the ticket through your own `claude -p` (no tools, no hooks). { ok, size, why, steps } | { ok: false, error }
#[tauri::command]
async fn ticket_classify(title: String, body: String) -> Result<Value, ()> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(claude) = find_bin("claude") else { return json!({ "ok": false, "error": "claude is not installed" }) };
        match tickets::classify(&claude, &title, &body) { Ok(mut v) => { v["ok"] = json!(true); v["text"] = json!(tickets::models_text(&v)); v["model"] = json!(tickets::main_model(&v)); v }, Err(e) => json!({ "ok": false, "error": e }) }
    }).await.map_err(|_| ())
}

// Approve the flow's plan (its session goes on in auto mode) or send it back with what to change.
#[tauri::command]
fn ticket_answer(key: String, approve: bool, change: Option<String>, b: B) -> bool {
    let id = {
        let c = b.core.lock().unwrap();
        c.tickets.list.get(&key).and_then(|f| c.sessions.get(f.session.as_deref()?)).and_then(|s| s.answer.as_ref()?["id"].as_str().map(String::from))
    };
    id.is_some_and(|id| answers::answer_with(&b, &id, if approve { "allow" } else { "deny" }, change.as_deref().unwrap_or("")))
}

// You looked at the flow: its assumptions so far are seen, the pet's mark goes.
#[tauri::command]
fn ticket_seen(key: String, b: B) {
    let changed = {
        let mut c = b.core.lock().unwrap();
        let Some(f) = c.tickets.list.get_mut(&key) else { return };
        let n = tickets::assumptions(&f.log);
        let ch = f.seen != n;
        f.seen = n;
        if ch { c.tickets.save(); }
        ch
    };
    if changed { b.broadcast(); }
}

// Only on your click: Haiku sums up what the log leaves out, from the log and the branch's commits. Kept with the flow.
#[tauri::command]
async fn ticket_summary(key: String, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(f) = b.core.lock().unwrap().tickets.list.get(&key).cloned() else { return json!({ "ok": false, "error": "gone" }) };
        let Some(claude) = find_bin("claude") else { return json!({ "ok": false, "error": "claude is not installed" }) };
        match tickets::summarize(&claude, &f) {
            Ok(t) => {
                { let mut c = b.core.lock().unwrap(); if let Some(x) = c.tickets.list.get_mut(&key) { x.summary = t; } c.tickets.save(); }
                b.broadcast();
                json!({ "ok": true })
            }
            Err(e) => json!({ "ok": false, "error": e }),
        }
    }).await.map_err(|_| ())
}

// Clean up a finished flow's worktree (git refuses when work is not committed; Bobix never forces). The log is kept.
#[tauri::command]
async fn ticket_clean(key: String, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(f) = b.core.lock().unwrap().tickets.list.get(&key).cloned() else { return json!({ "ok": false, "error": "gone" }) };
        if !matches!(f.status.as_str(), "done" | "blocked") { return json!({ "ok": false, "error": "running" }); }
        tickets::keep_log(&paths::data_dir(), &f);
        if let Err(e) = tickets::remove_worktree(&f.repo, &f.dir) { return json!({ "ok": false, "error": e }); }
        { let mut c = b.core.lock().unwrap(); if let Some(x) = c.tickets.list.get_mut(&key) { x.cleaned = true; } c.tickets.save(); c.work.new_for.shift_remove(&f.dir); }
        b.broadcast();
        json!({ "ok": true })
    }).await.map_err(|_| ())
}

// Settings → My skills: your own skills (names and descriptions, read-only), to pin the ones to offer.
#[tauri::command]
async fn skills_mine() -> Result<Value, ()> {
    tauri::async_runtime::spawn_blocking(|| json!(skills::mine(&paths::home()))).await.map_err(|_| ())
}

// The skills audit (skill_audit.rs). `dir`: a folder a session here works in (or its repository root); none: your own
// skills only. Every change needs the person's click, is backed up first and can be undone (skills_undo).
fn audit_dir(b: &Arc<Bobix>, dir: Option<String>) -> Result<Option<std::path::PathBuf>, Value> {
    let Some(d) = dir else { return Ok(None) };
    let p = std::path::PathBuf::from(&d);
    let known = b.core.lock().unwrap().sessions.values().any(|s| !s.cwd.is_empty() && (std::path::Path::new(&s.cwd) == p || skills::repo_root(std::path::Path::new(&s.cwd)) == p));
    if known && p.is_dir() { Ok(Some(p)) } else { Err(json!({ "ok": false, "error": "not a folder a session here works in" })) }
}

// { days, slashCounted, skills: [{ key, name, source, plugin, description, size, hidden, uses, lastUsed, unused, dup, change }],
//   duplicates: [{ why, names }], summary }
#[tauri::command]
async fn skills_audit(dir: Option<String>, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let d = match audit_dir(&b, dir) { Ok(d) => d, Err(e) => return e };
        let slash = b.core.lock().unwrap().settings["usageContent"] == true;
        skill_audit::audit(&paths::home(), d.as_deref(), skill_audit::now_ms(), slash)
    }).await.map_err(|_| ())
}

// act: disable | remove, on the skill whose `key` the audit gave. { ok, id } or { ok: false, error, plan? }.
#[tauri::command]
async fn skills_change(dir: Option<String>, key: String, act: String, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || match audit_dir(&b, dir) {
        Ok(d) => skill_audit::apply(&paths::home(), d.as_deref(), &paths::data_dir(), &key, &act),
        Err(e) => e,
    }).await.map_err(|_| ())
}

// The changes that can be undone, newest first: [{ id, kind: disable | remove | rules, name, source, path, at }].
#[tauri::command]
async fn skills_backups() -> Result<Value, ()> {
    tauri::async_runtime::spawn_blocking(|| skill_audit::backups(&paths::data_dir())).await.map_err(|_| ())
}

// Undoes one change. { ok, exact? } or { ok: false, error }.
#[tauri::command]
async fn skills_undo(id: String) -> Result<Value, ()> {
    tauri::async_runtime::spawn_blocking(move || skill_audit::undo(&paths::data_dir(), &id)).await.map_err(|_| ())
}

// The "When to split, which model" block of the project's CLAUDE.md: { file, present, text, block, undo } (add: false),
// or added now after a backup (add: true): { ok, id }.
#[tauri::command]
async fn skills_rules(dir: String, add: Option<bool>, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || match audit_dir(&b, Some(dir)) {
        Ok(Some(d)) if add == Some(true) => skill_audit::rules_add(&d, &paths::data_dir()),
        Ok(Some(d)) => skill_audit::rules_status(&d, &paths::data_dir()),
        Ok(None) => Value::Null,
        Err(e) => e,
    }).await.map_err(|_| ())
}

// The Skills tab, per project with a folder here: the process Bobix uses for it (the repository's .bobix/skills.json,
// or why it is not valid, and the one imported by hand), the project's and team's skills in that folder, and the
// skills the process names that Claude doesn't have there (names only).
#[tauri::command]
async fn process_status(b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (view, imported) = {
            let c = b.core.lock().unwrap();
            (c.work.view(&c.settings, &model::Model::build(&c.settings, &c.sessions, &c.work)), c.settings["skillProcess"].clone())
        };
        let list: Vec<Value> = view["local"].as_array().into_iter().flatten().map(|g| {
            let (path, dir) = (g["path"].as_str().unwrap_or(""), g["dir"].as_str().unwrap_or(""));
            let (repo, error) = match process::from_repo(std::path::Path::new(dir)) { Some(Ok(p)) => (p, Value::Null), Some(Err(e)) => (Value::Null, json!(e)), None => (Value::Null, Value::Null) };
            let imported = imported.get(path).cloned().unwrap_or(Value::Null);
            let all = skills::list(&paths::home(), std::path::Path::new(dir));
            let used = if repo.is_null() { &imported } else { &repo };
            let mut missing: Vec<&str> = vec![];
            for r in used["rules"].as_array().into_iter().flatten().filter_map(|r| r["skill"].as_str()) {
                if !all.iter().any(|s| s.name == r) && !missing.contains(&r) { missing.push(r); }
            }
            let team: Vec<&skills::Skill> = all.iter().filter(|s| matches!(s.source, "project" | "team" | "managed")).collect();
            json!({ "path": path, "dir": dir, "repo": repo, "repoError": error, "imported": imported, "skills": team, "missing": missing })
        }).collect();
        json!(list)
    }).await.map_err(|_| ())
}

// Imports a process for a project, from the text of a file you chose or from an https URL; checked strictly and kept
// in Bobix's settings only. { ok } or { ok: false, error }. remove: forgets the imported one.
#[tauri::command]
async fn process_import(project: String, text: Option<String>, url: Option<String>, remove: Option<bool>, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut all = b.core.lock().unwrap().settings["skillProcess"].as_object().cloned().unwrap_or_default();
        if remove == Some(true) { all.remove(&project); }
        else {
            let raw = match (text, url) { (Some(t), _) => Ok(t), (None, Some(u)) => process::fetch(&u), _ => Err("nothing to import".into()) };
            match raw.and_then(|r| process::parse(&r)) {
                Ok(p) => { all.insert(project.clone(), p); }
                Err(e) => return json!({ "ok": false, "error": e }),
            }
        }
        let next = update_settings(&b, json!({ "skillProcess": all }));
        let kept = next["skillProcess"].get(project.as_str()).is_some() != (remove == Some(true));
        if kept { json!({ "ok": true }) } else { json!({ "ok": false, "error": "not a project of yours" }) }
    }).await.map_err(|_| ())
}

// For the Start box: an issue Bobix listed, with its description read now (one GET through glab / gh, not kept), its
// labels, and the skills Claude Code has for the project's folder (names and descriptions, read-only, see skills.rs).
// { ok, title, url, body, labels, skills } or { ok: false, title, url, labels, skills, reason } (the box still opens,
// without the description).
#[tauri::command]
async fn work_brief(path: String, iid: i64, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (src, dir) = {
            let c = b.core.lock().unwrap();
            let view = c.work.view(&c.settings, &model::Model::build(&c.settings, &c.sessions, &c.work));
            let dir = view["local"].as_array().and_then(|l| l.iter().find(|g| g["path"] == path.as_str())).and_then(|g| g["dir"].as_str().map(String::from));
            (c.work.source_of(&c.settings, &path, iid), dir)
        };
        let Some((f, bin, host, i)) = src else { return json!({ "ok": false, "reason": "gone" }) };
        let skills = dir.as_deref().map(|d| skills::list(&paths::home(), std::path::Path::new(d))).unwrap_or_default();
        // The team's process: the repository's .bobix/skills.json, else one imported in Settings for this project.
        let repo = dir.as_deref().and_then(|d| process::from_repo(std::path::Path::new(d)));
        let imported = b.core.lock().unwrap().settings["skillProcess"].get(path.as_str()).cloned();
        let (proc_, proc_err) = match (repo, imported) {
            (Some(Ok(p)), _) => (json!({ "source": "repo", "process": p }), Value::Null),
            (Some(Err(e)), Some(p)) => (json!({ "source": "imported", "process": p }), json!(e)),
            (Some(Err(e)), None) => (Value::Null, json!(e)),
            (None, Some(p)) => (json!({ "source": "imported", "process": p }), Value::Null),
            (None, None) => (Value::Null, Value::Null),
        };
        let mut r = match work::fetch_body(f, &bin, &host, &path, iid) {
            Ok(body) => json!({ "ok": true, "body": body }),
            Err(e) => json!({ "ok": false, "reason": e["reason"] }),
        };
        for (k, v) in [("title", i["title"].clone()), ("url", i["url"].clone()), ("labels", i["labels"].clone()), ("skills", json!(skills)), ("process", proc_), ("processError", proc_err)] { r[k] = v; }
        r
    }).await.map_err(|_| ())
}

#[tauri::command]
fn rects(r: Vec<Vec<f64>>, b: B, app: AppHandle) {
    let r: Vec<[f64; 4]> = r.into_iter().filter(|x| x.len() == 4 && x.iter().all(|v| v.is_finite())).map(|x| [x[0], x[1], x[2], x[3]]).collect();
    b.core.lock().unwrap().rects = r;
    overlay::shape(&app, &b);
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

// ---------- a project's own pet picture ----------
// Chosen in the panel from a file dialog run here (pages never get file access). The pixel art runs in the panel's own
// worker (src/ui/pet-import.js, pixelate.js) with options the person adjusts in a preview; only Save writes anything.
// The original is kept next to the result, so the picture can be adjusted again later (petstore).
fn project_of(b: &Bobix, id: &str) -> Option<String> {
    b.core.lock().unwrap().sessions.get(id).and_then(|s| s.project.clone()).filter(|p| !p.is_empty())
}

#[tauri::command]
async fn pet_choose(id: String, b: B<'_>, app: AppHandle) -> Result<Value, ()> {
    use base64::Engine;
    use tauri_plugin_dialog::DialogExt;
    let Some(project) = project_of(&b, &id) else { return Ok(json!({ "ok": false })) };
    let l = lang(&b.core.lock().unwrap().settings);
    let (title, images) = (i18n::t(&l, "pane.petPick", &[]), i18n::t(&l, "pane.petPick.images", &[]));
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = app.dialog().file().set_title(title).add_filter(&images, &["png", "jpg", "jpeg", "gif", "webp"]);
        if let Some(w) = app.get_webview_window(panel::LABEL) { d = d.set_parent(&w); }
        let Some(path) = d.blocking_pick_file().and_then(|f| f.into_path().ok()) else { return json!({ "ok": false }) };
        let bytes = match std::fs::metadata(&path) {
            Ok(m) if m.len() as usize > petstore::MAX_PICTURE => return json!({ "ok": false, "reason": "big" }),
            Ok(_) => std::fs::read(&path).ok(),
            Err(_) => None, // gone or unreadable since the dialog closed
        };
        match bytes.as_deref().and_then(|x| petstore::kind_of(x).map(|k| (x, k))) {
            Some((x, kind)) => json!({ "ok": true, "project": project, "kind": kind, "data": base64::engine::general_purpose::STANDARD.encode(x) }),
            None => json!({ "ok": false, "reason": "bad" }),
        }
    }).await.map_err(|_| ())
}

// The stored original of a project's picture, to adjust it again.
#[tauri::command]
fn pet_original(id: String, b: B) -> Value {
    use base64::Engine;
    let Some(project) = project_of(&b, &id) else { return json!({ "ok": false, "reason": "gone" }) };
    let entry = b.core.lock().unwrap().settings["petImages"][&project].clone();
    match petstore::read_original(&petstore::dir(), &entry) {
        Some((x, opts)) => json!({ "ok": true, "project": project, "kind": petstore::kind_of(&x), "data": base64::engine::general_purpose::STANDARD.encode(&x), "opts": opts }),
        None => json!({ "ok": false, "reason": "gone" }),
    }
}

// Save: the result the panel made, checked (a PNG / GIF of pixel-art size) before anything is written. Settings are
// saved before the old files go: settings on disk still naming them would show a lost picture after a crash.
#[tauri::command]
async fn pet_save(id: String, project: String, original: String, result: String, ext: String, opts: Value, b: B<'_>) -> Result<Value, ()> {
    let b = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine;
        if project_of(&b, &id).as_deref() != Some(project.as_str()) { return json!({ "ok": false, "reason": "gone" }); }
        let dec = |s: &str| base64::engine::general_purpose::STANDARD.decode(s).ok();
        let (Some(original), Some(result)) = (dec(&original), dec(&result)) else { return json!({ "ok": false, "reason": "save" }) };
        let kind = petstore::kind_of(&original);
        if original.len() > petstore::MAX_PICTURE || kind.is_none() || !petstore::result_ok(&result, &ext) { return json!({ "ok": false, "reason": "save" }); }
        let (images, old) = { let c = b.core.lock().unwrap(); (c.settings["petImages"].clone(), c.settings["petImages"][&project].clone()) };
        let images = match petstore::save_import(&petstore::dir(), &images, &project, &original, kind.unwrap(), &result, &ext, &opts) {
            Ok(i) => i,
            Err(e) => { b.log_err(format!("pet picture: {e}")); return json!({ "ok": false, "reason": "save" }); }
        };
        let saved = update_settings(&b, json!({ "petImages": images }));
        if !saved["petImages"][&project].is_object() { return json!({ "ok": false, "reason": "save" }); }
        save_now(&b);
        petstore::release(&petstore::dir(), &saved["petImages"], &old);
        b.broadcast();
        json!({ "ok": true })
    }).await.map_err(|_| ())
}

// Default pet: the project's picture entry goes, and its files unless another project uses them.
#[tauri::command]
fn pet_reset(id: String, b: B) -> Value {
    let Some(project) = project_of(&b, &id) else { return json!({ "ok": false }) };
    let (mut rest, old) = { let c = b.core.lock().unwrap(); (c.settings["petImages"].clone(), c.settings["petImages"][&project].clone()) };
    if old.is_null() { return json!({ "ok": false }); }
    rest.as_object_mut().map(|o| o.shift_remove(&project));
    let saved = update_settings(&b, json!({ "petImages": rest }));
    save_now(&b);
    petstore::release(&petstore::dir(), &saved["petImages"], &old);
    b.broadcast();
    json!({ "ok": true })
}

pub const COMMANDS: &[&str] = &["state", "rects", "pet_click", "pet_set", "answer", "test_event", "page_error", "smoke_page", "install_hooks", "uninstall_hooks",
    "open_session", "settings_set", "settings_reset", "reveal_settings", "diagnostics_copy", "feedback_info", "feedback_send", "work_refresh", "work_open", "work_cancel", "work_start",
    "work_brief", "work_new_review", "skills_mine", "skills_audit", "skills_change", "skills_backups", "skills_undo", "skills_rules", "process_status", "process_import", "pet_choose", "pet_original", "pet_save", "pet_reset", "habits_read", "habits_clear", "habits_note", "show_board", "usage_prices",
    "ticket_prep", "ticket_start", "ticket_classify", "ticket_answer", "ticket_seen", "ticket_summary", "ticket_clean"];

pub fn install(b: &Bobix) -> Value { install_at(b, b.port) }

fn install_at(b: &Bobix, port: u16) -> Value {
    let node = b.core.lock().unwrap().node_path.clone();
    match hooks::install(port, node.as_deref(), &paths::home()) {
        Ok(r) => json!({ "file": r.file, "backup": r.backup, "mode": r.mode }),
        Err(e) => json!({ "error": e }),
    }
}

// Linux on Wayland (the session type, as the 0.1 Electron app read it).
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
    // Linux on Wayland: apps may not keep a window on top or position it, so the pets would sit behind whatever you work
    // in. Draw through XWayland (X11) when it is there, set before GTK starts; BOBIX_WAYLAND=1 keeps native Wayland.
    #[cfg(target_os = "linux")]
    if wayland() && std::env::var_os("DISPLAY").is_some() && std::env::var("BOBIX_WAYLAND").as_deref() != Ok("1") && std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    let file = paths::settings_file();
    autostart::migrate_from_old_name(&file);
    let mut settings = settings::load(&file);
    // Start at login: point at this copy, not one from an old path. Run from a clone (a dev build), a missing login entry
    // of its own doesn't mean "off": the installed app shares this settings file and owns the setting, so leave it as stored.
    // The installed app takes over the Electron build's start at login at its first start (the user does nothing).
    let mark = paths::data_dir().join("tauri-started");
    if !tauri::is_dev() && !mark.exists() {
        if autostart::from_electron() { eprintln!("[bobix] autostart: moved over from the Electron build"); }
        let _ = std::fs::create_dir_all(paths::data_dir()).and_then(|_| std::fs::write(&mark, ""));
    }
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
            node_path: None, work: Default::default(), update: None, update_told: None, main_errors: vec![], save_gen: 0, waits: vec![], habits: Default::default(), tickets: tickets::Flows::load(paths::data_dir().join("tickets.json")), usage: Value::Null }),
        tx, token: token.clone(), port, app: Default::default(), usage: Mutex::new(usage::Store::new(paths::data_dir())),
    });

    let rb = b.clone();
    tauri::Builder::default()
        .manage(b.clone())
        .invoke_handler(tauri::generate_handler![state, rects, pet_click, pet_set, answer, test_event, page_error, smoke_page, install_hooks, uninstall_hooks, open_session,
            settings_set, settings_reset, reveal_settings, diagnostics_copy, feedback_info, feedback_send, work_refresh, work_open, work_cancel, work_start, work_brief, work_new_review, skills_mine, skills_audit, skills_change, skills_backups, skills_undo, skills_rules, habit_skills, room, process_status, process_import, habits_read, habits_clear, habits_note, show_board, usage_prices,
            ticket_prep, ticket_start, ticket_classify, ticket_answer, ticket_seen, ticket_summary, ticket_clean,
            pet_choose, pet_original, pet_save, pet_reset])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // no Dock icon, like LSUIElement
            let handle = app.handle().clone();
            let _ = b.app.set(handle.clone());
            let version = app.package_info().version.to_string();
            let first = { let c = b.core.lock().unwrap(); settings::first_start(&c.settings, &version, has_hooks(), &i18n::TOUR) };
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
                pet_dir: Some(petstore::dir()),
            });
            if let Err(e) = server::start(port, ctx) {
                // One Bobix at a time: the running one shows its panel (and hears this copy's version).
                if server::show_running(port, &token, &version) { std::process::exit(0); }
                fail(format!("Port {port} is busy ({e}). Is Bobix already running?\nChange \"port\" in {}", paths::settings_file().display()));
            }

            thread::spawn(|| { room_serve(&paths::data_dir()); }); // a room still open from before
            let hb = b.clone();
            thread::spawn(move || { let node = find_bin("node"); hb.core.lock().unwrap().node_path = node; refresh_hooks(&hb); });
            overlay::create(&handle, &b)?;
            tray::build(&handle, &b);
            overlay::track_mouse(handle.clone(), b.clone());
            let tb = b.clone();
            thread::spawn(move || loop { thread::sleep(Duration::from_secs(5)); tick(&tb); });
            let lb = b.clone();
            thread::spawn(move || loop { thread::sleep(Duration::from_millis(400)); link_tick(&lb); });
            let ub = b.clone(); // every 5 s: what each session's transcript added (usage.rs)
            thread::spawn(move || loop { usage_tick(&ub); thread::sleep(Duration::from_secs(5)); });
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
        .plugin(tauri_plugin_dialog::init())
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

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use serde_json::json;

    // Every way a session, an issue and a PR / MR link up, as the pages get it. The JSON the panel and the pets read is
    // kept in test/fixtures/work-snapshot.json; BOBIX_WRITE_SNAPSHOT=1 writes it again.
    pub fn fixture() -> Core {
        let s = |id: &str, cwd: &str, git: Option<(&str, &str)>| state::Session {
            id: id.into(), name: cwd.rsplit('/').next().unwrap().into(), cwd: cwd.into(), state: "working".into(), since: 1000, updated: 2000, lv: 1, pet: "auto".into(),
            project: Some(git.map_or(cwd, |g| g.0).into()), git: git.map(|(top, b)| json!({ "branch": b, "top": top, "upstream": null, "ahead": 0, "behind": 0, "changed": 0 })),
            host_session: Some(format!("local_{id}")), ..Default::default()
        };
        let mut sessions = Sessions::new();
        let mut add = |x: state::Session| { sessions.insert(x.id.clone(), x); };
        add(s("s1", "/src/shop", Some(("/src/shop", "29-login")))); // its branch names #29
        add(state::Session { branches: vec!["30-cart".into()], ..s("s1b", "/src/shop", Some(("/src/shop", "main"))) }); // a branch it worked on names #30
        add(state::Session { started: Some(json!({ "path": "acme/app", "iid": 54 })), ..s("s2", "/src/app", Some(("/src/app", "main"))) }); // New on #54
        add(state::Session { state: "waiting".into(), ..s("p4", "/src/app", Some(("/src/app", "feat/52-board"))) });
        add(state::Session { parent_host: Some("local_p4".into()), ..s("c3", "/src/app/sub", Some(("/src/app", "claude/otter"))) }); // side session: its parent's issue
        add(state::Session { parent_host: Some("local_p4".into()), ..s("c5", "/tmp/scratch", None) }); // side session outside git
        add(state::Session { parent_host: Some("local_gone".into()), ..s("o6", "/src/other", Some(("/src/other", "7-x"))) }); // remote on no signed-in host
        add(state::Session { started: Some(json!({ "path": "acme/app", "iid": 99 })), state: "done".into(), ..s("s7", "/src/shop", Some(("/src/shop", "main"))) });
        add(state::Session { host_session: None, state: "idle".into(), ..s("cli", "/src/app", Some(("/src/app", "55-cli"))) }); // a terminal session
        let issue = |p: &str, iid: i64, due: Value| json!({ "iid": iid, "title": format!("t{iid}"), "url": format!("https://h/{p}/issues/{iid}"), "project": p, "labels": [], "due": due });
        let mr = json!({ "iid": 5, "web_url": "https://gitlab.example.com/team/shop/-/merge_requests/5", "references": { "full": "Team/Shop!5" }, "source_branch": "30-cart", "description": "Closes #31" });
        let pr: Value = serde_json::from_str(r#"{ "number": 60, "url": "https://github.com/Acme/App/pull/60", "headRefName": "feat/52-board", "body": "Fixes #53",
            "repository": { "nameWithOwner": "Acme/App" }, "closingIssuesReferences": { "nodes": [{ "number": 55, "repository": { "nameWithOwner": "Acme/App" } }] } }"#).unwrap();
        let closed = |p: &str, iid: i64, at: &str| json!({ "iid": iid, "title": format!("t{iid}"), "url": format!("https://h/{p}/issues/{iid}"), "project": p, "labels": [], "due": null, "closedAt": at });
        let mut work = work::Work::default();
        work.feeds[work::Forge::GitLab as usize].issues = json!({ "status": "ok", "hosts": ["gitlab.example.com"], "at": 100, "failed": [],
            "list": [issue("team/shop", 29, json!("2026-11-20")), issue("team/shop", 30, Value::Null), issue("team/shop", 31, Value::Null), issue("team/shop", 32, json!("2026-11-01"))],
            "closed": [closed("team/shop", 28, "2026-11-09T02:00:00Z")] });
        work.feeds[work::Forge::GitHub as usize].issues = json!({ "status": "ok", "hosts": ["github.com"], "at": 200, "failed": [{ "host": "ghe.example.com", "detail": "x" }],
            "list": [issue("acme/app", 52, Value::Null), issue("acme/app", 53, Value::Null), issue("acme/app", 54, Value::Null), issue("acme/app", 55, Value::Null), issue("acme/away", 1, Value::Null)],
            "closed": [closed("acme/app", 50, "2026-11-09T01:00:00Z"), closed("acme/app", 50, "2026-11-09T03:00:00Z"), closed("acme/new", 9, "2026-11-08T03:00:00Z")] });
        work.feeds[work::Forge::GitLab as usize].reviews = vec![model::gl_mr(&mr).unwrap()];
        work.feeds[work::Forge::GitHub as usize].reviews = vec![model::gh_pr(&pr).unwrap()];
        for (top, url) in [("/src/shop", "git@gitlab.example.com:Team/Shop.git"), ("/src/app", "https://github.com/Acme/App.git"), ("/src/other", "git@gitlab.other.org:x/y.git")] {
            work.remotes_of.insert(top.into(), vec![url.into()]);
        }
        work.claude_list = vec![
            links::ClaudeSession { id: "local_r1".into(), cwd: "/src/shop".into(), title: "[SHO-32] tidy".into(), at: 5.0 },
            links::ClaudeSession { id: "local_r2".into(), cwd: "/src/shop".into(), title: "shop #31 old".into(), at: 3.0 },
        ];
        work.new_for.insert("/src/shop".into(), json!({ "path": "team/shop", "iid": 32, "title": "t32" }));
        Core { sessions, settings: json!({ "gitlab": true, "github": true, "workKeys": { "acme/app": "APP" }, "petImages": {} }), rects: vec![], interactive: false,
            last_event_at: None, page_errors: vec![], pending: Default::default(), node_path: None, work, update: None, update_told: None, main_errors: vec![], save_gen: 0, waits: vec![], habits: Default::default(), tickets: Default::default(), usage: Value::Null }
    }

    const SNAPSHOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test/fixtures/work-snapshot.json");

    // Before the work model, the pages got exactly test/fixtures/work-snapshot.json for this fixture. Now they get the
    // same, plus two things nothing read before: each issue's "stage" and the model itself ("model").
    #[test]
    fn pages_get_the_same_json() {
        let c = fixture();
        let mut got = snapshot_of(&c);
        let added = got.as_object_mut().unwrap().shift_remove("model").expect("the model goes along");
        // the ticket flow's view (free disk differs per machine) and each session's flow and mark: none in this fixture
        let flows = got.as_object_mut().unwrap().shift_remove("tickets").expect("the ticket flow goes along");
        assert_eq!((&flows["list"], &flows["need"]), (&json!([]), &json!(tickets::WORKTREE_BYTES)));
        for s in got["sessions"].as_array_mut().unwrap() {
            let o = s.as_object_mut().unwrap();
            assert_eq!((o.shift_remove("mark"), o.shift_remove("flow")), (Some(json!(false)), Some(Value::Null)));
        }
        let mut stages = vec![];
        for side in ["local", "away"] {
            for i in got["work"][side].as_array_mut().unwrap().iter_mut().flat_map(|g| g["issues"].as_array_mut().unwrap()) {
                // a session on it waiting for you (ask) or working (doing), open PR / MR or not; else an open PR / MR
                // (verify); else a live session (doing); else todo
                let st: Vec<&str> = i["sessions"].as_array().unwrap().iter().map(|id| c.sessions[id.as_str().unwrap()].state.as_str()).collect();
                let review = i.get("review").is_some_and(|r| r.as_str().is_some_and(|r| !r.is_empty()));
                let was = if st.contains(&"waiting") { "ask" } else if st.contains(&"working") { "doing" } else if review { "verify" } else if !st.is_empty() { "doing" } else { "todo" };
                assert_eq!(i["stage"], was, "#{}", i["iid"]); stages.push((i["iid"].as_i64().unwrap(), i.as_object_mut().unwrap().shift_remove("stage").unwrap())); }
        }
        if std::env::var_os("BOBIX_WRITE_SNAPSHOT").is_some() { std::fs::write(SNAPSHOT, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap(); }
        let want: Value = serde_json::from_str(&std::fs::read_to_string(SNAPSHOT).unwrap()).unwrap();
        assert_eq!(serde_json::to_string_pretty(&got).unwrap(), serde_json::to_string_pretty(&want).unwrap(), "same JSON, same key order");
        let stage = |n: i64| stages.iter().find(|x| x.0 == n).unwrap().1.as_str().unwrap();
        // a session still waiting or working on it keeps an issue out of review, open PR / MR or not (#52 waits: ask,
        // #30 works: doing); an open PR / MR with its session resting is in review (#55); then a live session (#54, #29)
        assert_eq!([52, 55, 54, 53, 29, 30, 32, 31, 1].map(stage), ["ask", "verify", "doing", "verify", "doing", "doing", "todo", "verify", "todo"]);
        assert_eq!(added, model::Model::build(&fixture().settings, &fixture().sessions, &fixture().work).json());
    }

    // Each link of the fixture, and why it is there.
    #[test]
    fn the_model_says_why() {
        use model::Why::*;
        let c = fixture();
        let m = model::Model::build(&c.settings, &c.sessions, &c.work);
        let s = |id: &str| m.session(id).unwrap();
        let issue = |id: &str| s(id).issue.as_ref().map(|i| (i.to.project.as_str(), i.to.iid, i.why));
        assert_eq!(issue("s1"), Some(("team/shop", 29, Branch)), "the branch it is on");
        assert_eq!(issue("s1b"), Some(("team/shop", 30, Branch)), "a branch from its transcript");
        assert_eq!(issue("s2"), Some(("acme/app", 54, Started)), "New on #54");
        assert_eq!(issue("c3"), Some(("acme/app", 52, Parent)), "a side session on Claude's own branch: its parent's");
        assert_eq!(issue("cli"), Some(("acme/app", 55, Branch)));
        assert_eq!([issue("c5"), issue("o6"), issue("s7")], [None, None, None], "outside git; remote on no signed-in host; started in another project");
        let parent = |id: &str| s(id).parent.as_ref().map(|p| (p.to.id.as_str(), p.why));
        assert_eq!([parent("c3"), parent("c5"), parent("o6"), parent("p4")], [Some(("p4", Parent)), Some(("p4", Parent)), None, None]);
        assert_eq!(s("c3").dir.as_ref().map(|d| (d.to.path.as_str(), d.why)), Some(("/src/app", Folder)), "the repository's folder, not the one it cd'd into");
        assert_eq!(s("s1").repo.as_ref().map(|r| (r.to.path.as_str(), r.why)), Some(("team/shop", Remote)));
        assert!(s("c5").dir.is_none() && s("o6").repo.is_none());
        assert_eq!(s("p4").approvals, [model::Approval { tool: String::new(), ask: String::new(), since: 1000, held: false }]);
        assert!(s("s1").approvals.is_empty() && s("s7").status == "done" && s("s1").last_activity == 2000);
        let repo = |p: &str| m.repos.iter().find(|r| r.path == p).map(|r| (r.forge, r.host.as_str(), r.key.as_str(), r.dirs.iter().map(|d| d.to.path.as_str()).collect::<Vec<_>>())).unwrap();
        assert_eq!(repo("team/shop"), ("gitlab", "gitlab.example.com", "SHO", vec!["/src/shop"]));
        assert_eq!(repo("acme/app"), ("github", "github.com", "APP", vec!["/src/app"]), "the code set in Settings");
        assert_eq!(repo("acme/away"), ("github", "h", "AWA", vec![]), "not on this machine: the host of its issues");
        let reviewed = |p: &str, n: i64| m.issue(p, n).unwrap().reviews.iter().map(|r| r.why).collect::<Vec<_>>();
        assert_eq!([reviewed("team/shop", 31), reviewed("team/shop", 30), reviewed("acme/app", 55), reviewed("acme/app", 53), reviewed("acme/app", 52)],
            [vec![Closes], vec![Branch], vec![ForgeLink], vec![Closes], vec![Branch]]);
        assert_eq!(m.issue("acme/app", 52).unwrap().sessions.iter().map(|x| (x.to.id.as_str(), x.why)).collect::<Vec<_>>(), [("p4", Branch), ("c3", Parent)]);
        let resume = |n: i64| m.issue("team/shop", n).unwrap().resume.as_ref().map(|r| (r.to.id.as_str(), r.why));
        assert_eq!([resume(31), resume(32), resume(29)], [Some(("local_r2", SessionName)), Some(("local_r1", SessionName)), None]);
        let branch = |p: &str, n: &str| m.branches.iter().find(|b| b.project == p && b.name == n).map(|b| (b.issue.as_ref().map(|i| i.to.iid), b.sessions.clone(), b.reviews.len())).unwrap();
        assert_eq!(branch("team/shop", "29-login"), (Some(29), vec!["s1".to_string()], 0));
        assert_eq!(branch("team/shop", "30-cart"), (Some(30), vec![], 1), "worked on before, and the MR's source branch");
        assert_eq!(branch("acme/app", "feat/52-board"), (Some(52), vec!["p4".to_string()], 1));
        assert_eq!(branch("acme/app", "claude/otter"), (None, vec!["c3".to_string()], 0));
        // stuck work: the forge's page for a new PR / MR, only for a project and a branch the model knows
        assert_eq!(work::new_review_url(&m, "acme/app", "claude/otter").as_deref(), Some("https://github.com/acme/app/compare/claude/otter?expand=1"));
        assert_eq!(work::new_review_url(&m, "team/shop", "29-login").as_deref(), Some("https://gitlab.example.com/team/shop/-/merge_requests/new?merge_request%5Bsource_branch%5D=29-login"));
        assert_eq!([work::new_review_url(&m, "team/shop", "evil?x=1"), work::new_review_url(&m, "acme/other", "main")], [None, None]);
        // trackers off: no project, issue or review links; side sessions still know their parent
        let off = model::Model::build(&json!({}), &c.sessions, &c.work);
        assert!(off.issues.is_empty() && off.repos.is_empty() && off.reviews.is_empty() && off.sessions.iter().all(|x| x.issue.is_none() && x.repo.is_none()));
        assert_eq!(off.session("c3").unwrap().parent.as_ref().map(|p| p.to.id.as_str()), Some("p4"));
    }
}
