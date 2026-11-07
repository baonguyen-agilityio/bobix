// Processes behind a session: is a pid still running, and what is it called. Same commands as src/core/focus.js
// (processNames) and the isAlive check in src/main.js.
use std::process::Command;

// Is the process there? Signal 0 checks without touching it; EPERM means it runs under another user.
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

#[cfg(windows)]
pub fn is_alive(pid: u32) -> bool {
    use windows_sys::Win32::{Foundation::{CloseHandle, STILL_ACTIVE}, System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION}};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() { return std::io::Error::last_os_error().raw_os_error() == Some(5); } // access denied: it is there
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0 && code == STILL_ACTIVE as u32;
        CloseHandle(h);
        ok
    }
}

fn out(cmd: &str, args: &[&str]) -> String {
    let mut c = Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); } // CREATE_NO_WINDOW
    c.output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

// Names of processes ("claude", "node", "zsh"…), "?" when unknown.
pub fn process_names(pids: &[u32]) -> Vec<String> {
    if cfg!(windows) {
        // tasklist ships with Windows and answers in well under a second. CSV rows: "node.exe","5936",… Names lose ".exe".
        let txt = out("tasklist", &["/FO", "CSV", "/NH"]);
        let names: std::collections::HashMap<u32, String> = txt.lines().filter_map(|l| {
            let mut f = l.trim().split("\",\"");
            let name = f.next()?.strip_prefix('"')?;
            let pid = f.next()?.parse().ok()?;
            Some((pid, name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")).unwrap_or(name).to_string()))
        }).collect();
        return pids.iter().map(|p| names.get(p).cloned().unwrap_or_else(|| "?".into())).collect();
    }
    // Linux cuts `comm` to 15 characters ("gnome-terminal-"): read the program path from the command line there.
    let col = if cfg!(target_os = "linux") { "args=" } else { "comm=" };
    pids.iter().map(|p| {
        let v = out("ps", &["-o", col, "-p", &p.to_string()]);
        let prog = if cfg!(target_os = "linux") { v.split_whitespace().next().unwrap_or("").to_string() } else { v };
        let name = prog.rsplit('/').next().unwrap_or("").trim_start_matches('-').to_string(); // "-bash": a login shell
        if name.is_empty() { "?".into() } else { name }
    }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn this_process_is_alive_and_named() {
        assert!(super::is_alive(std::process::id()));
        let n = super::process_names(&[std::process::id()]);
        assert_eq!(n.len(), 1);
        assert_ne!(n[0], "?");
    }
}
