//! Log lines hold device ids and counts, never a packet body, a push token or a credential. Callers pass plain
//! strings built from ids and numbers (never format a `Frame` with `{:?}`: its Debug output includes the body).
//! With `LOG_DIR` set the lines also go to one file per day, and files older than `log_days` are removed.

use std::io::Write;
use std::path::PathBuf;

const DAY_MS: u64 = 86_400_000;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Text from a device, made safe for one log line: escaped (no newline or control character survives) and cut.
pub fn quote(s: &str) -> String {
    let cut: String = s.chars().take(100).collect();
    format!("{cut:?}{}", if s.chars().count() > 100 { "…" } else { "" })
}

pub struct Logger {
    dir: Option<PathBuf>,
    keep_days: u64,
}

impl Logger {
    pub fn new(dir: Option<PathBuf>, keep_days: u64) -> Logger {
        let l = Logger { dir, keep_days };
        l.prune(now_ms());
        l
    }

    pub fn line(&self, text: &str) {
        let now = now_ms();
        eprintln!("{now} {text}");
        if let Some(d) = &self.dir {
            let _ = std::fs::create_dir_all(d);
            let name = d.join(format!("relay-{}.log", now / DAY_MS));
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(name) {
                let _ = writeln!(f, "{now} {text}");
            }
        }
    }

    /// Removes day files older than `keep_days` (the day number is in the file name).
    pub fn prune(&self, now: u64) {
        let Some(d) = &self.dir else { return };
        let today = now / DAY_MS;
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let day = name.strip_prefix("relay-").and_then(|n| n.strip_suffix(".log")).and_then(|n| n.parse::<u64>().ok());
            if day.is_some_and(|day| today.saturating_sub(day) > self.keep_days) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_text_cannot_forge_a_log_line() {
        let q = quote("wss://x/\n1794 connected deadbeef");
        assert!(!q.contains('\n') && q.contains("\\n"));
        assert!(quote(&"a".repeat(500)).chars().count() < 120);
    }

    #[test]
    fn old_day_files_are_removed_and_recent_ones_kept() {
        let dir = std::env::temp_dir().join(format!("relay-log-test-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let today = now_ms() / DAY_MS;
        for d in [today - 20, today - 14, today - 15, today] {
            std::fs::write(dir.join(format!("relay-{d}.log")), "x").unwrap();
        }
        Logger::new(Some(dir.clone()), 14);
        let mut left: Vec<u64> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_string_lossy().trim_start_matches("relay-").trim_end_matches(".log").parse().ok())
            .collect();
        left.sort();
        assert_eq!(left, vec![today - 14, today]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
