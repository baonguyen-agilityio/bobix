//! Counters: three streams per direction of a pair, each with its own send counter and its own receive rule.
//! Pure logic, no file I/O: the caller persists what `SendCounter::next` asks for and `RecvState` (serde).

use crate::envelope::Stream;
use serde::{Deserialize, Serialize};

/// How far ahead of the last counter used the sender reserves in its store.
pub const RESERVE_BLOCK: u64 = 1000;
/// The `event` stream accepts a late counter this far below the highest seen.
pub const EVENT_WINDOW: u64 = 1024;
const WORDS: usize = (EVENT_WINDOW / 64) as usize;

/// A send counter. Before a packet with counter `c` leaves, `next` may ask the caller to store a new reservation
/// (`c + RESERVE_BLOCK`); the caller must store it first. After a restart, `resume` skips ahead to the stored
/// reservation, and never starts below the clock, so a lost counter file cannot send the counter backwards.
#[derive(Debug)]
pub struct SendCounter {
    next: u64,
    stored: u64,
}

impl SendCounter {
    pub fn resume(stored_reservation: u64, now_unix_ms: u64) -> Self {
        SendCounter { next: stored_reservation.max(now_unix_ms), stored: stored_reservation }
    }

    /// The counter to use, and `Some(value)` when `value` must be stored before the packet leaves.
    pub fn take(&mut self) -> (u64, Option<u64>) {
        let c = self.next;
        self.next += 1;
        if c >= self.stored {
            self.stored = c + RESERVE_BLOCK;
            (c, Some(self.stored))
        } else {
            (c, None)
        }
    }
}

/// What a receiver has seen on one stream.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamState {
    highest: Option<u64>,
    /// Bit `c % 1024` is set when counter `c`, inside the window below `highest`, was accepted.
    seen: Vec<u64>,
}

impl StreamState {
    fn bit(&self, c: u64) -> bool {
        let i = (c % EVENT_WINDOW) as usize;
        self.seen.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 == 1)
    }

    fn set(&mut self, c: u64, on: bool) {
        if self.seen.is_empty() {
            self.seen = vec![0; WORDS];
        }
        let i = (c % EVENT_WINDOW) as usize;
        if on {
            self.seen[i / 64] |= 1 << (i % 64);
        } else {
            self.seen[i / 64] &= !(1 << (i % 64));
        }
    }

    /// Whether `c` passes the rule of `stream`. Does not change anything: call `commit` after the packet opened.
    pub fn check(&self, stream: Stream, c: u64) -> bool {
        let Some(high) = self.highest else { return true };
        match stream {
            Stream::State => c > high,
            Stream::Activity => c >= high,
            Stream::Event => c > high || (c < high && high - c < EVENT_WINDOW && !self.bit(c)),
        }
    }

    pub fn commit(&mut self, stream: Stream, c: u64) {
        match (stream, self.highest) {
            (Stream::Event, Some(high)) if c <= high => self.set(c, true),
            (Stream::Event, high) => {
                // The window moves up to `c`: forget the counters that fall out of it.
                let from = high.map_or(0, |h| h + 1);
                let gap = c - from.min(c);
                if gap >= EVENT_WINDOW {
                    self.seen = vec![0; WORDS];
                } else {
                    for k in from..c {
                        self.set(k, false);
                    }
                }
                self.set(c, true);
                self.highest = Some(c);
            }
            (_, high) => self.highest = Some(high.map_or(c, |h| h.max(c))),
        }
    }
}

/// The receive state of one direction of a pair: one `StreamState` per stream.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecvState {
    state: StreamState,
    event: StreamState,
    activity: StreamState,
}

impl RecvState {
    fn of(&self, s: Stream) -> &StreamState {
        match s {
            Stream::State => &self.state,
            Stream::Event => &self.event,
            Stream::Activity => &self.activity,
        }
    }

    pub fn check(&self, s: Stream, c: u64) -> bool {
        self.of(s).check(s, c)
    }

    pub fn commit(&mut self, s: Stream, c: u64) {
        match s {
            Stream::State => self.state.commit(s, c),
            Stream::Event => self.event.commit(s, c),
            Stream::Activity => self.activity.commit(s, c),
        }
    }
}
