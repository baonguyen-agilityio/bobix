// Tells phone access when the computer is about to sleep and when it woke, so the relay and the phones hear "asleep" from
// the computer itself (macOS: the system power notifications; elsewhere nothing: the relay's ping shows it offline).
// shortcut: macOS only, written against the IOKit API and not exercised here (the machine must not be put to sleep by a
// test); other systems when a phone's "offline" without a reason matters.
use crate::Bobix;
use std::sync::Arc;

pub fn start(b: &Arc<Bobix>) {
    #[cfg(target_os = "macos")]
    mac::start(b.clone());
    #[cfg(not(target_os = "macos"))]
    let _ = b;
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::{ffi::c_void, sync::{atomic::{AtomicU32, Ordering}, OnceLock}, time::Duration};

    static BOBIX: OnceLock<Arc<Bobix>> = OnceLock::new();
    static ROOT: AtomicU32 = AtomicU32::new(0);

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IORegisterForSystemPower(refcon: *mut c_void, port: *mut *mut c_void, cb: extern "C" fn(*mut c_void, u32, u32, *mut c_void), notifier: *mut u32) -> u32;
        fn IOAllowPowerChange(kernel_port: u32, notification_id: isize) -> i32;
        fn IONotificationPortGetRunLoopSource(port: *mut c_void) -> *mut c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
        fn CFRunLoopRun();
        static kCFRunLoopCommonModes: *const c_void;
    }
    const CAN_SLEEP: u32 = 0xe000_0270;
    const WILL_SLEEP: u32 = 0xe000_0280;
    const POWERED_ON: u32 = 0xe000_0300;

    extern "C" fn on_power(_refcon: *mut c_void, _service: u32, message: u32, argument: *mut c_void) {
        if let Some(b) = BOBIX.get() {
            match message {
                WILL_SLEEP => { crate::phones::on_sleep(b); std::thread::sleep(Duration::from_millis(400)); } // the frames go out first
                POWERED_ON => crate::phones::on_wake(b),
                _ => {}
            }
        }
        // sleep is never held up: every change is allowed at once
        if matches!(message, CAN_SLEEP | WILL_SLEEP) { unsafe { IOAllowPowerChange(ROOT.load(Ordering::Relaxed), argument as isize); } }
    }

    pub fn start(b: Arc<Bobix>) {
        let _ = BOBIX.set(b);
        std::thread::spawn(|| unsafe {
            let (mut port, mut notifier) = (std::ptr::null_mut(), 0u32);
            let root = IORegisterForSystemPower(std::ptr::null_mut(), &mut port, on_power, &mut notifier);
            if root == 0 || port.is_null() { return; }
            ROOT.store(root, Ordering::Relaxed);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), IONotificationPortGetRunLoopSource(port), kCFRunLoopCommonModes);
            CFRunLoopRun();
        });
    }
}
