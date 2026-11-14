//! One recipient's queue of sealed packets. A packet stays until the recipient acknowledges it or its ttl ends.
//! The relay never looks inside `body`.

use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone)]
pub struct Packet {
    pub pid: String,
    pub from: String,
    pub body: String,
    pub expires: Instant,
    pub collapse: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Put {
    /// Accepted. `replaced` are the older packets with the same sender and collapse key that it took the place of.
    Queued { replaced: Vec<String> },
    /// The sender already queued this pid (a retry): nothing changed.
    Again,
    /// 64 packets or 2 MiB would be exceeded, or another sender holds this pid.
    Refused,
}

pub struct Mailbox {
    packets: VecDeque<Packet>,
    bytes: usize,
    max_packets: usize,
    max_bytes: usize,
}

impl Mailbox {
    pub fn new(max_packets: usize, max_bytes: usize) -> Self {
        Mailbox { packets: VecDeque::new(), bytes: 0, max_packets, max_bytes }
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Packet> {
        self.packets.iter()
    }

    pub fn put(&mut self, p: Packet) -> Put {
        if let Some(old) = self.packets.iter().find(|x| x.pid == p.pid) {
            return if old.from == p.from { Put::Again } else { Put::Refused };
        }
        let same = |x: &Packet| p.collapse.is_some() && x.from == p.from && x.collapse == p.collapse;
        let freed = self.packets.iter().filter(|x| same(x)).fold((0, 0), |(n, b), x| (n + 1, b + x.body.len()));
        if self.packets.len() - freed.0 >= self.max_packets || self.bytes - freed.1 + p.body.len() > self.max_bytes {
            return Put::Refused;
        }
        let replaced = self.drain_where(same).into_iter().map(|x| x.pid).collect();
        self.bytes += p.body.len();
        self.packets.push_back(p);
        Put::Queued { replaced }
    }

    pub fn ack(&mut self, pid: &str) -> Option<Packet> {
        self.take(|p| p.pid == pid)
    }

    /// Only the sender of a packet may take it back.
    pub fn withdraw(&mut self, from: &str, pid: &str) -> Option<Packet> {
        self.take(|p| p.pid == pid && p.from == from)
    }

    fn take(&mut self, f: impl Fn(&Packet) -> bool) -> Option<Packet> {
        let i = self.packets.iter().position(f)?;
        let p = self.packets.remove(i)?;
        self.bytes -= p.body.len();
        Some(p)
    }

    /// Removes and returns the packets `f` says to drop.
    pub fn drain_where(&mut self, f: impl Fn(&Packet) -> bool) -> Vec<Packet> {
        let (mut keep, mut gone) = (VecDeque::new(), Vec::new());
        for p in self.packets.drain(..) {
            if f(&p) {
                gone.push(p);
            } else {
                keep.push_back(p);
            }
        }
        self.packets = keep;
        self.bytes = self.packets.iter().map(|p| p.body.len()).sum();
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pk(pid: &str, from: &str, collapse: Option<&str>, n: usize) -> Packet {
        Packet { pid: pid.into(), from: from.into(), body: "x".repeat(n), expires: Instant::now() + Duration::from_secs(60), collapse: collapse.map(Into::into) }
    }

    #[test]
    fn collapse_replaces_same_sender_and_key_only() {
        let mut m = Mailbox::new(64, 1 << 20);
        m.put(pk("1", "a", Some("home"), 1));
        m.put(pk("2", "a", Some("other"), 1));
        m.put(pk("3", "b", Some("home"), 1));
        assert_eq!(m.put(pk("4", "a", Some("home"), 1)), Put::Queued { replaced: vec!["1".into()] });
        assert_eq!(m.iter().map(|p| p.pid.as_str()).collect::<Vec<_>>(), ["2", "3", "4"]);
    }

    #[test]
    fn the_65th_packet_and_the_byte_cap_are_refused() {
        let mut m = Mailbox::new(64, 100);
        for i in 0..64 {
            assert!(matches!(m.put(pk(&i.to_string(), "a", None, 1)), Put::Queued { .. }));
        }
        assert_eq!(m.put(pk("64", "a", None, 1)), Put::Refused);
        let mut m = Mailbox::new(64, 100);
        assert!(matches!(m.put(pk("1", "a", None, 60)), Put::Queued { .. }));
        assert_eq!(m.put(pk("2", "a", None, 41)), Put::Refused);
        assert!(matches!(m.put(pk("3", "a", None, 40)), Put::Queued { .. }));
    }

    #[test]
    fn only_the_sender_withdraws_and_a_retry_changes_nothing() {
        let mut m = Mailbox::new(64, 1 << 20);
        m.put(pk("1", "a", None, 1));
        assert_eq!(m.put(pk("1", "a", None, 1)), Put::Again);
        assert_eq!(m.put(pk("1", "b", None, 1)), Put::Refused);
        assert!(m.withdraw("b", "1").is_none());
        assert!(m.withdraw("a", "1").is_some());
        assert!(m.is_empty());
    }
}
