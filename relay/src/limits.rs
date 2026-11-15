//! Fixed-window counters, used by the push gateway (per token and per calling relay).

use axum::http::HeaderMap;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
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

/// The key an address is counted under: an IPv4 address as it is, an IPv6 address by its /64 (one host or one household
/// owns a whole /64, and can open sockets from as many addresses in it as it likes). An IPv4-mapped IPv6 address counts as
/// the IPv4 one. Anything else a proxy header holds stays as text.
pub fn ip_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let s = v6.segments();
                format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
            }
        },
    }
}

/// The client address as a key. Behind a proxy you trust, `header` names the one it fills in (its last value counts:
/// that is the one the proxy added); without it, the address of the connection. IPv6 counts by /64 (see [`ip_key`]).
pub fn client_ip(headers: &HeaderMap, peer: SocketAddr, header: &Option<String>) -> String {
    header
        .as_deref()
        .and_then(|h| headers.get(h))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty() && v.len() <= 64)
        .map(|v| v.parse::<IpAddr>().map(ip_key).unwrap_or(v))
        .unwrap_or_else(|| ip_key(peer.ip()))
}

/// The socket limits: per client address how many sockets have not authenticated yet, how many are open (authenticated
/// ones too) and how fast new ones are opened; and, over all addresses, how many are unauthenticated and how many are open.
/// The totals hold when an attacker owns many addresses (an IPv6 /64 is counted as one, but many networks are not).
pub struct Ips {
    unauth: HashMap<String, usize>,
    unauth_net: HashMap<String, usize>,
    max_unauth_net: usize,
    open: HashMap<String, usize>,
    opened: RateLimit,
    max_unauth: usize,
    max_open: usize,
    max_unauth_total: usize,
    max_open_total: usize,
    unauth_total: usize,
    open_total: usize,
}

/// The bounds `Ips` keeps (see `Config`).
#[derive(Clone, Copy)]
pub struct IpLimits {
    pub unauth_per_ip: usize,
    pub open_per_ip: usize,
    pub unauth_total: usize,
    pub open_total: usize,
    /// Unauthenticated sockets one IPv6 /48 (a whole site, 65536 /64s) may hold.
    pub unauth_per_net48: usize,
    pub per_min: u32,
}

/// The /48 an IPv6 key (`a:b:c:d::/64`) belongs to; none for an IPv4 or other key.
fn net48(key: &str) -> Option<String> {
    let rest = key.strip_suffix("::/64")?;
    let mut g = rest.split(':');
    Some(format!("{}:{}:{}::/48", g.next()?, g.next()?, g.next()?))
}

impl Ips {
    pub fn new(l: IpLimits) -> Ips {
        Ips {
            unauth: HashMap::new(),
            unauth_net: HashMap::new(),
            max_unauth_net: l.unauth_per_net48,
            open: HashMap::new(),
            opened: RateLimit::new(l.per_min, Duration::from_secs(60)),
            max_unauth: l.unauth_per_ip,
            max_open: l.open_per_ip,
            max_unauth_total: l.unauth_total,
            max_open_total: l.open_total,
            unauth_total: 0,
            open_total: 0,
        }
    }

    /// True when `ip` may open one more socket; it then holds an unauthenticated slot until `authenticated`, and an open slot
    /// until `release`.
    pub fn admit(&mut self, ip: &str) -> bool {
        if !self.opened.allow(ip) {
            return false;
        }
        if self.unauth_total >= self.max_unauth_total || self.open_total >= self.max_open_total {
            return false;
        }
        if self.unauth.get(ip).copied().unwrap_or(0) >= self.max_unauth || self.open.get(ip).copied().unwrap_or(0) >= self.max_open {
            return false;
        }
        if let Some(n) = net48(ip) {
            if self.unauth_net.get(&n).copied().unwrap_or(0) >= self.max_unauth_net {
                return false;
            }
            *self.unauth_net.entry(n).or_insert(0) += 1;
        }
        *self.unauth.entry(ip.to_owned()).or_insert(0) += 1;
        *self.open.entry(ip.to_owned()).or_insert(0) += 1;
        self.unauth_total += 1;
        self.open_total += 1;
        true
    }

    /// The socket proved its key: it no longer counts as unauthenticated (it still counts as open).
    pub fn authenticated(&mut self, ip: &str) {
        if Self::dec(&mut self.unauth, ip) {
            self.unauth_total -= 1;
            if let Some(n) = net48(ip) {
                Self::dec(&mut self.unauth_net, &n);
            }
        }
    }

    /// The socket is gone; `was_authenticated` says which slots it still held.
    pub fn release(&mut self, ip: &str, was_authenticated: bool) {
        if !was_authenticated {
            self.authenticated(ip);
        }
        if Self::dec(&mut self.open, ip) {
            self.open_total -= 1;
        }
    }

    fn dec(m: &mut HashMap<String, usize>, ip: &str) -> bool {
        let Some(n) = m.get_mut(ip) else { return false };
        *n -= 1;
        if *n == 0 {
            m.remove(ip);
        }
        true
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

    fn lim() -> IpLimits {
        IpLimits { unauth_per_ip: 2, open_per_ip: 3, unauth_total: 4, open_total: 5, unauth_per_net48: 3, per_min: 100 }
    }

    #[test]
    fn unauthenticated_sockets_per_address_are_capped_and_freed() {
        let mut i = Ips::new(lim());
        assert!(i.admit("a") && i.admit("a") && !i.admit("a"));
        assert!(i.admit("b"));
        i.release("a", false);
        assert!(i.admit("a"));
    }

    #[test]
    fn an_authenticated_socket_still_counts_as_open_for_its_address_and_in_total() {
        let mut i = Ips::new(lim());
        for _ in 0..3 {
            assert!(i.admit("a"));
            i.authenticated("a");
        }
        assert!(!i.admit("a"), "a fourth open socket from one address, though none is unauthenticated");
        assert!(i.admit("b") && i.admit("c"), "two more fill the total of five");
        assert!(!i.admit("d"), "no address is over its own cap, but the total is");
        i.release("a", true);
        assert!(i.admit("d"));
    }

    #[test]
    fn the_total_of_unauthenticated_sockets_holds_across_addresses() {
        let mut i = Ips::new(lim());
        assert!(i.admit("a") && i.admit("b") && i.admit("c") && i.admit("d"));
        assert!(!i.admit("e"), "four unauthenticated sockets from four addresses");
        i.authenticated("a");
        assert!(i.admit("e"));
    }

    #[test]
    fn one_ipv6_48_cannot_fill_the_total_of_unauthenticated_sockets() {
        let mut i = Ips::new(IpLimits { unauth_per_ip: 8, open_per_ip: 32, unauth_total: 100, open_total: 100, unauth_per_net48: 3, per_min: 1000 });
        let k = |n: u16| ip_key(format!("2001:db8:7:{n}::1").parse().unwrap());
        assert!(i.admit(&k(1)) && i.admit(&k(2)) && i.admit(&k(3)));
        assert!(!i.admit(&k(4)), "a fourth /64 of the same /48, each with room of its own");
        assert!(i.admit(&ip_key("2001:db8:8::1".parse().unwrap())), "another /48 is not affected");
        assert!(i.admit("192.0.2.1"), "nor an IPv4 address");
        i.authenticated(&k(2));
        assert!(i.admit(&k(4)), "a slot came back");
        i.release(&k(1), false);
        i.release(&k(3), false);
        assert!(i.admit(&k(5)) && i.admit(&k(6)));
    }

    #[test]
    fn an_ipv6_client_counts_by_its_64() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb:cccc:dddd:1".parse().unwrap();
        let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(ip_key(a), ip_key(b));
        assert_ne!(ip_key(a), ip_key(other));
        assert_eq!(ip_key("::ffff:192.0.2.7".parse().unwrap()), "192.0.2.7");
        assert_eq!(ip_key("192.0.2.7".parse().unwrap()), "192.0.2.7");
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "2001:db8:1:2:1234::9".parse().unwrap());
        let peer: SocketAddr = "10.0.0.1:5".parse().unwrap();
        assert_eq!(client_ip(&h, peer, &Some("x-forwarded-for".into())), ip_key(a), "also from a proxy header");
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
