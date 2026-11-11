// Processes behind a session: is a pid still running, what is it called, and bringing its window forward. Same
// commands and timeouts as the 0.1 (Electron) app's core/focus.js and the isAlive check in the 0.1 (Electron) app's main.js.
use std::{process::{Command, Stdio}, thread, time::{Duration, Instant}};

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

// What a command printed. `ok`: it ran and exited 0; `missing`: there is no such program.
pub struct Ran { pub ok: bool, pub missing: bool, pub out: String, pub err: String }

// Runs a command (argument list, never a shell) for at most `secs` seconds; stdout and stderr are read while it runs, so
// a long output can't fill the pipe and stall it.
pub fn exec(cmd: &str, args: &[&str], secs: u64, env: &[(&str, &str)]) -> Ran {
    let mut c = Command::new(cmd);
    c.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).envs(env.iter().copied());
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); } // CREATE_NO_WINDOW
    #[cfg(unix)]
    { use std::os::unix::process::CommandExt; c.process_group(0); } // its own group: a timeout ends what it started too
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return Ran { ok: false, missing: e.kind() == std::io::ErrorKind::NotFound, out: String::new(), err: e.to_string() },
    };
    let read = |p: Option<Box<dyn std::io::Read + Send>>| p.map(|mut p| thread::spawn(move || { let mut v = Vec::new(); let _ = p.read_to_end(&mut v); String::from_utf8_lossy(&v).into_owned() }));
    let (so, se) = (read(child.stdout.take().map(|x| Box::new(x) as _)), read(child.stderr.take().map(|x| Box::new(x) as _)));
    let limit = Instant::now() + Duration::from_secs(secs);
    let ok = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.success(),
            Ok(None) if Instant::now() < limit => thread::sleep(Duration::from_millis(20)),
            _ => {
                #[cfg(unix)]
                unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL); } // the whole group: no child keeps its pipes open
                let _ = child.kill(); let _ = child.wait(); break false;
            }
        }
    };
    let get = |h: Option<thread::JoinHandle<String>>| h.and_then(|h| h.join().ok()).unwrap_or_default();
    Ran { ok, missing: false, out: get(so), err: get(se) }
}

// Its trimmed output, or None when it fails or takes too long (PowerShell can take several seconds to start on a cold
// Windows machine).
fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let r = exec(cmd, args, if cmd == "powershell" { 15 } else { 4 }, &[]);
    r.ok.then(|| r.out.trim().to_string())
}
fn out(cmd: &str, args: &[&str]) -> String { run(cmd, args).unwrap_or_default() }

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

// claude → shell → … → terminal / IDE / app. Walked when a pet is clicked (the hook only sends the claude pid).
pub fn ancestors(pid: u32) -> Vec<u32> {
    let mut chain = vec![];
    if pid <= 1 { return chain; }
    if cfg!(windows) {
        let txt = out("powershell", &["-NoProfile", "-Command", "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId) $($_.ParentProcessId)\" }"]);
        let parent: std::collections::HashMap<u32, u32> = txt.lines().filter_map(|l| {
            let mut f = l.split_whitespace().map(|x| x.parse::<u32>());
            match (f.next(), f.next(), f.next()) { (Some(Ok(a)), Some(Ok(b)), None) => Some((a, b)), _ => None }
        }).collect();
        let mut p = Some(pid);
        while let Some(x) = p.filter(|x| *x != 0 && chain.len() < 16 && !chain.contains(x)) { chain.push(x); p = parent.get(&x).copied(); }
        return chain;
    }
    let mut p = pid;
    while p > 1 && chain.len() < 16 && !chain.contains(&p) {
        chain.push(p);
        p = out("ps", &["-o", "ppid=", "-p", &p.to_string()]).parse().unwrap_or(0);
    }
    chain
}

// macOS: the outermost app bundle in the chain is the one with the window (Terminal, iTerm, VS Code, Claude…). Helpers
// live inside it ("…/Code.app/Contents/Frameworks/Code Helper.app/…"), so take the first ".app" of each path.
pub fn outer_bundle(exec_paths: &[String]) -> Option<String> {
    exec_paths.iter().filter_map(|p| {
        if !p.starts_with('/') { return None; }
        let end = p.match_indices(".app").map(|(i, _)| i + 4).find(|e| *e == p.len() || p[*e..].starts_with('/'))?;
        Some(p[..end].to_string())
    }).last()
}

// Brings the first window found for these processes to the front.
pub fn focus_pids(pids: &[u32]) -> bool {
    let list: Vec<u32> = pids.iter().copied().filter(|p| *p > 1).collect();
    if list.is_empty() { return false; }
    if cfg!(target_os = "macos") {
        // `open -a` activates the running app: no Automation permission needed (System Events would ask for one).
        let paths: Vec<String> = list.iter().map(|p| out("ps", &["-o", "comm=", "-p", &p.to_string()])).collect();
        return outer_bundle(&paths).is_some_and(|app| run("open", &["-a", &app]).is_some());
    }
    if cfg!(windows) {
        let ids = list.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");
        return run("powershell", &["-NoProfile", "-Command",
            &format!("$w = New-Object -ComObject WScript.Shell; foreach ($p in @({ids})) {{ if ($w.AppActivate([int]$p)) {{ exit 0 }} }}; exit 1")]).is_some();
    }
    // Linux (X11): needs xdotool
    list.iter().any(|p| run("xdotool", &["search", "--onlyvisible", "--pid", &p.to_string(), "windowactivate"]).is_some())
}

pub fn has_xdotool() -> bool { run("xdotool", &["version"]).is_some() }

// The app that holds the session's window, named the way people know it: the first process up the chain that is not
// claude itself, a runtime or a shell ("gnome-terminal-server" → "Terminal").
pub fn host_app(names: &[String]) -> Option<String> {
    const SKIP: [&str; 19] = ["claude", "node", "bun", "npx", "npm", "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "sudo", "su", "login", "env", "script", "systemd", "init"];
    const KNOWN: [(&str, &str); 16] = [("gnome-terminal-server", "Terminal"), ("ptyxis-agent", "Ptyxis"), ("ptyxis", "Ptyxis"), ("kgx", "Console"),
        ("konsole", "Konsole"), ("xfce4-terminal", "Xfce Terminal"), ("tilix", "Tilix"), ("terminator", "Terminator"), ("alacritty", "Alacritty"),
        ("kitty", "kitty"), ("wezterm-gui", "WezTerm"), ("ghostty", "Ghostty"), ("code", "VS Code"), ("cursor", "Cursor"), ("tmux", "tmux"),
        ("claude-desktop", "Claude")];
    let n = names.iter().skip(1).find(|x| !x.is_empty() && *x != "?" && !SKIP.iter().any(|s| s.eq_ignore_ascii_case(x)))?;
    let low = n.to_lowercase();
    let known = |k: &str| KNOWN.iter().find(|(a, _)| *a == k).map(|(_, b)| b.to_string());
    Some(known(&low).or_else(|| known(low.split(':').next().unwrap_or(""))).unwrap_or_else(|| n.clone()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn macos_focus_picks_the_outermost_app() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(super::outer_bundle(&s(&["/Users/x/Library/Application Support/Claude/claude-code/2.1/claude.app/Contents/MacOS/claude",
            "/Applications/Claude.app/Contents/Helpers/disclaimer", "/Applications/Claude.app/Contents/MacOS/Claude"])).as_deref(), Some("/Applications/Claude.app"));
        assert_eq!(super::outer_bundle(&s(&["/usr/local/bin/claude", "/bin/zsh", "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper",
            "/Applications/Visual Studio Code.app/Contents/MacOS/Electron"])).as_deref(), Some("/Applications/Visual Studio Code.app"));
        assert_eq!(super::outer_bundle(&s(&["/usr/local/bin/claude", "/bin/zsh", "/opt/homebrew/bin/tmux"])), None);
    }

    #[test]
    fn host_app_is_named_the_way_people_know_it() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(super::host_app(&s(&["claude", "bash", "gnome-terminal-server"])).as_deref(), Some("Terminal"));
        assert_eq!(super::host_app(&s(&["claude", "zsh", "claude-desktop"])).as_deref(), Some("Claude"));
        assert_eq!(super::host_app(&s(&["claude", "zsh", "iTerm2"])).as_deref(), Some("iTerm2"));
        assert_eq!(super::host_app(&s(&["claude", "node", "?"])), None);
    }

    #[test]
    fn this_process_is_alive_and_named() {
        assert!(super::is_alive(std::process::id()));
        let n = super::process_names(&[std::process::id()]);
        assert_eq!(n.len(), 1);
        assert_ne!(n[0], "?");
    }
}
