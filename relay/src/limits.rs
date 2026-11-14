//! Fixed-window counters, used by the push gateway (per token and per calling relay).

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct RateLimit {
    max: u32,
    window: Duration,
    seen: HashMap<String, (Instant, u32)>,
}

impl RateLimit {
    pub fn new(max: u32, window: Duration) -> Self {
        RateLimit { max, window, seen: HashMap::new() }
    }

    /// True when `key` may go ahead now. Other keys are unaffected.
    pub fn allow(&mut self, key: &str) -> bool {
        let now = Instant::now();
        if self.seen.len() > 10_000 {
            let w = self.window;
            self.seen.retain(|_, (t, _)| now.duration_since(*t) < w);
        }
        let e = self.seen.entry(key.to_owned()).or_insert((now, 0));
        if now.duration_since(e.0) >= self.window {
            *e = (now, 0);
        }
        e.1 += 1;
        e.1 <= self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_key_is_limited_and_others_are_not() {
        let mut r = RateLimit::new(2, Duration::from_secs(60));
        assert!(r.allow("a") && r.allow("a") && !r.allow("a"));
        assert!(r.allow("b"));
    }

    #[test]
    fn the_window_reopens() {
        let mut r = RateLimit::new(1, Duration::from_millis(30));
        assert!(r.allow("a") && !r.allow("a"));
        std::thread::sleep(Duration::from_millis(50));
        assert!(r.allow("a"));
    }
}
