// The tray / menu bar icon and its menu (buildTray in src/main.js): same items, same order. The icon is the app's
// pixel creature drawn from pixels (src/core/icon.js), 32 px for a 16 pt slot.
use crate::{i18n::t, panel, settings, Bobix};
use serde_json::json;
use std::sync::Arc;
use tauri::{image::Image, menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem}, tray::TrayIconBuilder, AppHandle, Manager};

const ICON: [&str; 8] = [".o....o.", "obo..obo", "obboobbo", "obbbbbbo", "obebbebo", "obbbbbbo", ".obbbbo.", ".oo..oo."];

// size×size RGBA pixels, the creature centered with `pad` pixels around it.
pub fn bitmap(size: usize, pad: usize) -> Vec<u8> {
    let mut buf = vec![0u8; size * size * 4];
    let cell = (size - 2 * pad) as f64 / 8.0;
    for y in 0..size {
        for x in 0..size {
            let (gx, gy) = (((x as f64 - pad as f64) / cell).floor(), ((y as f64 - pad as f64) / cell).floor());
            if !(0.0..8.0).contains(&gx) || !(0.0..8.0).contains(&gy) { continue; }
            let rgb = match ICON[gy as usize].as_bytes()[gx as usize] { b'o' => [90, 50, 70], b'b' => [200, 190, 236], b'e' => [30, 30, 40], _ => continue };
            buf[(y * size + x) * 4..][..4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
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
        let made = TrayIconBuilder::with_id("main").icon(Image::new_owned(bitmap(32, 0), 32, 32)).tooltip("Bobix").menu(&m)
            .on_menu_event(|app, e| on_menu(app, e.id().as_ref())).build(&app);
        if let Err(e) = made { eprintln!("[bobix] tray: {e}"); }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_creature_fills_the_icon() {
        let px = super::bitmap(32, 0);
        let at = |x: usize, y: usize| &px[(y * 32 + x) * 4..][..4];
        assert_eq!(at(0, 0), [0, 0, 0, 0], "corner: transparent");
        assert_eq!(at(4, 0), [90, 50, 70, 255], "ear outline");
        assert_eq!(at(9, 17), [30, 30, 40, 255], "eye");
    }
}
