// How long since the last keyboard or mouse input at this computer, from the OS (no permission asked), so a request is
// not held for the phones while someone is sitting here. Read once a second and only while such a request is held.
// Where it cannot be read (some Wayland sessions, a Linux without xprintidle) it is None, and Settings → Phones shows a
// choice instead. Behind a trait so tests use a fake; under BOBIX_SMOKE it is fixed at "no recent input".

pub trait Source: Send + Sync {
    /// Milliseconds since the last input, None when the OS does not tell.
    fn idle_ms(&self) -> Option<u64>;
}

pub struct Os;
pub struct Fixed(pub Option<u64>);
impl Source for Fixed { fn idle_ms(&self) -> Option<u64> { self.0 } }

#[cfg(target_os = "macos")]
mod sys {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" { fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64; }
    pub fn idle_ms() -> Option<u64> {
        // kCGEventSourceStateCombinedSessionState = 0, kCGAnyInputEventType = ~0
        let s = unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) };
        (s.is_finite() && s >= 0.0).then(|| (s * 1000.0) as u64)
    }
}
#[cfg(windows)]
mod sys {
    use windows_sys::Win32::{System::SystemInformation::GetTickCount, UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO}};
    pub fn idle_ms() -> Option<u64> {
        let mut i = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        // SAFETY: `i` is a valid LASTINPUTINFO with its size set.
        if unsafe { GetLastInputInfo(&mut i) } == 0 { return None; }
        Some(unsafe { GetTickCount() }.wrapping_sub(i.dwTime) as u64)
    }
}
// shortcut: Linux asks `xprintidle` (X11 screen saver extension) and reads nothing without it; a direct libXss call when it matters.
#[cfg(all(unix, not(target_os = "macos")))]
mod sys {
    pub fn idle_ms() -> Option<u64> {
        let o = std::process::Command::new("xprintidle").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).flatten()
    }
}
impl Source for Os { fn idle_ms(&self) -> Option<u64> { sys::idle_ms() } }

// The source the app reads: fixed under the smoke run, so it does not depend on who sits at the machine.
pub fn source() -> Box<dyn Source> {
    if std::env::var_os("BOBIX_SMOKE").is_some() { Box::new(Fixed(Some(3_600_000))) } else { Box::new(Os) }
}

pub const AT_COMPUTER_MS: u64 = 10_000;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_source_and_the_real_one() {
        assert_eq!(Fixed(Some(5)).idle_ms(), Some(5));
        assert_eq!(Fixed(None).idle_ms(), None);
        if cfg!(target_os = "macos") { assert!(Os.idle_ms().is_some(), "macOS tells the input time without a permission"); }
    }
}
