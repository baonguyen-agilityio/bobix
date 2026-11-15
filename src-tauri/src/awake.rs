// Keeps the computer awake while phone access is on and a session works or waits, so a phone's command can arrive. The
// inhibitor is tied to Bobix's own process, so a crash releases it: `caffeinate -i -w <pid>` on macOS, `systemd-inhibit`
// around a process that ends with Bobix on Linux, SetThreadExecutionState on a thread on Windows. It does not hold when a
// laptop's lid is closed (the settings text says so). Released as soon as a condition ends.
use std::process::{Child, Command, Stdio};

#[derive(Default)]
pub struct Awake {
    held: Option<Held>,
}
enum Held { Proc(Child), #[cfg(windows)] Thread(std::sync::mpsc::Sender<()>) }

impl Awake {
    pub fn is_held(&self) -> bool { self.held.is_some() }
    // Takes or drops the inhibitor to match `want`. Returns whether it is held now.
    pub fn set(&mut self, want: bool) -> bool {
        if want && self.held.is_none() { self.held = acquire(); }
        if !want { self.release(); }
        self.held.is_some()
    }
    pub fn release(&mut self) {
        match self.held.take() {
            Some(Held::Proc(mut c)) => { let _ = c.kill(); let _ = c.wait(); }
            #[cfg(windows)]
            Some(Held::Thread(tx)) => { let _ = tx.send(()); }
            None => {}
        }
    }
}
impl Drop for Awake { fn drop(&mut self) { self.release(); } }

#[cfg(target_os = "macos")]
fn acquire() -> Option<Held> {
    let me = std::process::id().to_string();
    Command::new("caffeinate").args(["-i", "-w", &me]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok().map(Held::Proc)
}
#[cfg(all(unix, not(target_os = "macos")))]
fn acquire() -> Option<Held> {
    let me = std::process::id().to_string();
    // `tail --pid` ends when Bobix does, and systemd-inhibit holds the lock for as long as it runs
    Command::new("systemd-inhibit").args(["--what=idle:sleep", "--who=Bobix", "--why=phone access: a session is working", "--mode=block", "tail", "--pid", &me, "-f", "/dev/null"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok().map(Held::Proc)
}
#[cfg(windows)]
fn acquire() -> Option<Held> {
    use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        // SAFETY: plain flags; the state belongs to this thread and ends with it.
        unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED); }
        let _ = rx.recv(); // until released, or Bobix ends
        unsafe { SetThreadExecutionState(ES_CONTINUOUS); }
    });
    Some(Held::Thread(tx))
}

#[cfg(test)]
mod tests {
    #[test]
    fn held_while_wanted_and_released_after() {
        let mut a = super::Awake::default();
        assert!(!a.set(false));
        if cfg!(target_os = "macos") {
            assert!(a.set(true) && a.is_held(), "caffeinate runs while wanted");
            assert!(a.set(true), "asking again does not start a second one");
            assert!(!a.set(false) && !a.is_held(), "released when the condition ends");
        }
    }
}
