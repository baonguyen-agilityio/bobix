// The tray / menu bar icon and its menu (buildTray in the 0.1 (Electron) app's main.js): same items, same order. The icon is
// Bobix's default flat pet (FLAT.default in src/ui/creature.js), drawn here from the same shapes.
use crate::{i18n::t, panel, settings, Bobix};
use serde_json::json;
use std::sync::Arc;
use tauri::{image::Image, menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem}, tray::TrayIconBuilder, AppHandle, Manager};

// size×size RGBA pixels of the pet (ears to feet), 4×4 samples a pixel. template: black with the eyes cut out, a macOS
// template image that follows the menu bar's light or dark look; otherwise the sky pet with dark eyes.
// Whether (x, y) is inside the rectangle l, t, w × h with corners of radius r.
fn rr(x: f64, y: f64, l: f64, t: f64, w: f64, h: f64, r: f64) -> bool {
    ((x - l - w / 2.0).abs() - w / 2.0 + r).max(0.0).hypot(((y - t - h / 2.0).abs() - h / 2.0 + r).max(0.0)) <= r
}

pub fn bitmap(size: usize, template: bool) -> Vec<u8> {
    let dot = |x: f64, y: f64, cx: f64, cy: f64, r: f64| (x - cx).hypot(y - cy) <= r;
    let (hi, lo, ink) = ([154.0, 211.0, 245.0], [98.0, 181.0, 228.0], [31.0, 34.0, 48.0]);
    // a point of the pet's 36-unit box: its colour, or None outside it
    let at = |x: f64, y: f64| -> Option<[f64; 3]> {
        let body = rr(x, y, 6.5, 7.0, 23.0, 22.0, 8.5) || dot(x, y, 11.5, 8.6, 3.3) || dot(x, y, 24.5, 8.6, 3.3);
        let limb = dot(x, y, 6.5, 21.0, 2.3) || dot(x, y, 29.5, 21.0, 2.3) || rr(x, y, 10.3, 28.4, 5.6, 3.2, 1.6) || rr(x, y, 20.1, 28.4, 5.6, 3.2, 1.6);
        let eye = dot(x, y, 14.5, 16.0, 2.0) || dot(x, y, 21.5, 16.0, 2.0);
        if limb { return Some(if template { [0.0; 3] } else { lo }); }
        if !body { return None; }
        if eye { return if template { None } else { Some(ink) }; }
        if template { return Some([0.0; 3]); }
        let k = ((y - 5.3) / 23.7).clamp(0.0, 1.0);
        Some([0, 1, 2].map(|i| hi[i] + (lo[i] - hi[i]) * k))
    };
    let (x0, y0, span) = (4.0, 4.6, 28.0); // ears at the top to feet at the bottom, centred across
    let mut buf = vec![0u8; size * size * 4];
    for py in 0..size {
        for px in 0..size {
            let (mut sum, mut n) = ([0.0; 3], 0.0);
            for sy in 0..4 {
                for sx in 0..4 {
                    let u = |p: usize, s: usize| (p as f64 + (s as f64 + 0.5) / 4.0) / size as f64 * span;
                    if let Some(c) = at(x0 + u(px, sx), y0 + u(py, sy)) { for i in 0..3 { sum[i] += c[i]; } n += 1.0; }
                }
            }
            if n > 0.0 { buf[(py * size + px) * 4..][..4].copy_from_slice(&[(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, (n / 16.0 * 255.0).round() as u8]); }
        }
    }
    buf
}

// ---- the badge: how many sessions wait for your approval ----
// Digits 0–9 and "+" as strokes through a 3×5 grid (0–2 across, 0–4 down), drawn round and smoothed like the pet, not
// as pixel steps.
type Stroke = &'static [(f64, f64)];
#[cfg_attr(target_os = "macos", allow(dead_code))] // drawn on Windows and Linux; macOS shows the count as text
const GLYPHS: [&[Stroke]; 11] = [
    &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 0.0)]],
    &[&[(0.2, 0.8), (1.0, 0.0), (1.0, 4.0)]],
    &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0), (0.0, 4.0), (2.0, 4.0)]],
    &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0)], &[(0.4, 2.0), (2.0, 2.0)]],
    &[&[(0.0, 0.0), (0.0, 2.0), (2.0, 2.0)], &[(2.0, 0.0), (2.0, 4.0)]],
    &[&[(2.0, 0.0), (0.0, 0.0), (0.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]],
    &[&[(2.0, 0.0), (0.0, 0.0), (0.0, 4.0), (2.0, 4.0), (2.0, 2.0), (0.0, 2.0)]],
    &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0)]],
    &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 0.0)], &[(0.0, 2.0), (2.0, 2.0)]],
    &[&[(2.0, 2.0), (0.0, 2.0), (0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0)]],
    &[&[(1.0, 1.0), (1.0, 3.0)], &[(0.0, 2.0), (2.0, 2.0)]],
];
#[cfg_attr(target_os = "macos", allow(dead_code))] // drawn on Windows and Linux; macOS shows the count as text
const CORAL: [f64; 3] = [255.0, 122.0, 77.0]; // the pets' "waiting" badge (STATUS_FILL in creature.js)

// Draws n (1–9, "9+" above) on a coral pill h pixels high in the top right corner of a size×size RGBA picture. What is
// under the pill is cut away a pixel around it, so the count reads on any picture.
#[cfg_attr(target_os = "macos", allow(dead_code))] // drawn on Windows and Linux; macOS shows the count as text
pub fn draw_badge(buf: &mut [u8], size: usize, n: usize, h: usize) {
    let glyphs: Vec<usize> = if n > 9 { vec![9, 10] } else { vec![n] };
    let k = (h / 7).min((size - 6) / (glyphs.len() * 4)).max(1); // pixels per glyph cell, smaller for "9+" in a small picture
    let tw = glyphs.len() * 4 * k - k;
    let w = (tw + 2 * k + 2).max(h).min(size);
    let (l, hf, wf, r) = ((size - w) as f64, h as f64, w as f64, h as f64 / 2.0);
    for py in 0..(h + 2).min(size) {
        for px in (size - w).saturating_sub(2)..size {
            let (mut pill, mut ring) = (0.0, 0.0);
            for sy in 0..4 {
                for sx in 0..4 {
                    let (x, y) = (px as f64 + (sx as f64 + 0.5) / 4.0, py as f64 + (sy as f64 + 0.5) / 4.0);
                    if rr(x, y, l, 0.0, wf, hf, r) { pill += 1.0 / 16.0; }
                    if rr(x, y, l - 1.2, -1.2, wf + 2.4, hf + 2.4, r + 1.2) { ring += 1.0 / 16.0; }
                }
            }
            if ring == 0.0 { continue; }
            let o = &mut buf[(py * size + px) * 4..][..4];
            let under = o[3] as f64 / 255.0 * (1.0 - ring); // cut out round the pill
            let a = pill + under * (1.0 - pill);
            if a <= 0.0 { o.copy_from_slice(&[0; 4]); continue; }
            for i in 0..3 { o[i] = ((CORAL[i] * pill + o[i] as f64 * under * (1.0 - pill)) / a).round() as u8; }
            o[3] = (a * 255.0).round() as u8;
        }
    }
    let (x0, y0, kf) = (size - w + (w - tw) / 2, (h - 5 * k) / 2, k as f64);
    let half = kf * 0.55; // half the stroke: a glyph cell's width, a little bolder
    let near = |x: f64, y: f64, (ax, ay): (f64, f64), (bx, by): (f64, f64)| { // within half a stroke of the segment a–b
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy).max(1e-9)).clamp(0.0, 1.0);
        (x - ax - t * dx).hypot(y - ay - t * dy) <= half
    };
    for (gi, &g) in glyphs.iter().enumerate() {
        let (ox, oy) = ((x0 + gi * 4 * k) as f64 + kf / 2.0, y0 as f64 + kf / 2.0); // the grid's first cell centre
        let at = |(gx, gy): (f64, f64)| (ox + gx * kf, oy + gy * kf);
        let (l, t) = ((ox - half).floor().max(0.0) as usize, (oy - half).floor().max(0.0) as usize);
        let (r, b) = (((ox + 2.0 * kf + half).ceil() as usize).min(size), ((oy + 4.0 * kf + half).ceil() as usize).min(size));
        for py in t..b {
            for px in l..r {
                let mut c = 0.0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let (x, y) = (px as f64 + (sx as f64 + 0.5) / 4.0, py as f64 + (sy as f64 + 0.5) / 4.0);
                        if GLYPHS[g].iter().any(|s| s.windows(2).any(|p| near(x, y, at(p[0]), at(p[1])))) { c += 1.0 / 16.0; }
                    }
                }
                if c == 0.0 { continue; }
                let o = &mut buf[(py * size + px) * 4..][..4];
                let a = o[3] as f64 / 255.0;
                for i in 0..3 { o[i] = (255.0 * c + o[i] as f64 * (1.0 - c)).round() as u8; }
                o[3] = ((c + a * (1.0 - c)) * 255.0).round() as u8;
            }
        }
    }
}

// Sessions waiting for your approval (the panel's "Needs you"), or None for no badge: switched off in Settings, or
// Do not disturb is on.
pub fn waiting(settings: &serde_json::Value, sessions: &crate::state::Sessions, now: u64) -> Option<usize> {
    let dnd = settings["dndUntil"].as_f64().unwrap_or(0.0) > now as f64;
    (settings["badge"] == true && !dnd).then(|| sessions.values().filter(|s| s.state == "waiting").count())
}

// What the tray and the panel window show now: (count, language). None: not set yet (a new tray or panel window).
static SHOWN: std::sync::Mutex<Option<(usize, String)>> = std::sync::Mutex::new(None);
// The count last put on the tray icon, for the smoke check.
pub static APPLIED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

// Puts the count on the tray icon (macOS: a number next to the menu bar icon; Windows and Linux: a pill drawn into
// the icon) and in its tooltip, and, while the panel is open, on the Dock icon (macOS) or its taskbar button
// (Windows). Called on every change of state; does nothing when the count is the same.
pub fn badge(app: &AppHandle, b: &Arc<Bobix>, force: bool) {
    let (n, l) = { let c = b.core.lock().unwrap(); (waiting(&c.settings, &c.sessions, crate::state::now()).unwrap_or(0), crate::lang(&c.settings)) };
    {
        let mut shown = SHOWN.lock().unwrap();
        if !force && shown.as_ref().is_some_and(|(m, k)| *m == n && *k == l) { return; }
        *shown = Some((n, l.clone()));
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let app = app2;
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(if n > 0 { t(&l, "tray.badge", &[("n", &n.to_string())]) } else { "Bobix".into() }));
            #[cfg(target_os = "macos")]
            let _ = tray.set_title(if n > 0 { Some(if n > 9 { "9+".to_string() } else { n.to_string() }) } else { None::<String> });
            #[cfg(not(target_os = "macos"))]
            {
                let mut px = bitmap(32, false);
                if n > 0 { draw_badge(&mut px, 32, n, 16); }
                let _ = tray.set_icon(Some(Image::new_owned(px, 32, 32)));
            }
            APPLIED.store(n, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(w) = app.get_webview_window(panel::LABEL) {
            #[cfg(target_os = "macos")]
            let _ = w.set_badge_count(if n > 0 { Some(n as i64) } else { None });
            #[cfg(target_os = "windows")]
            let _ = w.set_overlay_icon(if n > 0 { let mut px = vec![0u8; 32 * 32 * 4]; draw_badge(&mut px, 32, n, 32); Some(Image::new_owned(px, 32, 32)) } else { None });
            #[cfg(target_os = "linux")]
            let _ = &w;
        }
    });
}

fn menu(app: &AppHandle, b: &Bobix) -> tauri::Result<Menu<tauri::Wry>> {
    let (st, update) = { let c = b.core.lock().unwrap(); (c.settings.clone(), c.update.clone()) };
    let l = crate::lang(&st);
    let tr = |k: &str| t(&l, k, &[]);
    let version = app.package_info().version.to_string();
    let item = |id: &str, text: String| MenuItem::with_id(app, id, text, true, None::<&str>);
    let sep = || PredefinedMenuItem::separator(app);
    let m = Menu::new(app)?;
    m.append(&MenuItem::with_id(app, "version", format!("Bobix {version}"), false, None::<&str>)?)?;
    m.append(&sep()?)?;
    m.append(&item("panel", tr("tray.panel"))?)?;
    m.append(&item("pets", tr(if st["petsVisible"] == true { "tray.hide" } else { "tray.show" }))?)?;
    m.append(&item("showAll", tr("tray.showAll"))?)?;
    m.append(&CheckMenuItem::with_id(app, "pause", tr("tray.pause"), true, st["paused"] == true, None::<&str>)?)?;
    m.append(&item("feedback", tr("tray.feedback"))?)?;
    if let Some(v) = settings::news_for(&st, &version, &crate::i18n::NEWS) { m.append(&item("news", t(&l, "tray.news", &[("v", &v)]))?)?; }
    if let Some((v, _)) = &update {
        m.append(&sep()?)?;
        m.append(&item("update", t(&l, "tray.update", &[("v", v)]))?)?;
    }
    m.append(&sep()?)?;
    m.append(&item("quit", tr("tray.quit"))?)?;
    Ok(m)
}

fn on_menu(app: &AppHandle, id: &str) {
    let b = app.state::<Arc<Bobix>>().inner().clone();
    let st = b.core.lock().unwrap().settings.clone();
    match id {
        "panel" => panel::show(app, &b, None, None, None),
        "pets" => { crate::update_settings(&b, json!({ "petsVisible": st["petsVisible"] != true })); }
        "showAll" => {
            crate::state::set_pet(&mut b.core.lock().unwrap().sessions, "*", "shown");
            b.broadcast();
            if st["petsVisible"] != true { crate::update_settings(&b, json!({ "petsVisible": true })); }
        }
        "pause" => { crate::update_settings(&b, json!({ "paused": st["paused"] != true })); }
        "feedback" => panel::show(app, &b, None, Some("feedback".into()), None),
        "news" => panel::show(app, &b, None, Some("sessions".into()), None),
        "update" => { if let Some((_, url)) = b.core.lock().unwrap().update.clone() { let _ = tauri_plugin_opener::open_url(url, None::<&str>); } }
        "quit" => app.exit(0),
        _ => {}
    }
}

// Makes the tray icon the first time, then sets a fresh menu (texts and checks follow the settings).
pub fn build(app: &AppHandle, b: &Arc<Bobix>) {
    let (app2, b) = (app.clone(), b.clone());
    let _ = app.run_on_main_thread(move || {
        let app = app2;
        let m = match menu(&app, &b) { Ok(m) => m, Err(e) => return eprintln!("[bobix] tray menu: {e}") };
        if let Some(tray) = app.tray_by_id("main") { let _ = tray.set_menu(Some(m)); return; }
        let mac = cfg!(target_os = "macos");
        let made = TrayIconBuilder::with_id("main").icon(Image::new_owned(bitmap(32, mac), 32, 32)).icon_as_template(mac).tooltip("Bobix").menu(&m)
            .on_menu_event(|app, e| on_menu(app, e.id().as_ref())).build(&app);
        match made { Ok(_) => badge(&app, &b, true), Err(e) => eprintln!("[bobix] tray: {e}") }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_pet_fills_the_icon() {
        for template in [false, true] {
            let px = super::bitmap(32, template);
            let at = |x: usize, y: usize| &px[(y * 32 + x) * 4..][..4];
            assert_eq!(at(0, 0)[3], 0, "corner: transparent");
            assert_eq!(at(16, 26)[3], 255, "body: solid");
            assert_eq!(at(8, 3)[3], 255, "ear: solid");
            // the left eye, at (14.5, 16) of the pet's box
            let eye = at(12, 13);
            if template { assert_eq!(eye[3], 0, "eye: cut out of the template"); } else { assert_eq!(eye, [31, 34, 48, 255], "eye: dark"); }
        }
    }

    #[test]
    fn the_badge_counts_and_reads() {
        let mut px = super::bitmap(32, false);
        super::draw_badge(&mut px, 32, 3, 16);
        let at = |x: usize, y: usize| px[(y * 32 + x) * 4..][..4].to_vec();
        assert_eq!(at(18, 8), [255, 122, 77, 255], "pill: coral");
        assert_eq!(at(24, 4), [255, 255, 255, 255], "the 3: white");
        assert_eq!(at(16, 26)[3], 255, "the pet below: untouched");
        let mut wide = vec![0u8; 32 * 32 * 4];
        super::draw_badge(&mut wide, 32, 12, 16);
        assert!((0..32).any(|x| wide[(8 * 32 + x) * 4 + 3] == 255 && x < 16), "9+ makes the pill wider");
    }

    #[test]
    fn the_count_follows_settings_and_do_not_disturb() {
        let mut sessions = crate::state::Sessions::new();
        for (id, st) in [("a", "waiting"), ("b", "working"), ("c", "waiting"), ("d", "done")] {
            let mut s = crate::state::Session::default();
            s.id = id.into(); s.state = st.into();
            sessions.insert(id.into(), s);
        }
        let mut st = crate::settings::defaults();
        assert_eq!(super::waiting(&st, &sessions, 1000), Some(2), "on by default: the waiting ones");
        st["dndUntil"] = serde_json::json!(5000);
        assert_eq!(super::waiting(&st, &sessions, 1000), None, "do not disturb: no badge");
        assert_eq!(super::waiting(&st, &sessions, 6000), Some(2), "do not disturb over");
        st["badge"] = serde_json::json!(false);
        assert_eq!(super::waiting(&st, &sessions, 6000), None, "switched off");
    }
}
