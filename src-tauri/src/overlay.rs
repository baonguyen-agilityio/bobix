// The pets overlay: a transparent, frameless, always-on-top strip along one edge of a screen that shows the pets page.
// The mouse goes through it everywhere except over a pet. Port of createOverlay / showOverlay / overlayBounds /
// trackMouse in src/main.js.
use crate::Bobix;
use std::{sync::Arc, thread, time::Duration};
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const LABEL: &str = "pets";

// The screen chosen in Settings (by name), else the primary one.
pub fn target_monitor(app: &AppHandle, display: &str) -> Option<Monitor> {
    if display != "primary" {
        if let Some(m) = app.available_monitors().ok()?.into_iter().find(|m| m.name().is_some_and(|n| n == display)) { return Some(m); }
    }
    app.primary_monitor().ok().flatten().or_else(|| app.available_monitors().ok()?.into_iter().next())
}

// Only a strip along the chosen edge, tall enough for a pet, its card and a few bubble lanes: the compositor redraws
// this window while pets walk, so every pixel it does not cover is saved work. Physical pixels.
pub fn bounds(app: &AppHandle, b: &Bobix) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>)> {
    let (display, size, top) = {
        let c = b.core.lock().unwrap();
        (c.settings["display"].as_str().unwrap_or("primary").to_string(), c.settings["size"].as_f64().unwrap_or(56.0), c.settings["edge"] == "top")
    };
    let m = target_monitor(app, &display)?;
    let (wa, scale) = (m.work_area(), m.scale_factor());
    let h = (((size * 1.5).round() + 320.0) * scale).round().min(wa.size.height as f64) as u32;
    let y = if top { wa.position.y } else { wa.position.y + wa.size.height as i32 - h as i32 };
    Some((PhysicalPosition::new(wa.position.x, y), PhysicalSize::new(wa.size.width, h)))
}

pub fn create(app: &AppHandle, b: &Arc<Bobix>) -> tauri::Result<WebviewWindow> {
    let url = format!("http://127.0.0.1:{}/pets?t={}", b.port, b.token);
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url.parse().expect("page url")))
        .title("Bobix pets")
        .transparent(true).decorations(false).shadow(false).resizable(false).maximizable(false).minimizable(false)
        .always_on_top(true).skip_taskbar(true).visible_on_all_workspaces(true)
        .focused(false).focusable(false).accept_first_mouse(true) // macOS: never key, so the first click must reach the page
        .initialization_script(crate::BRIDGE)
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled) // Electron: backgroundThrottling false (macOS 14+)
        .on_navigation(|u| u.path() == "/pets") // the page never leaves
        .visible(false)
        .build()?;
    if let Some((pos, size)) = bounds(app, b) { win.set_size(size)?; win.set_position(pos)?; }
    win.set_ignore_cursor_events(true)?;
    #[cfg(target_os = "macos")]
    on_full_screen_spaces(&win);
    Ok(win)
}

// macOS: also over apps in full screen (Electron's visibleOnFullScreen).
#[cfg(target_os = "macos")]
fn on_full_screen_spaces(win: &WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior as B};
    let w = win.clone();
    let _ = win.run_on_main_thread(move || {
        let Ok(ptr) = w.ns_window() else { return };
        let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
        ns.setCollectionBehavior(ns.collectionBehavior() | B::CanJoinAllSpaces | B::FullScreenAuxiliary | B::Stationary | B::IgnoresCycle);
    });
}

pub fn place(app: &AppHandle, b: &Bobix) {
    let Some(win) = app.get_webview_window(LABEL) else { return };
    if let Some((pos, size)) = bounds(app, b) { let _ = win.set_size(size); let _ = win.set_position(pos); }
}

// A full-screen transparent window costs the compositor even when empty: keep it hidden while no pet is out.
pub fn show_or_hide(app: &AppHandle, b: &Bobix) {
    let Some(win) = app.get_webview_window(LABEL) else { return };
    let want = { let c = b.core.lock().unwrap(); c.settings["petsVisible"] == true && !c.rects.is_empty() };
    let visible = win.is_visible().unwrap_or(false);
    if want && !visible {
        let _ = win.show();
        // Linux drops "always on top" when a window is hidden and shown again: set it every time.
        let _ = win.set_always_on_top(true);
        let _ = win.set_visible_on_all_workspaces(true);
    } else if !want && visible {
        let _ = win.hide();
        b.core.lock().unwrap().interactive = false;
        let _ = win.set_ignore_cursor_events(true);
    }
}

// The page only reports its pets (rects) while it runs, and a hidden webview may not run (macOS, WebView2): when
// sessions arrive while the overlay is hidden, show it so the page can lay its pets out; hide it again if it shows none.
pub fn wake(app: &AppHandle, b: &Arc<Bobix>) {
    let Some(win) = app.get_webview_window(LABEL) else { return };
    let go = { let c = b.core.lock().unwrap(); c.settings["petsVisible"] == true && c.rects.is_empty() && !c.sessions.is_empty() };
    if !go || win.is_visible().unwrap_or(true) { return; }
    let _ = win.show();
    let _ = win.set_always_on_top(true);
    let (app, b) = (app.clone(), b.clone());
    thread::spawn(move || { thread::sleep(Duration::from_millis(1500)); show_or_hide(&app, &b); });
}

// The overlay ignores the mouse except over a pet: poll the cursor every 33 ms, compare with the pets' rects from the
// page (CSS px relative to the window), and feed the cursor to the page, which gets no mouse events while ignored.
pub fn track_mouse(app: AppHandle, b: Arc<Bobix>) {
    thread::spawn(move || {
        let mut last = (f64::NAN, f64::NAN);
        loop {
            thread::sleep(Duration::from_millis(33));
            let Some(win) = app.get_webview_window(LABEL) else { continue };
            if !win.is_visible().unwrap_or(false) { continue; }
            let (Ok(p), Ok(o), Ok(s)) = (win.cursor_position(), win.outer_position(), win.scale_factor()) else { continue };
            let (x, y) = (((p.x - o.x as f64) / s).round(), ((p.y - o.y as f64) / s).round());
            let flip = {
                let mut c = b.core.lock().unwrap();
                let over = c.rects.iter().any(|r| x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]);
                let flip = over != c.interactive;
                c.interactive = over;
                flip.then_some(over)
            };
            if let Some(over) = flip { let _ = win.set_ignore_cursor_events(!over); }
            if (x, y) != last {
                last = (x, y);
                let _ = win.eval(format!("window.__bobixEmit && __bobixEmit('mouse', {{ x: {x}, y: {y} }})"));
            }
        }
    });
}
