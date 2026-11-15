//! Fixed-window counters, used by the push gateway (per token and per calling relay).

use axum::http::HeaderMap;
use std::collections::HashMap;
use std::net::SocketAddr;
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

/// The client address as a key. Behind a proxy you trust, `header` names the one it fills in (its last value counts:
/// that is the one the proxy added); without it, the address of the connection.
pub fn client_ip(headers: &HeaderMap, peer: SocketAddr, header: &Option<String>) -> String {
    header
        .as_deref()
        .and_then(|h| headers.get(h))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty() && v.len() <= 64)
        .unwrap_or_else(|| peer.ip().to_string())
}

/// Per client address: how many sockets have not authenticated yet, and how fast new ones are opened.
pub struct Ips {
    unauth: HashMap<String, usize>,
    opened: RateLimit,
    max_unauth: usize,
}

impl Ips {
    pub fn new(max_unauth: usize, per_min: u32) -> Ips {
        Ips { unauth: HashMap::new(), opened: RateLimit::new(per_min, Duration::from_secs(60)), max_unauth }
    }

    /// True when `ip` may open one more socket; it then holds an unauthenticated slot until `release`.
    pub fn admit(&mut self, ip: &str) -> bool {
        if !self.opened.allow(ip) {
            return false;
        }
        let n = self.unauth.entry(ip.to_owned()).or_insert(0);
        if *n >= self.max_unauth {
            return false;
        }
        *n += 1;
        true
    }

    pub fn release(&mut self, ip: &str) {
        if let Some(n) = self.unauth.get_mut(ip) {
            *n -= 1;
            if *n == 0 {
                self.unauth.remove(ip);
            }
        }
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
    fn unauthenticated_sockets_per_address_are_capped_and_freed() {
        let mut i = Ips::new(2, 100);
        assert!(i.admit("a") && i.admit("a") && !i.admit("a"));
        assert!(i.admit("b"));
        i.release("a");
        assert!(i.admit("a"));
    }

    #[test]
    fn the_client_address_comes_from_the_proxy_header_only_when_told_to() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "6.6.6.6, 1.2.3.4".parse().unwrap());
        let peer: SocketAddr = "10.0.0.1:5".parse().unwrap();
        assert_eq!(client_ip(&h, peer, &None), "10.0.0.1");
        assert_eq!(client_ip(&h, peer, &Some("x-forwarded-for".into())), "1.2.3.4", "the proxy's value, not the client's");
        assert_eq!(client_ip(&HeaderMap::new(), peer, &Some("x-forwarded-for".into())), "10.0.0.1");
    }

    #[test]
    fn the_window_reopens() {
        let mut r = RateLimit::new(1, Duration::from_millis(30));
        assert!(r.allow("a") && !r.allow("a"));
        std::thread::sleep(Duration::from_millis(50));
        assert!(r.allow("a"));
    }
}
