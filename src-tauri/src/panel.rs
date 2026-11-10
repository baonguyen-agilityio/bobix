// The control panel window (showPanel in the 0.1 (Electron) app's main.js): one window, opened on a session, a tab or an "open failed"
// notice; when it is already open the page gets select / show-tab / open-fail instead. On macOS the Dock icon shows
// while it is open.
use crate::{work::enc, Bobix};
use serde_json::Value;
use std::sync::Arc;
use tauri::{webview::NewWindowResponse, window::Color, AppHandle, Manager, Theme, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "panel";

// Sends `v` to a page's window.bobix.on<Channel> listeners.
pub fn emit(app: &AppHandle, label: &str, ch: &str, v: &Value) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.eval(format!("window.__bobixEmit && __bobixEmit({}, {})", Value::from(ch), v));
    }
}

pub fn show(app: &AppHandle, b: &Arc<Bobix>, select: Option<String>, tab: Option<String>, fail: Option<Value>) {
    let (app2, b) = (app.clone(), b.clone());
    // Windows are made on the main thread (a window built inside a command can deadlock on Windows).
    let _ = app.run_on_main_thread(move || {
        let app = app2;
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        if let Some(w) = app.get_webview_window(LABEL) {
            if let Some(s) = &select { emit(&app, LABEL, "select", &Value::from(s.as_str())); }
            if let Some(t) = &tab { emit(&app, LABEL, "show-tab", &Value::from(t.as_str())); }
            if let Some(f) = &fail { emit(&app, LABEL, "open-fail", f); }
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            return;
        }
        let origin = format!("http://127.0.0.1:{}", b.port);
        let mut q = vec![];
        if let Some(s) = &select { q.push(format!("s={}", enc(s))); }
        if let Some(t) = &tab { q.push(format!("tab={}", enc(t))); }
        if let Some(f) = &fail { q.push(format!("fail={}", enc(&f.to_string()))); }
        q.push(format!("t={}", enc(&b.token))); // the page needs the token to read /events
        let Ok(url) = format!("{origin}/?{}", q.join("&")).parse() else { return };
        let dark = app.get_webview_window(crate::overlay::LABEL).and_then(|w| w.theme().ok()) == Some(Theme::Dark);
        let bg = if dark { Color(0x15, 0x17, 0x1a, 255) } else { Color(0xfd, 0xfe, 0xfe, 255) }; // the panel's window colour (--surface)
        let page = format!("{origin}/?");
        eprintln!("[bobix] panel: opening");
        let built = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(url))
            .title("Bobix").inner_size(1180.0, 780.0).min_inner_size(420.0, 480.0).background_color(bg)
            .initialization_script(crate::BRIDGE)
            .on_navigation(move |u| u.as_str().starts_with(&page)) // the panel itself only (its first load, a reload)
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .build();
        match built {
            Ok(w) => {
                #[cfg(target_os = "macos")]
                {
                    let app = app.clone();
                    w.on_window_event(move |e| if let tauri::WindowEvent::Destroyed = e { let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory); });
                }
                let _ = w.set_focus();
                eprintln!("[bobix] panel: open");
            }
            Err(e) => eprintln!("[bobix] panel: {e}"),
        }
    });
}
