// The tray / menu bar icon and its menu (buildTray in the 0.1 (Electron) app's main.js): same items, same order. The icon is
// Bobix's default flat pet (FLAT.default in src/ui/creature.js), drawn here from the same shapes.
use crate::{i18n::t, panel, settings, Bobix};
use serde_json::json;
use std::sync::Arc;
use tauri::{image::Image, menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem}, tray::TrayIconBuilder, AppHandle, Manager};

// size×size RGBA pixels of the pet (ears to feet), 4×4 samples a pixel. template: black with the eyes cut out, a macOS
// template image that follows the menu bar's light or dark look; otherwise the sky pet with dark eyes.
pub fn bitmap(size: usize, template: bool) -> Vec<u8> {
    let rr = |x: f64, y: f64, l: f64, t: f64, w: f64, h: f64, r: f64| ((x - l - w / 2.0).abs() - w / 2.0 + r).max(0.0).hypot(((y - t - h / 2.0).abs() - h / 2.0 + r).max(0.0)) <= r;
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
        if let Err(e) = made { eprintln!("[bobix] tray: {e}"); }
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
}
