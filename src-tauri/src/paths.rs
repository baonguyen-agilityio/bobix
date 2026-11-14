// Where Bobix keeps its files: the same places the Electron build uses, so either build reads the other's settings,
// token and pictures. BOBIX_DATA_DIR and BOBIX_HOME move them for dev and smoke runs.
use std::{fs, path::PathBuf};

pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("BOBIX_HOME") { return PathBuf::from(h); }
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

// Electron's app.getPath('userData'): <appData>/<package.json name>, "bobix". An install that wrote "Bobix" (Linux is
// case-sensitive) is used when that is the one there.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("BOBIX_DATA_DIR") { return PathBuf::from(d); }
    let h = home();
    let app_data = if cfg!(target_os = "macos") { h.join("Library/Application Support") }
        else if cfg!(windows) { std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| h.join("AppData/Roaming")) }
        else { std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| h.join(".config")) };
    let upper = app_data.join("Bobix");
    if !cfg!(any(target_os = "macos", windows)) && upper.join("settings.json").is_file() && !app_data.join("bobix/settings.json").is_file() { return upper; }
    app_data.join("bobix")
}

pub fn settings_file() -> PathBuf { data_dir().join("settings.json") }

// Per-user secret the hook sends and the server requires: another local user can't feed or read the app.
// Same files and modes as ensureToken in the 0.1 (Electron) app's core/hooks.js.
pub fn ensure_token() -> std::io::Result<String> { ensure_token_in(&home()) }

pub fn ensure_token_in(home: &std::path::Path) -> std::io::Result<String> {
    let dir = home.join(".claude").join("bobix");
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)); }
    let file = dir.join("token");
    let mut token = fs::read_to_string(&file).map(|t| t.trim().to_string()).unwrap_or_default();
    if token.len() < 32 || !token.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        let mut buf = [0u8; 24];
        getrandom::fill(&mut buf).map_err(|e| std::io::Error::other(e.to_string()))?;
        token = buf.iter().map(|b| format!("{b:02x}")).collect();
        write_private(&file, &format!("{token}\n"))?;
    }
    write_private(&dir.join("header"), &format!("X-Bobix: {token}\n"))?;
    Ok(token)
}

// A folder as text for comparing: one separator, no trailing one, and case folded where the file system usually
// ignores case (Windows, macOS). Git prints "C:/x/app", Claude "C:\x\app", a worktree path mixes both: one folder.
pub fn dir_key(p: &str) -> String {
    let mut s = p.replace('\\', "/");
    while s.len() > 1 && s.ends_with('/') && !s.ends_with(":/") { s.pop(); }
    if cfg!(any(windows, target_os = "macos")) { s.to_lowercase() } else { s }
}
pub fn same_dir(a: &str, b: &str) -> bool { dir_key(a) == dir_key(b) }

// A file only this user can read (0600 on Unix).
pub fn write_private(file: &std::path::Path, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(file)?;
        f.write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    fs::write(file, text)
}

#[cfg(test)]
mod tests {
    // Windows: git's top folder ("C:/Users/x/code/app"), a worktree joined onto it with "\" and Claude's folder (all
    // "\") are one folder; a trailing separator or another case (Windows, macOS) too.
    #[test]
    fn folders_compare_as_folders() {
        use super::same_dir;
        let wt = std::path::Path::new("C:/Users/x/code/app").join(".claude").join("worktrees").join("bobix-7");
        let wt = if cfg!(windows) { wt.to_string_lossy().into_owned() } else { "C:/Users/x/code/app\\.claude\\worktrees\\bobix-7".into() };
        assert!(same_dir(&wt, "C:\\Users\\x\\code\\app\\.claude\\worktrees\\bobix-7"));
        assert!(same_dir("C:\\Users\\x\\code\\app\\", "C:/Users/x/code/app"));
        assert!(same_dir("/src/app/", "/src/app") && !same_dir("/src/app", "/src/app2"));
        assert_eq!(same_dir("C:/Users/X/Code/App", "c:\\users\\x\\code\\app"), cfg!(any(windows, target_os = "macos")));
        assert_eq!(super::dir_key("C:/"), super::dir_key("C:\\"));
    }

    #[test]
    fn token_is_kept_and_header_written() {
        let home = std::env::temp_dir().join(format!("bobix-token-{}", std::process::id()));
        std::env::set_var("BOBIX_HOME", &home);
        let a = super::ensure_token().unwrap();
        assert_eq!(a.len(), 48);
        assert_eq!(super::ensure_token().unwrap(), a, "the same token next time");
        let header = std::fs::read_to_string(home.join(".claude/bobix/header")).unwrap();
        assert_eq!(header, format!("X-Bobix: {a}\n"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(home.join(".claude/bobix/token")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::env::remove_var("BOBIX_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }
}
