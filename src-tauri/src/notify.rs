// Desktop notifications: "<name> needs you" and "<name> finished". A click opens the session; on macOS a held
// permission request also gets Allow / Deny. Port of notify() in the 0.1 (Electron) app's main.js (Electron's Notification).
// Each OS's own notification API: the notification plugin has no click or buttons on desktop.

pub struct Note {
    pub title: String,
    pub body: String,
    pub sound: bool,
    pub buttons: Option<(String, String)>, // (Allow, Deny) labels
}

pub enum Reply { Click, Allow, Deny }

// Shows the notification and calls `then` with what the person did (never, if they did nothing). Runs on its own
// thread: the platform calls block until the notification is answered or closed.
pub fn show(note: Note, then: impl FnOnce(Reply) + Send + 'static) {
    std::thread::spawn(move || {
        if let Some(r) = show_blocking(&note) { then(r); }
    });
}

#[cfg(target_os = "macos")]
fn show_blocking(n: &Note) -> Option<Reply> {
    use mac_notification_sys::{MainButton, Notification, NotificationResponse};
    static APP: std::sync::Once = std::sync::Once::new();
    APP.call_once(|| { let _ = mac_notification_sys::set_application("dev.bobix.app"); }); // the app's icon and name
    let labels = n.buttons.as_ref().map(|(a, d)| [a.as_str(), d.as_str()]);
    let menu = n.buttons.as_ref().map(|(a, d)| format!("{a} / {d}"));
    let mut x = Notification::new();
    x.title(&n.title).message(&n.body).wait_for_click(true);
    if n.sound { x.default_sound(); }
    if let (Some(l), Some(m)) = (labels.as_ref(), menu.as_deref()) { x.main_button(MainButton::DropdownActions(m, l)); }
    match x.send().ok()? {
        NotificationResponse::Click => Some(Reply::Click),
        NotificationResponse::ActionButton(b) => n.buttons.as_ref().map(|(a, _)| if &b == a { Reply::Allow } else { Reply::Deny }),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn show_blocking(n: &Note) -> Option<Reply> {
    use notify_rust::{Hint, Notification};
    let h = Notification::new().appname("Bobix").summary(&n.title).body(&n.body).action("default", "Open")
        .hint(Hint::SuppressSound(!n.sound)).show().ok()?;
    let mut out = None;
    h.wait_for_action(|a| if a == "default" { out = Some(Reply::Click) });
    out
}

#[cfg(windows)]
fn show_blocking(n: &Note) -> Option<Reply> {
    use tauri_winrt_notification::{Sound, Toast};
    let (tx, rx) = std::sync::mpsc::channel();
    // The installer's Start menu shortcut carries the app id (the identifier), so the toast says "Bobix"; a dev build
    // has no shortcut and borrows PowerShell's.
    Toast::new(if tauri::is_dev() { Toast::POWERSHELL_APP_ID } else { "dev.bobix.app" }).title(&n.title).text1(&n.body).sound(n.sound.then_some(Sound::Default))
        .on_activated(move |_| -> tauri_winrt_notification::Result<()> { let _ = tx.send(()); Ok(()) })
        .show().ok()?;
    // A toast that is never clicked: give up after a day, like a held request.
    rx.recv_timeout(std::time::Duration::from_secs(86400)).ok().map(|_| Reply::Click)
}
