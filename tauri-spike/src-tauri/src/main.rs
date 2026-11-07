// Bobix Tauri spike: only the pets overlay. A transparent, frameless, always-on-top strip along the bottom of the
// primary screen that loads the real pets page from a running Bobix (port 4747 by default, BOBIX_PORT to change)
// and lets the mouse through everywhere except over a pet, the way src/main.js does it in Electron.
// It never writes to Bobix: pet clicks, hide and answer buttons only log here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, sync::Mutex, thread, time::Duration};
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

// Pets' rects in CSS px relative to the window, sent by the page about 10 times a second.
struct Rects(Mutex<Vec<[f64; 4]>>);

#[tauri::command]
fn rects(r: Vec<[f64; 4]>, state: tauri::State<Rects>) {
    *state.0.lock().unwrap() = r.into_iter().filter(|x| x.iter().all(|v| v.is_finite())).collect();
}

#[tauri::command]
fn pet_click(id: String) {
    println!("[spike] pet click {id} (not forwarded to Bobix)");
}

// window.bobix as the pets page expects it from src/preload.js, backed by Tauri IPC.
const SHIM: &str = r#"
(() => {
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  let mouseCb = () => {};
  window.__bobixMouse = p => mouseCb(p);
  window.bobix = {
    getState: () => Promise.resolve({ settings: window.__BOBIX_SETTINGS__ || {} }),
    setRects: r => invoke('rects', { r }),
    petClick: id => invoke('pet_click', { id }),
    setPet: () => Promise.resolve(), answer: () => Promise.resolve(false),
    onMouse: cb => { mouseCb = cb; }, onSettings: () => {},
  };
})();
"#;

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default()
}

// Bobix's own display settings (size, label, language...), read only.
fn bobix_settings() -> serde_json::Value {
    let dir = if cfg!(target_os = "macos") { home().join("Library/Application Support/bobix") }
        else if cfg!(windows) { PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("bobix") }
        else { home().join(".config/bobix") };
    std::fs::read_to_string(dir.join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::json!({}))
}

fn main() {
    tauri::Builder::default()
        .manage(Rects(Mutex::new(Vec::new())))
        .invoke_handler(tauri::generate_handler![rects, pet_click])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // no Dock icon, like LSUIElement
            let settings = bobix_settings();
            let port = std::env::var("BOBIX_PORT").unwrap_or_else(|_| "4747".into());
            let token = std::fs::read_to_string(home().join(".claude/bobix/token")).unwrap_or_default();
            let url = format!("http://127.0.0.1:{port}/pets?t={}", token.trim());

            // Same strip as overlayBounds() in src/main.js: work area width, size * 1.5 + 320 tall, at the bottom.
            let mon = app.primary_monitor()?.ok_or("no primary monitor")?;
            let (wa, scale) = (mon.work_area().clone(), mon.scale_factor());
            let size = settings.get("size").and_then(|v| v.as_f64()).unwrap_or(56.0);
            let h = (((size * 1.5).round() + 320.0) * scale).min(wa.size.height as f64) as u32;

            let init = format!("window.__BOBIX_SETTINGS__ = {settings};\n{SHIM}");
            let win = WebviewWindowBuilder::new(app, "pets", WebviewUrl::External(url.parse()?))
                .title("Bobix pets")
                .transparent(true).decorations(false).shadow(false).resizable(false)
                .always_on_top(true).skip_taskbar(true).visible_on_all_workspaces(true)
                .focused(false).accept_first_mouse(true)
                .initialization_script(&init)
                .visible(false)
                .build()?;
            win.set_size(PhysicalSize::new(wa.size.width, h))?;
            win.set_position(PhysicalPosition::new(wa.position.x, wa.position.y + wa.size.height as i32 - h as i32))?;
            win.set_ignore_cursor_events(true)?;
            win.show()?;

            // Click-through except over a pet: poll the cursor every 33 ms like trackMouse() in src/main.js,
            // hit-test it against the page's rects, and feed it to the page (it gets no mouse events while ignored).
            let handle = app.handle().clone();
            thread::spawn(move || {
                let (mut interactive, mut last) = (false, (f64::NAN, f64::NAN));
                loop {
                    thread::sleep(Duration::from_millis(33));
                    let Some(win) = handle.get_webview_window("pets") else { break };
                    let (Ok(p), Ok(o), Ok(s)) = (win.cursor_position(), win.outer_position(), win.scale_factor()) else { continue };
                    let (x, y) = ((p.x - o.x as f64) / s, (p.y - o.y as f64) / s);
                    let over = handle.state::<Rects>().0.lock().unwrap().iter()
                        .any(|r| x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]);
                    if over != interactive {
                        interactive = over;
                        let _ = win.set_ignore_cursor_events(!over);
                    }
                    if (x, y) != last {
                        last = (x, y);
                        let _ = win.eval(format!("window.__bobixMouse && window.__bobixMouse({{x:{x},y:{y}}})"));
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the spike");
}
