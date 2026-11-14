// Updating Bobix from inside the app (#189). One signed manifest per channel on GitHub (stable: the latest release's
// `latest.json`; test builds: `test.json` of the `test-channel` release), read with Tauri's updater plugin. A newer version
// is only offered: nothing is downloaded until the person presses Update, and the plugin installs only a version greater
// than the running one, so there is no downgrade (also after the "Get test builds" switch goes on and off).
//
// The manifest is read the same way on every system. Where the app cannot update itself (no signing key in this build,
// no way to ask for the password on Linux, a read-only place on macOS) the version is still announced, with a link to its
// release page and the reason: today's notice.
use crate::{answers, i18n, notify, panel, tray, Bobix};
use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::{Error as UpErr, Update, UpdaterExt};

pub const REPO: &str = "baonguyen-agilityio/bobix";

// A newer version. `why`: the reason it cannot be installed from here (nokey | readonly | nopw); the page is then the way.
#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found { pub version: String, pub page: String, pub why: Option<String> }

// What the pages and the tray show. phase: idle | checking | downloading | confirm | installing.
// error: a code the pages turn into words (offline | download | signature | install | password | readonly | other).
#[derive(Clone, Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ui {
    pub found: Option<Found>,
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
    pub checked: u64,  // when the last check got an answer (ms since 1970); 0: none yet
    pub held: usize,   // confirm: permission requests held in Bobix right now
    pub flows: usize,  // confirm: ticket flows running
    pub no_list: bool, // the last answer: GitHub has no update list for this channel (not "nothing newer")
}

// The downloaded, verified file, kept while the person decides about the restart (confirm).
static READY: Mutex<Option<(Update, Vec<u8>)>> = Mutex::new(None);

pub fn page_url(version: &str) -> String { format!("https://github.com/{REPO}/releases/tag/v{version}") }
pub fn releases_page() -> String { format!("https://github.com/{REPO}/releases/latest") }

// The channel's manifest: tauri.conf.json lists the stable one first and the test builds' one second.
fn endpoint(cfg: &Value, test: bool) -> Option<String> {
    let e = cfg["endpoints"].as_array()?;
    e.get(usize::from(test)).or_else(|| e.first())?.as_str().map(String::from)
}

// A real public key, not the placeholder a build has before the owner gives one: minisign's text, base64 of "untrusted comment: …".
fn key_set(cfg: &Value) -> bool {
    use base64::{engine::general_purpose::STANDARD, Engine};
    cfg["pubkey"].as_str().and_then(|k| STANDARD.decode(k.trim()).ok()).is_some_and(|b| b.starts_with(b"untrusted comment:"))
}

fn config(app: &AppHandle) -> Value { app.config().plugins.0.get("updater").cloned().unwrap_or(Value::Null) }

// macOS: the app opened from a mounted disk image or from the place Gatekeeper copies a downloaded app to. Neither can be replaced.
#[cfg(target_os = "macos")]
fn read_only_place() -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(app) = exe.ancestors().nth(3).filter(|a| a.extension().is_some_and(|e| e == "app")) else { return false };
    let marked = { let s = app.to_string_lossy(); s.starts_with("/Volumes/") || s.contains("/AppTranslocation/") };
    let Ok(c) = std::ffi::CString::new(app.as_os_str().as_bytes()) else { return marked };
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    let ro = unsafe { libc::statfs(c.as_ptr(), &mut s) } == 0 && s.f_flags & libc::MNT_RDONLY as u32 != 0;
    marked || ro
}

// Linux: the package manager needs the password (the updater asks with pkexec, else zenity or kdialog); with none of them
// there is no way to ask from here.
#[cfg(target_os = "linux")]
fn can_ask_password() -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    ["pkexec", "zenity", "kdialog"].iter().any(|n| std::env::split_paths(&path).any(|d| d.join(n).is_file()))
}

// Why this copy cannot install an update by itself, if it cannot.
fn why_not(app: &AppHandle) -> Option<&'static str> {
    if !key_set(&config(app)) { return Some("nokey"); }
    #[cfg(target_os = "macos")]
    if read_only_place() { return Some("readonly"); }
    #[cfg(target_os = "linux")]
    if !can_ask_password() { return Some("nopw"); }
    None
}

// The plugin's error as the code the pages know. `stage`: what it was doing (offline: the check; download; install).
fn code_of(e: &UpErr, stage: &'static str) -> &'static str {
    match e {
        UpErr::Reqwest(_) | UpErr::Network(_) | UpErr::Http(_) => stage,
        UpErr::Minisign(_) | UpErr::SignatureUtf8(_) | UpErr::Base64(_) | UpErr::SignedVersionMismatch { .. } | UpErr::MissingSignedVersion => "signature",
        UpErr::AuthenticationFailed => "password",
        // Linux: pkexec declined or missing, then sudo with no terminal to ask in: the password was not given.
        UpErr::DebInstallFailed | UpErr::PackageInstallFailed if cfg!(target_os = "linux") => "password",
        UpErr::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied => "readonly",
        UpErr::Io(_) | UpErr::DebInstallFailed | UpErr::PackageInstallFailed | UpErr::TempDirNotOnSameMountPoint | UpErr::PreviousAppNotRestored(_) | UpErr::InvalidUpdaterFormat => "install",
        _ => "other",
    }
}

fn updater(b: &Arc<Bobix>, app: &AppHandle, test: bool) -> Result<tauri_plugin_updater::Updater, &'static str> {
    let url = endpoint(&config(app), test).and_then(|u| u.parse().ok()).ok_or("other")?;
    let sb = b.clone();
    app.updater_builder().endpoints(vec![url]).map_err(|_| "other")?
        .timeout(Duration::from_secs(30))
        .on_before_exit(move || crate::save_now(&sb)) // Windows: the installer ends Bobix without going through its exit
        .build().map_err(|_| "other")
}

// What the channel's list said: a newer version; nothing newer; or no list published (GitHub answered no: nothing is
// released on that channel yet, or the latest release has none). The last two are not the same thing to say to the person.
enum Seen { Newer(Update), Newest, NoList }

// Not reachable: Err.
async fn find(b: &Arc<Bobix>, app: &AppHandle, test: bool) -> Result<Seen, &'static str> {
    match updater(b, app, test)?.check().await {
        Ok(Some(u)) => Ok(Seen::Newer(u)),
        Ok(None) => Ok(Seen::Newest),
        Err(UpErr::ReleaseNotFound) => Ok(Seen::NoList),
        Err(e) => { b.log_err(format!("update check: {e}")); Err(code_of(&e, "offline")) }
    }
}

// Tells the tray and the panel what is shown now.
fn push(b: &Arc<Bobix>) {
    let ui = b.core.lock().unwrap().update.clone();
    if let Some(app) = b.app.get() {
        panel::emit(app, panel::LABEL, "update", &serde_json::to_value(&ui).unwrap_or(Value::Null));
        tray::build(app, b);
    }
}

// Changes the shown state under the lock (what `f` decides from it and what it sets are one step), then tells the tray and the panel.
fn set<R>(b: &Arc<Bobix>, f: impl FnOnce(&mut Ui) -> R) -> R {
    let r = f(&mut b.core.lock().unwrap().update);
    push(b);
    r
}

fn busy(ui: &Ui) -> bool { ui.phase != "idle" && !ui.phase.is_empty() }

// A check starts only when nothing else is going on; the test and the change of phase are one step, so two callers
// cannot both start.
fn begin_check(u: &mut Ui) -> bool {
    if busy(u) { return false; }
    u.phase = "checking".into();
    u.error = None;
    true
}

// What a press on Update starts. One press gets Download (or Install, at the confirmation); a second press, a click on the
// tray at the same time, finds the work started and gets No. Again one step: test and change of phase together.
#[derive(PartialEq, Debug)]
enum Claim { Download, Install, No }
fn claim(u: &mut Ui) -> Claim {
    match u.phase.as_str() {
        "" | "idle" => { u.phase = "downloading".into(); u.done = 0; u.total = 0; u.error = None; Claim::Download }
        "confirm" => { u.phase = "installing".into(); Claim::Install }
        _ => Claim::No,
    }
}

// Looks for a newer version. `manual`: the person pressed Check now (works with "Tell me about new versions" off, shows
// what went wrong); otherwise the start-up and daily check, silent unless something new turns up.
pub async fn check(b: &Arc<Bobix>, manual: bool) {
    let Some(app) = b.app.get().cloned() else { return };
    let test = {
        let mut c = b.core.lock().unwrap();
        if (!manual && c.settings["checkUpdates"] != true) || !begin_check(&mut c.update) { return; }
        c.settings["testBuilds"] == true
    };
    push(b);
    let res = find(b, &app, test).await;
    let why = why_not(&app);
    let news = set(b, |u| {
        u.phase = "idle".into();
        match res {
            Ok(Seen::Newer(f)) => {
                u.checked = crate::state::now();
                u.no_list = false;
                let found = Found { version: f.version.clone(), page: page_url(&f.version), why: why.map(String::from) };
                u.found = Some(found.clone());
                Some(found)
            }
            Ok(seen) => { u.checked = crate::state::now(); u.no_list = matches!(seen, Seen::NoList); u.found = None; None }
            Err(code) => { if manual { u.error = Some(code.to_string()); } None }
        }
    });
    // A notification once per version, only for the check Bobix ran on its own.
    let Some(f) = news else { return };
    let tell = { let mut c = b.core.lock().unwrap(); let t = !manual && c.update_told.as_deref() != Some(f.version.as_str()); if t { c.update_told = Some(f.version.clone()); } t };
    if tell { announce(b, &app, &f); }
}

fn announce(b: &Arc<Bobix>, app: &AppHandle, f: &Found) {
    let l = crate::lang(&b.core.lock().unwrap().settings);
    let current = app.package_info().version.to_string();
    let in_app = f.why.is_none();
    // macOS notifications have buttons: Update is one. Elsewhere a click opens Settings, where the Update button is.
    let buttons = (in_app && cfg!(target_os = "macos")).then(|| (i18n::t(&l, "update.btn", &[]), i18n::t(&l, "update.later", &[])));
    let note = notify::Note {
        title: i18n::t(&l, "update.title", &[("v", &f.version)]),
        body: i18n::t(&l, if in_app { "update.body.app" } else { "update.body" }, &[("v", &current)]),
        sound: true, buttons, open: i18n::t(&l, if in_app { "update.open" } else { "notify.open" }, &[]),
    };
    let (b, app, page) = (b.clone(), app.clone(), f.page.clone());
    notify::show(note, move |r| match r {
        notify::Reply::Allow => { tauri::async_runtime::spawn(install(b)); }
        notify::Reply::Click if in_app => panel::show(&app, &b, None, Some("update".into()), None),
        notify::Reply::Click => { let _ = tauri_plugin_opener::open_url(page, None::<&str>); }
        notify::Reply::Deny => {}
    });
}

// The person pressed Update (the panel, the tray or a notification): look again, download and check the file, then, when
// held requests or a ticket flow would be cut off by the restart, say so and wait for "Install and restart".
pub async fn install(b: Arc<Bobix>) {
    let Some(app) = b.app.get().cloned() else { return };
    let (claimed, test) = { let mut c = b.core.lock().unwrap(); (claim(&mut c.update), c.settings["testBuilds"] == true) };
    if claimed == Claim::No { return; }
    push(&b);
    if claimed == Claim::Install {
        let ready = READY.lock().unwrap().take();
        return match ready { Some((u, bytes)) => finish(b, app, u, bytes).await, None => set(&b, |u| u.phase = "idle".into()) };
    }
    let fail = |b: &Arc<Bobix>, app: &AppHandle, code: &str| {
        set(b, |u| { u.phase = "idle".into(); u.error = Some(code.into()); });
        panel::show(app, b, None, Some("update".into()), None); // the person pressed Update: show why it did not happen
    };
    if let Some(why) = why_not(&app) { return fail(&b, &app, if why == "nopw" { "password" } else { why }); }
    let u = match find(&b, &app, test).await {
        Ok(Seen::Newer(u)) => u,
        Ok(seen) => return set(&b, |u| { u.phase = "idle".into(); u.found = None; u.checked = crate::state::now(); u.no_list = matches!(seen, Seen::NoList); }),
        Err(code) => return fail(&b, &app, code),
    };
    let mut last = 0;
    let bytes = u.download(|got, total| {
        let mut c = b.core.lock().unwrap();
        c.update.done += got as u64;
        c.update.total = total.unwrap_or(0);
        let pct = if c.update.total > 0 { c.update.done * 100 / c.update.total } else { 0 };
        if pct != last {
            last = pct;
            let v = serde_json::to_value(&c.update).unwrap_or(Value::Null);
            drop(c);
            panel::emit(&app, panel::LABEL, "update", &v);
        }
    }, || {}).await;
    let bytes = match bytes { Ok(x) => x, Err(e) => { b.log_err(format!("update download: {e}")); return fail(&b, &app, code_of(&e, "download")) } };
    let (held, flows) = {
        let c = b.core.lock().unwrap();
        (c.pending.len(), running_flows(&c.tickets))
    };
    if held + flows == 0 { return finish(b, app, u, bytes).await; }
    *READY.lock().unwrap() = Some((u, bytes));
    set(&b, |x| { x.phase = "confirm".into(); x.held = held; x.flows = flows; });
    panel::show(&app, &b, None, Some("update".into()), None);
}

// Ticket flows in progress: their Claude sessions run on without Bobix, and Bobix picks them up after the restart.
fn running_flows(flows: &crate::tickets::Flows) -> usize {
    flows.list.values().filter(|f| !f.cleaned && ["starting", "planning", "approve", "running"].contains(&f.status.as_str())).count()
}

// "Not now" at the confirmation: the file is dropped; Update downloads it again. (The file goes while the phase changes, so
// a new press cannot have stored another one that this would drop.)
pub fn later(b: &Arc<Bobix>) {
    set(b, |u| if u.phase == "confirm" { u.phase = "idle".into(); *READY.lock().unwrap() = None; });
}

// Held requests go back to Claude's own prompt, then the new version goes in and Bobix starts again.
async fn finish(b: Arc<Bobix>, app: AppHandle, u: Update, bytes: Vec<u8>) {
    set(&b, |x| x.phase = "installing".into());
    let ids: Vec<String> = b.core.lock().unwrap().pending.keys().cloned().collect();
    for id in ids { answers::done(&b, &id, None); }
    crate::save_now(&b);
    match tauri::async_runtime::spawn_blocking(move || u.install(bytes)).await {
        Ok(Ok(())) => app.restart(),
        Ok(Err(e)) => {
            b.log_err(format!("update install: {e}"));
            set(&b, |x| { x.phase = "idle".into(); x.error = Some(code_of(&e, "install").into()); });
            panel::show(&app, &b, None, Some("update".into()), None);
        }
        Err(_) => set(&b, |x| { x.phase = "idle".into(); x.error = Some("other".into()); }),
    }
}

// What was offered or said goes (not while an update is under way).
fn clear(u: &mut Ui) -> bool {
    if busy(u) { return false; }
    u.found = None;
    u.error = None;
    u.no_list = false;
    true
}

// Settings → "Get test builds" changed: what was offered belongs to the other channel. Ask again when checks are on.
pub fn channel_changed(b: &Arc<Bobix>) {
    if !set(b, clear) { return; }
    let b = b.clone();
    tauri::async_runtime::spawn(async move { check(&b, false).await; });
}

// "Tell me about new versions" went off: what was announced goes.
pub fn forget(b: &Arc<Bobix>) { set(b, clear); }

pub fn page_of(b: &Arc<Bobix>) -> String {
    b.core.lock().unwrap().update.found.as_ref().map(|f| f.page.clone()).unwrap_or_else(releases_page)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn channel_endpoint() {
        let c = json!({ "endpoints": ["https://x/latest.json", "https://x/test.json"] });
        assert_eq!(endpoint(&c, false).as_deref(), Some("https://x/latest.json"));
        assert_eq!(endpoint(&c, true).as_deref(), Some("https://x/test.json"));
        assert_eq!(endpoint(&json!({ "endpoints": ["https://x/latest.json"] }), true).as_deref(), Some("https://x/latest.json"), "one endpoint serves both");
        assert_eq!(endpoint(&json!({}), false), None);
    }

    #[test]
    fn placeholder_key_is_not_a_key() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        assert!(!key_set(&json!({ "pubkey": "REPLACE_WITH_THE_PUBLIC_KEY" })));
        assert!(!key_set(&json!({})));
        assert!(key_set(&json!({ "pubkey": STANDARD.encode("untrusted comment: minisign public key: AB12\nRWQ=\n") })));
    }

    #[test]
    fn errors_become_codes() {
        let io = |k| UpErr::Io(std::io::Error::from(k));
        assert_eq!(code_of(&UpErr::Network("x".into()), "download"), "download");
        assert_eq!(code_of(&UpErr::Network("x".into()), "offline"), "offline");
        assert_eq!(code_of(&UpErr::SignatureUtf8("x".into()), "download"), "signature");
        assert_eq!(code_of(&UpErr::MissingSignedVersion, "download"), "signature");
        assert_eq!(code_of(&UpErr::AuthenticationFailed, "install"), "password");
        assert_eq!(code_of(&io(std::io::ErrorKind::PermissionDenied), "install"), "readonly");
        assert_eq!(code_of(&io(std::io::ErrorKind::NotFound), "install"), "install");
        assert_eq!(code_of(&UpErr::DebInstallFailed, "install"), if cfg!(target_os = "linux") { "password" } else { "install" });
    }

    // Two presses (a double click, the tray and a notification together) start one download; the rest find it started.
    #[test]
    fn two_presses_start_one_download() {
        let mut u = Ui::default(); // nothing yet: "" is idle
        assert_eq!(claim(&mut u), Claim::Download);
        assert_eq!(u.phase, "downloading");
        assert_eq!(claim(&mut u), Claim::No);
        assert!(!begin_check(&mut u), "no check while an update is under way");
        u.phase = "confirm".into();
        assert_eq!(claim(&mut u), Claim::Install, "the confirmation is answered once");
        assert_eq!(claim(&mut u), Claim::No);
        u.phase = "idle".into();
        assert!(begin_check(&mut u) && !begin_check(&mut u), "two checks start one");
        assert_eq!(claim(&mut u), Claim::No, "no download while checking");
        assert!(!clear(&mut u), "nothing is cleared under the work's feet");
    }

    #[test]
    fn only_flows_in_progress_count() {
        let mut flows = crate::tickets::Flows::default();
        for (k, status, cleaned) in [("a", "running", false), ("b", "approve", false), ("c", "done", false), ("d", "blocked", false), ("e", "running", true), ("f", "starting", false)] {
            flows.list.insert(k.into(), crate::tickets::Flow { key: k.into(), status: status.into(), cleaned, ..Default::default() });
        }
        assert_eq!(running_flows(&flows), 3);
    }

    #[test]
    fn pages_are_the_projects_own() {
        assert_eq!(page_url("0.2.0-rc.8"), format!("https://github.com/{REPO}/releases/tag/v0.2.0-rc.8"));
        assert!(releases_page().starts_with(&format!("https://github.com/{REPO}/releases")));
    }

    // The plugin offers a version only when it is greater: a test build is never a step back from a release, and the
    // release is a step up from its own test builds.
    #[test]
    fn never_a_downgrade() {
        use semver::Version;
        let v = |s: &str| Version::parse(s).unwrap();
        assert!(v("0.2.0") > v("0.2.0-rc.9"));
        assert!(v("0.2.0-rc.10") > v("0.2.0-rc.9"));
        assert!(!(v("0.1.4") > v("0.2.0-rc.7")), "a stable manifest older than the installed test build offers nothing");
        assert!(!(v("0.2.0-rc.7") > v("0.2.0-rc.7")));
    }
}
