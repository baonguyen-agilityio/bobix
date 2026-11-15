//! The file the app and the notification extension share: one receive state per paired computer, and the
//! "Hide details" switch. They are two processes, so every access is read, change, write under one exclusive `flock`
//! on a sibling lock file, and nothing is cached between accesses: a copy kept in memory would write back what the
//! other process already changed and undo its commit.

use bobix_link::counters::RecvState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Doc {
    pub hide_details: bool,
    pub computers: BTreeMap<String, Computer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Computer {
    /// The computer's box public key, URL-safe base64.
    pub peer: String,
    pub version_min: u8,
    pub version_max: u8,
    pub recv: RecvState,
}

#[derive(Debug)]
pub enum StoreError {
    Io(String),
    /// The file is not what this version writes. Never overwritten: the caller reports it.
    Corrupt(String),
}

fn io<E: std::fmt::Display>(e: E) -> StoreError {
    StoreError::Io(e.to_string())
}

/// Runs `f` on the stored document with the lock held. `f` returns the result and whether the document changed.
pub fn with_doc<T>(path: &Path, f: impl FnOnce(&mut Doc) -> (T, bool)) -> Result<T, StoreError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    let lock_path = sibling(path, ".lock");
    let lock = OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path).map_err(io)?;
    lock.lock().map_err(io)?;
    let mut doc = read(path)?;
    let (out, changed) = f(&mut doc);
    if changed {
        write(path, &doc)?;
    }
    // Closing the file releases the lock.
    drop(lock);
    Ok(out)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn read(path: &Path) -> Result<Doc, StoreError> {
    match fs::read(path) {
        Ok(bytes) if bytes.is_empty() => Ok(Doc::default()),
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Doc::default()),
        Err(e) => Err(io(e)),
    }
}

/// Written beside the file and renamed over it, so a reader that does not take the lock never sees half a file.
fn write(path: &Path, doc: &Doc) -> Result<(), StoreError> {
    let tmp = sibling(path, ".tmp");
    let mut f = File::create(&tmp).map_err(io)?;
    f.write_all(&serde_json::to_vec(doc).map_err(io)?).map_err(io)?;
    f.sync_all().map_err(io)?;
    fs::rename(&tmp, path).map_err(io)
}
