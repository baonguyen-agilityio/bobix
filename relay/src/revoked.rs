//! What survives a restart, one small JSON file per desktop (names are authenticated device ids):
//! `<desktop id>.json` holds the devices it revoked (device id → unix ms), `<desktop id>.paired` the devices that
//! answered one of its pairing tokens (the only ones it may revoke). Both are kept 90 days.

use std::collections::HashMap;
use std::path::PathBuf;

const DAY_MS: u64 = 86_400_000;
/// A desktop remembers at most this many paired devices.
const MAX_PAIRED: usize = 1024;

type Dated = HashMap<String, HashMap<String, u64>>;

pub struct Revoked {
    dir: Option<PathBuf>,
    days: u64,
    by: Dated,
    paired: Dated,
}

fn read(dir: &Option<PathBuf>, ext: &str, into: &mut Dated) {
    for e in dir.iter().flat_map(std::fs::read_dir).flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(desktop) = name.strip_suffix(ext) else { continue };
        let map = std::fs::read_to_string(e.path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        into.insert(desktop.to_owned(), map);
    }
}

impl Revoked {
    pub fn load(dir: Option<PathBuf>, days: u64, now_ms: u64) -> Revoked {
        let mut r = Revoked { dir: dir.clone(), days, by: HashMap::new(), paired: HashMap::new() };
        read(&dir, ".json", &mut r.by);
        read(&dir, ".paired", &mut r.paired);
        r.prune(now_ms);
        r
    }

    fn keep_after(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.days * DAY_MS)
    }

    /// Drops entries older than the retention and rewrites the files that changed.
    pub fn prune(&mut self, now_ms: u64) {
        let after = self.keep_after(now_ms);
        for (map, ext) in [(&mut self.by, ".json"), (&mut self.paired, ".paired")] {
            let changed: Vec<String> = map
                .iter_mut()
                .filter_map(|(d, m)| {
                    let n = m.len();
                    m.retain(|_, at| *at > after);
                    (m.len() != n).then(|| d.clone())
                })
                .collect();
            for d in changed {
                save(&self.dir, &d, ext, &map[&d]);
            }
        }
    }

    pub fn add(&mut self, desktop: &str, id: &str, now_ms: u64) {
        self.by.entry(desktop.to_owned()).or_default().insert(id.to_owned(), now_ms);
        save(&self.dir, desktop, ".json", &self.by[desktop]);
    }

    /// A revocation cuts the path between one desktop and one device, whichever way the question is asked.
    pub fn is_cut(&self, a: &str, b: &str, now_ms: u64) -> bool {
        let after = self.keep_after(now_ms);
        let live = |d: &str, x: &str| self.by.get(d).and_then(|m| m.get(x)).is_some_and(|at| *at > after);
        live(a, b) || live(b, a)
    }

    /// Some desktop has revoked `id` (it is told so when it connects).
    pub fn cut_by_any(&self, id: &str, now_ms: u64) -> bool {
        let after = self.keep_after(now_ms);
        self.by.values().any(|m| m.get(id).is_some_and(|at| *at > after))
    }

    /// The desktop opened a fresh pairing and `id` answered it: the old cut no longer applies.
    pub fn lift(&mut self, desktop: &str, id: &str) {
        if self.by.get_mut(desktop).is_some_and(|m| m.remove(id).is_some()) {
            save(&self.dir, desktop, ".json", &self.by[desktop]);
        }
    }

    /// `id` sent a packet that `desktop` had allowed: it treats the desktop as paired (a pairing it confirmed).
    /// Only such a device can be revoked by the desktop. Written when new, and again at most once a day.
    pub fn note_paired(&mut self, desktop: &str, id: &str, now_ms: u64) {
        let m = self.paired.entry(desktop.to_owned()).or_default();
        if m.get(id).is_some_and(|at| now_ms.saturating_sub(*at) < DAY_MS) || (m.len() >= MAX_PAIRED && !m.contains_key(id)) {
            return;
        }
        m.insert(id.to_owned(), now_ms);
        save(&self.dir, desktop, ".paired", &self.paired[desktop]);
    }

    pub fn has_paired(&self, desktop: &str, id: &str, now_ms: u64) -> bool {
        let after = self.keep_after(now_ms);
        self.paired.get(desktop).and_then(|m| m.get(id)).is_some_and(|at| *at > after)
    }
}

fn save(dir: &Option<PathBuf>, desktop: &str, ext: &str, map: &HashMap<String, u64>) {
    let Some(dir) = dir else { return };
    let _ = std::fs::create_dir_all(dir);
    let (path, tmp) = (dir.join(format!("{desktop}{ext}")), dir.join(format!("{desktop}{ext}.tmp")));
    if let Ok(text) = serde_json::to_string(map) {
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survives_a_restart_and_forgets_after_90_days() {
        let dir = std::env::temp_dir().join(format!("relay-revoked-test-{}", crate::log::now_ms()));
        let t0 = 1_000 * DAY_MS;
        let mut r = Revoked::load(Some(dir.clone()), 90, t0);
        r.add("desk", "phone-old", t0 - 91 * DAY_MS);
        r.add("desk", "phone-new", t0 - DAY_MS);
        r.note_paired("desk", "phone-new", t0 - DAY_MS);
        r.note_paired("desk", "phone-old", t0 - 91 * DAY_MS);
        assert!(!r.is_cut("desk", "phone-old", t0), "an entry past 90 days no longer counts, even before it is pruned");
        let r2 = Revoked::load(Some(dir.clone()), 90, t0);
        assert!(r2.is_cut("desk", "phone-new", t0) && r2.is_cut("phone-new", "desk", t0), "the cut holds both ways");
        assert!(!r2.is_cut("desk", "phone-old", t0));
        assert!(!r2.is_cut("desk", "someone", t0) && !r2.is_cut("other-desk", "phone-new", t0), "only that pair is cut");
        assert!(r2.cut_by_any("phone-new", t0) && !r2.cut_by_any("someone", t0));
        assert!(r2.has_paired("desk", "phone-new", t0), "who paired survives a restart");
        assert!(!r2.has_paired("desk", "phone-old", t0) && !r2.has_paired("desk", "stranger", t0));
        let text = std::fs::read_to_string(dir.join("desk.json")).unwrap();
        assert!(!text.contains("phone-old"), "the pruned entry is gone from the file too");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
