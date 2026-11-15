// Named secrets for phone access (the desktop's keys, the key that seals the list of phones): the OS keychain (Keychain,
// Credential Manager, Secret Service), or an owner-only file where there is no keychain, or when BOBIX_DATA_DIR is set.
// The keychain is per user, not per folder: a dev, smoke or throwaway run must not read, replace or erase the owner's real
// identity, so it never touches the keychain. Nothing is read or created until a caller asks. No value appears in an error.
#![allow(dead_code)] // phones.rs is the caller
use std::{fs, io::Write, path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}};

const SERVICE: &str = "Bobix";

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    BadName,
    // The keychain exists but did not answer: locked, or access refused. Never answered with a file.
    Keychain(&'static str),
    File(&'static str),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Error::BadName => f.write_str("bad secret name"), Error::Keychain(m) => write!(f, "keychain: {m}"), Error::File(m) => write!(f, "secrets file: {m}") }
    }
}
pub type Result<T> = std::result::Result<T, Error>;

// Where the secrets are, for Settings to say.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Storage { Keychain, FileDataDir, FileNoKeychain }

pub trait Backend: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>>;
    fn set(&self, name: &str, value: &[u8]) -> Result<()>;
    fn delete(&self, name: &str) -> Result<()>; // absent is fine
}

pub struct Secrets { backend: Box<dyn Backend>, storage: Storage, fell_back: Option<&'static AtomicBool> }

impl Secrets {
    // The app's: a file inside BOBIX_DATA_DIR when that is set, else the OS keychain (a file only where there is none).
    // Under `cargo test` it is always a file in a folder of this process: no test can reach the OS keychain through it (a
    // test that wants the real keychain says so, like `real_keychain_round_trip`, which is ignored by default).
    #[cfg(test)]
    pub fn open() -> Secrets { Self::choose(Some(std::env::temp_dir().join(format!("bobix-test-secrets-{}", std::process::id()))), Box::new(Os { fallback: File { dir: PathBuf::new() } })) }
    #[cfg(not(test))]
    pub fn open() -> Secrets { Self::choose(std::env::var_os("BOBIX_DATA_DIR").map(PathBuf::from), Box::new(Os { fallback: File { dir: crate::paths::data_dir().join("secrets") } })) }
    fn choose(data_dir: Option<PathBuf>, os: Box<dyn Backend>) -> Secrets {
        match data_dir {
            Some(d) => Secrets { backend: Box::new(File { dir: d.join("secrets") }), storage: Storage::FileDataDir, fell_back: None },
            None => Secrets { backend: os, storage: Storage::Keychain, fell_back: Some(&NO_KEYCHAIN) },
        }
    }
    pub fn storage(&self) -> Storage { if self.fell_back.is_some_and(|f| f.load(Ordering::Relaxed)) { Storage::FileNoKeychain } else { self.storage } }
    pub fn get(&self, name: &str) -> Result<Option<Vec<u8>>> { ok_name(name)?; self.backend.get(name) }
    pub fn set(&self, name: &str, value: &[u8]) -> Result<()> { ok_name(name)?; self.backend.set(name, value) }
    pub fn delete(&self, name: &str) -> Result<()> { ok_name(name)?; self.backend.delete(name) }
}

fn ok_name(n: &str) -> Result<()> {
    if (1..=64).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')) && !n.starts_with('.') { Ok(()) } else { Err(Error::BadName) }
}

// Set once the OS has no keychain to offer; Settings then says the secrets are in a file.
static NO_KEYCHAIN: AtomicBool = AtomicBool::new(false);

// The OS keychain through the keyring crate. Where the crate finds no keychain at all (no Secret Service on a Linux
// session, an unsupported platform) the owner-only file is used and storage() says so. A keychain that exists but is
// locked or refuses is an error, never a quiet fall back to a file.
struct Os { fallback: File }
impl Os {
    fn entry(&self, name: &str) -> Option<Result<keyring::Entry>> {
        if keyring::Entry::store_status().is_err() { NO_KEYCHAIN.store(true, Ordering::Relaxed); return None; }
        Some(keyring::Entry::new(SERVICE, name).map_err(kc))
    }
}
fn kc(e: keyring::Error) -> Error {
    Error::Keychain(match e {
        keyring::Error::NoStorageAccess(_) => "the keychain is locked or access was refused",
        keyring::Error::PlatformFailure(_) => "the keychain failed",
        keyring::Error::NoDefaultStore => "no keychain",
        _ => "the keychain refused the entry",
    })
}
impl Backend for Os {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let Some(e) = self.entry(name) else { return self.fallback.get(name) };
        match e?.get_secret() { Ok(v) => Ok(Some(v)), Err(keyring::Error::NoEntry) => Ok(None), Err(e) => Err(kc(e)) }
    }
    fn set(&self, name: &str, value: &[u8]) -> Result<()> {
        let Some(e) = self.entry(name) else { return self.fallback.set(name, value) };
        e?.set_secret(value).map_err(kc)
    }
    fn delete(&self, name: &str) -> Result<()> {
        let Some(e) = self.entry(name) else { return self.fallback.delete(name) };
        match e?.delete_credential() { Ok(()) | Err(keyring::Error::NoEntry) => Ok(()), Err(e) => Err(kc(e)) }
    }
}

// One file per secret in a folder only this user can open (0700, files 0600). On Windows the folder inherits the user's
// profile permissions, which other users cannot read by default.
struct File { dir: PathBuf }
impl File {
    fn path(&self, name: &str) -> PathBuf { self.dir.join(name) }
}
fn io(_: std::io::Error) -> Error { Error::File("could not read or write the file") }
impl Backend for File {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        match fs::read(self.path(name)) { Ok(v) => Ok(Some(v)), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None), Err(e) => Err(io(e)) }
    }
    fn set(&self, name: &str, value: &[u8]) -> Result<()> {
        fs::create_dir_all(&self.dir).map_err(io)?;
        #[cfg(unix)]
        { use std::os::unix::fs::PermissionsExt; fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700)).map_err(io)?; }
        let tmp = self.path(&format!("{name}.tmp"));
        let mut f = open_private(&tmp).map_err(io)?;
        f.write_all(value).and_then(|_| f.sync_all()).map_err(io)?;
        fs::rename(&tmp, self.path(name)).map_err(io) // a crash leaves the old value, not half of the new one
    }
    fn delete(&self, name: &str) -> Result<()> {
        match fs::remove_file(self.path(name)) { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(e) => Err(io(e)) }
    }
}
fn open_private(p: &Path) -> std::io::Result<fs::File> {
    let mut o = fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    { use std::os::unix::fs::OpenOptionsExt; o.mode(0o600); }
    o.open(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // Records every call; the tests' stand-in for the OS keychain, so a test never touches the developer's own.
    #[derive(Clone, Default)]
    struct Fake { calls: Arc<Mutex<Vec<String>>>, store: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>, locked: bool }
    impl Backend for Fake {
        fn get(&self, n: &str) -> Result<Option<Vec<u8>>> { self.calls.lock().unwrap().push(format!("get {n}")); if self.locked { return Err(Error::Keychain("the keychain is locked or access was refused")) } Ok(self.store.lock().unwrap().get(n).cloned()) }
        fn set(&self, n: &str, v: &[u8]) -> Result<()> { self.calls.lock().unwrap().push(format!("set {n}")); if self.locked { return Err(Error::Keychain("locked")) } self.store.lock().unwrap().insert(n.into(), v.to_vec()); Ok(()) }
        fn delete(&self, n: &str) -> Result<()> { self.calls.lock().unwrap().push(format!("delete {n}")); self.store.lock().unwrap().remove(n); Ok(()) }
    }
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bobix-secrets-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_secret_is_written_read_replaced_and_deleted() {
        let f = Fake::default();
        let s = Secrets::choose(None, Box::new(f.clone()));
        assert_eq!(s.get("identity").unwrap(), None, "never written reads as absent, not as an error");
        s.set("identity", b"one").unwrap();
        assert_eq!(s.get("identity").unwrap().as_deref(), Some(&b"one"[..]));
        s.set("identity", b"two").unwrap();
        assert_eq!(s.get("identity").unwrap().as_deref(), Some(&b"two"[..]));
        s.delete("identity").unwrap();
        assert_eq!(s.get("identity").unwrap(), None);
        s.delete("identity").unwrap(); // deleting twice is fine
        assert_eq!(s.storage(), Storage::Keychain);
    }

    #[test]
    fn with_a_data_folder_the_keychain_is_never_called_and_the_file_is_owner_only() {
        let (f, d) = (Fake::default(), scratch("dir"));
        let s = Secrets::choose(Some(d.clone()), Box::new(f.clone()));
        assert_eq!(s.get("k").unwrap(), None);
        s.set("k", b"value").unwrap();
        assert_eq!(s.get("k").unwrap().as_deref(), Some(&b"value"[..]), "a second read returns the same value");
        s.set("k", b"other").unwrap();
        assert_eq!(s.get("k").unwrap().as_deref(), Some(&b"other"[..]));
        s.delete("k").unwrap();
        assert_eq!(s.get("k").unwrap(), None);
        assert!(f.calls.lock().unwrap().is_empty(), "the recording keychain saw no call");
        assert_eq!(s.storage(), Storage::FileDataDir);
        s.set("k", b"v").unwrap();
        #[cfg(unix)]
        { use std::os::unix::fs::PermissionsExt;
          assert_eq!(fs::metadata(d.join("secrets/k")).unwrap().permissions().mode() & 0o777, 0o600);
          assert_eq!(fs::metadata(d.join("secrets")).unwrap().permissions().mode() & 0o777, 0o700); }
        assert!(!d.join("secrets/k.tmp").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_locked_keychain_is_an_error_not_a_file() {
        let (f, d) = (Fake { locked: true, ..Default::default() }, scratch("locked"));
        let s = Secrets::choose(None, Box::new(f));
        let e = s.get("k").unwrap_err();
        assert!(matches!(e, Error::Keychain(_)) && e.to_string().contains("locked"));
        assert!(s.set("k", b"secret-value").is_err());
        assert!(!d.exists() && !crate::paths::data_dir().join("secrets/k").exists(), "nothing was written to a file instead");
    }

    #[test]
    fn open_never_reaches_the_os_keychain_under_test() {
        let s = Secrets::open();
        assert_eq!(s.storage(), Storage::FileDataDir);
        s.set("probe", b"x").unwrap();
        assert_eq!(s.get("probe").unwrap().as_deref(), Some(&b"x"[..]));
        s.delete("probe").unwrap();
        assert!(!NO_KEYCHAIN.load(Ordering::Relaxed), "the keychain fallback flag is untouched");
    }

    #[test]
    fn names_are_plain_and_errors_carry_no_value() {
        let s = Secrets::choose(Some(scratch("names")), Box::new(Fake::default()));
        for bad in ["", "../x", "a/b", ".hidden", "UPPER", &"a".repeat(65)] { assert_eq!(s.get(bad), Err(Error::BadName), "{bad}"); }
        // a file that cannot be written (the folder is a file): the error names no value
        let blocker = scratch("blocker");
        fs::write(&blocker, b"x").unwrap();
        let s = Secrets::choose(Some(blocker.clone()), Box::new(Fake::default()));
        let e = s.set("k", b"TOP-SECRET-VALUE").unwrap_err();
        assert!(!format!("{e} {e:?}").contains("TOP-SECRET-VALUE"));
        let _ = fs::remove_file(&blocker);
    }

    // Against the real OS keychain, under a throwaway name that is deleted at the end. By hand:
    //   cargo test secrets::tests::real_keychain -- --ignored
    #[test]
    #[ignore]
    fn real_keychain_round_trip() {
        let os = Os { fallback: File { dir: scratch("real") } };
        let name = format!("bobix-test-{}", std::process::id());
        assert_eq!(os.get(&name).unwrap(), None);
        os.set(&name, b"one").unwrap();
        os.set(&name, b"two").unwrap();
        assert_eq!(os.get(&name).unwrap().as_deref(), Some(&b"two"[..]));
        if std::env::var_os("KEEP_TEST_SECRET").is_some() { eprintln!("left {name}"); return; }
        os.delete(&name).unwrap();
        assert_eq!(os.get(&name).unwrap(), None);
        assert!(!NO_KEYCHAIN.load(Ordering::Relaxed), "this machine has a keychain");
    }
}
