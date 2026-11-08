// Start at login, on macOS, Windows and Linux. Port of src/core/autostart.js: the same LaunchAgent label and Linux
// .desktop file, pointing at this executable.
// ponytail: macOS uses the LaunchAgent for every build (Electron's packaged app used a login item); a login item
// needs SMAppService, add it if the LaunchAgent's "background item" notice bothers anyone.
use std::{fs, path::PathBuf};

const LABEL: &str = "dev.bobix";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

fn linux_file() -> PathBuf { crate::paths::home().join(".config/autostart/bobix.desktop") }
fn mac_file() -> PathBuf { crate::paths::home().join(format!("Library/LaunchAgents/{LABEL}.plist")) }
fn exe() -> String { std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default() }

fn xml(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

pub fn desktop_entry(exe: &str) -> String {
    let q: String = exe.chars().flat_map(|c| if "\"\\$`".contains(c) { vec!['\\', c] } else { vec![c] }).collect();
    format!("[Desktop Entry]\nType=Application\nName=Bobix\nExec=\"{q}\"\nX-GNOME-Autostart-enabled=true\n")
}

pub fn plist(exe: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>ProgramArguments</key><array><string>{}</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
"#, xml(exe))
}

fn write(f: PathBuf, text: String, on: bool) -> Result<(), String> {
    if !on { return match fs::remove_file(&f) { Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()), _ => Ok(()) }; }
    fs::create_dir_all(f.parent().unwrap()).and_then(|_| fs::write(&f, text)).map_err(|e| e.to_string())
}

pub fn set(on: bool) -> Result<(), String> {
    if cfg!(target_os = "macos") { return write(mac_file(), plist(&exe()), on); }
    if cfg!(windows) {
        let quoted = format!("\"{}\"", exe());
        let r = if on { crate::procs::exec("reg", &["add", RUN_KEY, "/v", "Bobix", "/t", "REG_SZ", "/d", &quoted, "/f"], 10, &[]) }
            else if get() { crate::procs::exec("reg", &["delete", RUN_KEY, "/v", "Bobix", "/f"], 10, &[]) } else { return Ok(()) };
        return if r.ok { Ok(()) } else { Err(r.err.trim().to_string()) };
    }
    write(linux_file(), desktop_entry(&exe()), on)
}

pub fn get() -> bool {
    if cfg!(target_os = "macos") { return mac_file().exists(); }
    if cfg!(windows) { return crate::procs::exec("reg", &["query", RUN_KEY, "/v", "Bobix"], 10, &[]).ok; }
    linux_file().exists()
}

// The app used to be called Session Guild: carry its settings over once, and its dev-mode start-at-login files.
pub fn migrate_from_old_name(settings_file: &std::path::Path) {
    if settings_file.exists() { return; }
    let Some(app_data) = settings_file.parent().and_then(|d| d.parent()) else { return };
    if let Some(old) = ["Session Guild", "session-guild"].iter().map(|d| app_data.join(d).join("settings.json")).find(|f| f.exists()) {
        let _ = fs::create_dir_all(settings_file.parent().unwrap()).and_then(|_| fs::copy(old, settings_file));
    }
    let home = crate::paths::home();
    let old: Vec<PathBuf> = [home.join("Library/LaunchAgents/dev.session-guild.plist"), home.join(".config/autostart/session-guild.desktop")]
        .into_iter().filter(|f| f.exists()).collect();
    if !old.is_empty() {
        for f in old { let _ = fs::remove_file(f); }
        if let Err(e) = set(true) { eprintln!("[bobix] autostart: {e}"); }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn login_entries_quote_the_path() {
        assert_eq!(super::desktop_entry("/opt/My \"Apps\"/bobix"), "[Desktop Entry]\nType=Application\nName=Bobix\nExec=\"/opt/My \\\"Apps\\\"/bobix\"\nX-GNOME-Autostart-enabled=true\n");
        assert!(super::plist("/Applications/A&B.app/Contents/MacOS/bobix").contains("<string>/Applications/A&amp;B.app/Contents/MacOS/bobix</string>"));
        assert!(super::plist("x").contains("<key>Label</key><string>dev.bobix</string>"));
    }
}
