//! The computers that got in by redeeming an invite code, and the codes already spent, in one JSON file
//! (`INVITED_FILE`, the image puts it at `/data/invited.json`). Written whole on every change: a temporary file,
//! flushed to disk, then renamed over the old one, so a crash leaves the old list or the new one, never half of one.
//! A spent code's nonce is kept until the code would have expired, so a restart never makes a used code work again.

use bobix_link::relay_frames::Invitee;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// At most this many computers get in by a code.
pub const MAX_INVITEES: usize = 1000;
/// A spent nonce is kept this long past its code's expiry, so a clock that jumps forward and back never frees it early.
const KEEP_PAST_EXPIRY_MS: u64 = 86_400_000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Entry {
    #[serde(default)]
    label: String,
    by: String,
    at: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    v: u8,
    #[serde(default)]
    invitees: BTreeMap<String, Entry>,
    /// Nonce (URL-safe base64) → the unix ms its code expires at.
    #[serde(default)]
    used: HashMap<String, u64>,
}

pub struct Invited {
    path: Option<PathBuf>,
    data: Saved,
    /// Whether the file could be written, the last time it was (or, before any change, at start). `/healthz` reads this
    /// and never touches the disk itself.
    writable: bool,
}

/// What a redemption came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Redeemed {
    Added,
    /// The nonce was spent before.
    Used,
    TooMany,
    /// The file could not be written: nothing changed.
    NotSaved,
}

impl Invited {
    /// No file yet is an empty list. A file that cannot be read or parsed is an error: starting with an empty list would
    /// drop every tester and forget which codes were spent.
    pub fn load(path: Option<PathBuf>, now_ms: u64) -> Result<Invited, String> {
        let data = match &path {
            None => Saved::default(),
            Some(p) => match std::fs::read(p) {
                Ok(b) => serde_json::from_slice::<Saved>(&b).ok().filter(|s| s.v == 1).ok_or("INVITED_FILE is not a list of invited computers this relay can read (fix or move the file; see the relay README)")?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Saved::default(),
                Err(e) => return Err(format!("cannot read INVITED_FILE: {e}")),
            },
        };
        let writable = probe(path.as_deref());
        let mut i = Invited { path, data, writable };
        i.data.v = 1;
        i.forget_spent(now_ms);
        Ok(i)
    }

    fn forget_spent(&mut self, now_ms: u64) {
        self.data.used.retain(|_, until| until.saturating_add(KEEP_PAST_EXPIRY_MS) > now_ms);
    }

    pub fn contains(&self, id: &str) -> bool {
        self.data.invitees.contains_key(id)
    }

    pub fn list(&self) -> Vec<Invitee> {
        self.data.invitees.iter().map(|(id, e)| Invitee { id: id.clone(), label: e.label.clone(), by: e.by.clone(), at: e.at }).collect()
    }

    /// Admits `id` with a code (its nonce and expiry), unless the nonce was spent or the list is full. Saved before it counts.
    pub fn redeem(&mut self, id: &str, nonce: &str, expires_ms: u64, label: &str, by: &str, now_ms: u64) -> Redeemed {
        self.forget_spent(now_ms);
        if self.data.used.contains_key(nonce) {
            return Redeemed::Used;
        }
        if self.data.invitees.len() >= MAX_INVITEES {
            return Redeemed::TooMany;
        }
        self.data.used.insert(nonce.to_owned(), expires_ms);
        self.data.invitees.insert(id.to_owned(), Entry { label: label.to_owned(), by: by.to_owned(), at: now_ms });
        if self.save().is_err() {
            self.data.used.remove(nonce);
            self.data.invitees.remove(id);
            return Redeemed::NotSaved;
        }
        Redeemed::Added
    }

    /// False when `id` did not get in by a code, or the file could not be written (then it stays).
    pub fn remove(&mut self, id: &str) -> bool {
        let Some(e) = self.data.invitees.remove(id) else { return false };
        if self.save().is_err() {
            self.data.invitees.insert(id.to_owned(), e);
            return false;
        }
        true
    }

    /// Called with the hub's lock held: the write and its fsync block other frames for a moment. That is accepted: only an
    /// owner's removal or a valid code (5 a minute per socket, at most 1,000 in all) gets here, and the file is small.
    fn save(&mut self) -> std::io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let r = serde_json::to_vec(&self.data).map_err(std::io::Error::other).and_then(|b| write_atomic(path, &b));
        self.writable = r.is_ok();
        r
    }

    /// For `/healthz`: the last write worked (or, before the first one, the folder took a file at start).
    pub fn writable(&self) -> bool {
        self.writable
    }
}

/// At start only: the file's folder can take a new file.
fn probe(path: Option<&Path>) -> bool {
    let Some(dir) = path.and_then(|p| p.parent()) else { return true };
    let probe = dir.join(".invited-probe");
    let ok = std::fs::create_dir_all(dir).is_ok() && std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(probe);
    ok
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("json.tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    // The rename itself is on disk only once the directory is.
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spent_nonces_are_forgotten_once_their_code_expired() {
        let mut i = Invited::load(None, 0).unwrap();
        assert_eq!(i.redeem("a", "n1", 100, "", "o", 10), Redeemed::Added);
        assert_eq!(i.redeem("b", "n1", 100, "", "o", 50), Redeemed::Used);
        // Kept a day past the code's expiry (a clock that jumped forward and back), then forgotten.
        assert_eq!(i.redeem("b", "n1", 100, "", "o", 100 + KEEP_PAST_EXPIRY_MS - 1), Redeemed::Used);
        assert_eq!(i.redeem("b", "n1", 300, "", "o", 100 + KEEP_PAST_EXPIRY_MS), Redeemed::Added);
        assert!(i.contains("a") && i.contains("b"));
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_written_changes_nothing() {
        // A directory nobody may write to (as root this test proves nothing, so it stops early).
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("relay-invited-ro-{}", bobix_link::hex(&bobix_link::random::<6>())));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o555)).unwrap();
        let mut i = Invited::load(Some(base.join("invited.json")), 0).unwrap();
        if i.writable() {
            return std::fs::remove_dir(base).unwrap();
        }
        assert_eq!(i.redeem("a", "n", 100, "", "o", 1), Redeemed::NotSaved);
        assert!(!i.contains("a"));
        assert_eq!(i.redeem("a", "n", 100, "", "o", 1), Redeemed::NotSaved, "the nonce was not spent");
        std::fs::remove_dir(base).unwrap();
    }
}
