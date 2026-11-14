//! Devices a desktop revoked. One small JSON file per desktop (`<desktop id>.json`: device id → unix ms), kept
//! 90 days, read again at start so a restart forgets nothing.

use std::collections::HashMap;
use std::path::PathBuf;

const DAY_MS: u64 = 86_400_000;

pub struct Revoked {
    dir: Option<PathBuf>,
    days: u64,
    by: HashMap<String, HashMap<String, u64>>,
}

impl Revoked {
    pub fn load(dir: Option<PathBuf>, days: u64, now_ms: u64) -> Revoked {
        let mut r = Revoked { dir: dir.clone(), days, by: HashMap::new() };
        for e in dir.iter().flat_map(std::fs::read_dir).flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(desktop) = name.strip_suffix(".json") else { continue };
            let map: HashMap<String, u64> = std::fs::read_to_string(e.path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            r.by.insert(desktop.to_owned(), map);
        }
        r.prune(now_ms);
        r
    }

    /// Drops entries older than the retention and rewrites the files that changed.
    pub fn prune(&mut self, now_ms: u64) {
        let keep_after = now_ms.saturating_sub(self.days * DAY_MS);
        let changed: Vec<String> = self
            .by
            .iter_mut()
            .filter_map(|(d, m)| {
                let n = m.len();
                m.retain(|_, at| *at > keep_after);
                (m.len() != n).then(|| d.clone())
            })
            .collect();
        for d in changed {
            self.save(&d);
        }
    }

    pub fn add(&mut self, desktop: &str, id: &str, now_ms: u64) {
        self.by.entry(desktop.to_owned()).or_default().insert(id.to_owned(), now_ms);
        self.save(desktop);
    }

    pub fn is_revoked(&self, id: &str, now_ms: u64) -> bool {
        let keep_after = now_ms.saturating_sub(self.days * DAY_MS);
        self.by.values().any(|m| m.get(id).is_some_and(|at| *at > keep_after))
    }

    fn save(&self, desktop: &str) {
        let Some(dir) = &self.dir else { return };
        let _ = std::fs::create_dir_all(dir);
        let (path, tmp) = (dir.join(format!("{desktop}.json")), dir.join(format!("{desktop}.json.tmp")));
        if let Ok(text) = serde_json::to_string(&self.by[desktop]) {
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(tmp, path);
            }
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
        assert!(!r.is_revoked("phone-old", t0), "an entry past 90 days no longer counts, even before it is pruned");
        let r2 = Revoked::load(Some(dir.clone()), 90, t0);
        assert!(r2.is_revoked("phone-new", t0));
        assert!(!r2.is_revoked("phone-old", t0));
        assert!(!r2.is_revoked("someone", t0));
        let text = std::fs::read_to_string(dir.join("desk.json")).unwrap();
        assert!(!text.contains("phone-old"), "the pruned entry is gone from the file too");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
