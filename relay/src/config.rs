//! Configuration: environment variables only. Credentials are read from files the variables name.

use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub dev: bool,
    pub addr: String,
    /// This relay's own address as devices know it (`RELAY_PUBLIC_URL`, e.g. `wss://relay.example`): an `auth`
    /// signature must be made for it. `--dev` derives it from the bind address.
    pub public_url: String,
    /// The id a phone's push grant must name (`RELAY_ID`).
    pub relay_id: String,
    pub invite_file: Option<PathBuf>,
    /// Device ids from `INVITE_IDS`, read once at start; invited in addition to the ones in `invite_file`.
    pub invite_ids: Vec<String>,
    pub gateway_keys_file: Option<PathBuf>,
    /// One small file per desktop. `None` keeps the list in memory only (`--dev`).
    pub revoked_dir: Option<PathBuf>,
    pub log_dir: Option<PathBuf>,
    pub apns: Option<ApnsConfig>,
    pub fcm: Option<FcmConfig>,
    /// A relay that has no APNs / FCM credentials of its own pushes through this gateway.
    pub gateway_url: Option<String>,
    pub gateway_key_file: Option<PathBuf>,
    // Durations and sizes live here so tests can shrink them.
    pub auth_within: Duration,
    pub ping_every: Duration,
    pub dead_after: Duration,
    pub sweep_every: Duration,
    pub max_ttl_secs: u32,
    pub pair_ttl_secs: u32,
    pub max_pair_uses: u8,
    pub max_open_pairs: usize,
    pub revoked_days: u64,
    pub log_days: u64,
    pub mailbox_packets: usize,
    pub mailbox_bytes: usize,
    /// Outbound frames queued for one socket; a socket whose queue is full is closed.
    pub out_queue: usize,
    /// Frames one device may send per second; more closes the socket.
    pub device_frames_per_sec: u32,
    /// Unauthenticated sockets one client address may hold, and new connections it may open per minute.
    pub max_unauth_per_ip: usize,
    pub conns_per_ip_per_min: u32,
    /// Sockets one client address may hold open in all (authenticated ones too), and the bounds over all addresses: sockets
    /// that have not authenticated, and sockets open at all. An IPv6 address counts by its /64.
    pub max_open_per_ip: usize,
    pub max_unauth_total: usize,
    /// Unauthenticated sockets one IPv6 /48 may hold (so one site cannot fill `max_unauth_total`).
    pub max_unauth_per_net48: usize,
    pub max_open_total: usize,
    /// With invitations on, how long a guest has to name an invited computer in `peers` before the socket is closed.
    pub guest_within: Duration,
    /// How long the allow-list and push tokens of a device that left are kept.
    pub state_keep: Duration,
    /// Global bounds: mailboxes that exist at once, and bytes held in all of them.
    pub max_mailboxes: usize,
    pub max_total_bytes: usize,
    /// Pushes through this relay's own credentials: per token and per sending device, per `gateway_window`.
    pub push_per_token: u32,
    pub push_per_sender: u32,
    /// How often the invitation file is looked at again.
    pub invite_recheck: Duration,
    /// Behind a proxy: the header whose last value is the client address (`CLIENT_IP_HEADER`, e.g. `X-Forwarded-For`).
    /// Set it only when a proxy you trust overwrites it; otherwise a client could pick its own address.
    pub client_ip_header: Option<String>,
    pub gateway_per_token: u32,
    pub gateway_per_caller: u32,
    pub gateway_window: Duration,
}

/// Where a credential comes from: a file, or bytes decoded from an environment variable (for a platform with no shell to
/// write a file with). The bytes are held in memory only and never print, not even in `Debug`.
#[derive(Clone)]
pub enum KeySource {
    File(PathBuf),
    Inline(Vec<u8>),
}

impl std::fmt::Debug for KeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeySource::File(_) => f.write_str("KeySource::File"),
            KeySource::Inline(_) => f.write_str("KeySource::Inline(<redacted>)"),
        }
    }
}

impl KeySource {
    /// The credential's bytes; a read error names `file_var`, never the path or any content.
    pub fn read(&self, file_var: &str) -> Result<Vec<u8>, String> {
        match self {
            KeySource::File(p) => std::fs::read(p).map_err(|e| format!("cannot read {file_var}: {e}")),
            KeySource::Inline(b) => Ok(b.clone()),
        }
    }

    /// The variable that supplied this credential, for error messages.
    pub fn var<'a>(&self, file_var: &'a str, base64_var: &'a str) -> &'a str {
        match self {
            KeySource::File(_) => file_var,
            KeySource::Inline(_) => base64_var,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApnsConfig {
    pub key: KeySource,
    pub key_id: String,
    pub team_id: String,
    /// The app's bundle id.
    pub topic: String,
    pub sandbox_url: String,
    pub production_url: String,
}

#[derive(Clone, Debug)]
pub struct FcmConfig {
    pub credential: KeySource,
    pub base_url: String,
}

impl Config {
    pub fn base(dev: bool) -> Config {
        Config {
            dev,
            addr: "127.0.0.1:8787".into(),
            public_url: "ws://127.0.0.1:8787".into(),
            relay_id: "dev".into(),
            invite_file: None,
            invite_ids: Vec::new(),
            gateway_keys_file: None,
            revoked_dir: None,
            log_dir: None,
            apns: None,
            fcm: None,
            gateway_url: None,
            gateway_key_file: None,
            auth_within: Duration::from_secs(10),
            ping_every: Duration::from_secs(10),
            dead_after: Duration::from_secs(30),
            sweep_every: Duration::from_millis(250),
            max_ttl_secs: 600,
            pair_ttl_secs: 120,
            max_pair_uses: 4,
            max_open_pairs: 4,
            revoked_days: 90,
            log_days: 14,
            mailbox_packets: 64,
            mailbox_bytes: 2 * 1024 * 1024,
            out_queue: 256,
            device_frames_per_sec: 100,
            max_unauth_per_ip: 8,
            conns_per_ip_per_min: 120,
            max_open_per_ip: 32,
            max_unauth_total: 256,
            max_unauth_per_net48: 32,
            max_open_total: 4096,
            guest_within: Duration::from_secs(30),
            state_keep: Duration::from_secs(3600),
            max_mailboxes: 1000,
            max_total_bytes: 256 * 1024 * 1024,
            push_per_token: 60,
            push_per_sender: 600,
            invite_recheck: Duration::from_secs(5),
            client_ip_header: None,
            gateway_per_token: 60,
            gateway_per_caller: 600,
            gateway_window: Duration::from_secs(60),
        }
    }

    /// Reads the environment. `--dev` only changes defaults (localhost, nothing persisted, no credentials needed).
    pub fn from_env(dev: bool) -> Result<Config, String> {
        Config::from_vars(dev, |n| std::env::var(n).ok())
    }

    /// `from_env` over any lookup, so tests need not touch the process environment.
    pub fn from_vars(dev: bool, lookup: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let mut c = Config::base(dev);
        let var = |n: &str| lookup(n).filter(|v| !v.is_empty());
        let need = |n: &str| var(n).ok_or_else(|| format!("{n} is required (set it, or run with --dev to try the relay on this machine)"));
        if let Some(a) = var("RELAY_ADDR") {
            c.addr = a;
        } else if !dev {
            c.addr = "0.0.0.0:8787".into();
        }
        let forced = var("RELAY_DEV_FORCE_BIND").as_deref() == Some("1");
        if dev {
            check_dev_bind(&c.addr, forced)?;
        }
        c.public_url = match var("RELAY_PUBLIC_URL") {
            Some(u) => u,
            None if dev => dev_public_url(&c.addr)?,
            None => return Err("RELAY_PUBLIC_URL is required: the address devices connect to, such as wss://relay.example (a device signs it when it logs in)".into()),
        };
        check_public_url(&c.public_url, dev)?;
        c.client_ip_header = var("CLIENT_IP_HEADER");
        c.relay_id = if dev { var("RELAY_ID").unwrap_or(c.relay_id) } else { need("RELAY_ID")? };
        c.revoked_dir = if dev { var("REVOKED_DIR").map(Into::into) } else { Some(need("REVOKED_DIR")?.into()) };
        c.invite_file = var("INVITE_FILE").map(Into::into);
        if let Some(ids) = var("INVITE_IDS") {
            c.invite_ids = parse_invite_ids(&ids)?;
        }
        c.gateway_keys_file = var("GATEWAY_KEYS_FILE").map(Into::into);
        c.log_dir = var("LOG_DIR").map(Into::into);
        c.gateway_url = var("PUSH_GATEWAY_URL");
        // The bearer key, the push tokens and the grants travel to the gateway: only over TLS (plain http only with --dev).
        if let Some(u) = &c.gateway_url {
            if !dev && !u.to_ascii_lowercase().starts_with("https://") {
                return Err(format!("PUSH_GATEWAY_URL {u} must be an https:// address (plain http only with --dev)"));
            }
        }

        c.gateway_key_file = var("PUSH_GATEWAY_KEY_FILE").map(Into::into);
        if let Some(key) = key_source(&var, "APNS_KEY_FILE", "APNS_KEY_BASE64")? {
            c.apns = Some(ApnsConfig {
                key,
                key_id: need("APNS_KEY_ID")?,
                team_id: need("APNS_TEAM_ID")?,
                topic: need("APNS_TOPIC")?,
                sandbox_url: "https://api.sandbox.push.apple.com".into(),
                production_url: "https://api.push.apple.com".into(),
            });
        }
        if let Some(credential) = key_source(&var, "FCM_SERVICE_ACCOUNT_FILE", "FCM_SERVICE_ACCOUNT_BASE64")? {
            c.fcm = Some(FcmConfig { credential, base_url: "https://fcm.googleapis.com".into() });
        }
        Ok(c)
    }
}

/// The credential named by exactly one of `file_var` / `base64_var`, or `None` when neither is set.
fn key_source(var: &dyn Fn(&str) -> Option<String>, file_var: &str, base64_var: &str) -> Result<Option<KeySource>, String> {
    match (var(file_var), var(base64_var)) {
        (None, None) => Ok(None),
        (Some(f), None) => Ok(Some(KeySource::File(f.into()))),
        (None, Some(b)) => Ok(Some(KeySource::Inline(decode_base64(&b, base64_var)?))),
        (Some(_), Some(_)) => Err(format!("{file_var} and {base64_var} are both set: set only one")),
    }
}

/// Standard-alphabet base64, newlines and other whitespace tolerated. The error names the variable only: what it holds is a secret.
fn decode_base64(text: &str, var: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let compact: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    match base64::engine::general_purpose::STANDARD.decode(compact) {
        Ok(b) if !b.is_empty() => Ok(b),
        _ => Err(format!("{var} could not be decoded: it must be the file's content in base64 (standard alphabet)")),
    }
}

/// `INVITE_IDS`: device ids (32 lowercase hex characters) separated by commas, spaces around each ignored. The error names
/// the variable and the entry's position, never its text.
pub fn parse_invite_ids(list: &str) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    for (i, raw) in list.split(',').enumerate() {
        let id = raw.trim();
        if id.is_empty() {
            continue; // a trailing comma is harmless
        }
        if id.len() != 32 || !id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(format!("INVITE_IDS entry {} is not a device id (32 lowercase hex characters)", i + 1));
        }
        if !ids.iter().any(|x| x == id) {
            ids.push(id.to_owned());
        }
    }
    if ids.is_empty() {
        return Err("INVITE_IDS has no device id in it".into());
    }
    Ok(ids)
}

/// The address `--dev` checks device signatures against when `RELAY_PUBLIC_URL` is not set: its own, on loopback. Listening
/// beyond loopback (`RELAY_DEV_FORCE_BIND=1`) there is no address to derive (`0.0.0.0` is not one a device uses), so the
/// one the phone and the computer connect to must be given, such as `ws://192.168.1.20:8787`.
pub fn dev_public_url(addr: &str) -> Result<String, String> {
    let loopback = addr.parse::<std::net::SocketAddr>().map(|a| a.ip().is_loopback()).unwrap_or_else(|_| addr.starts_with("localhost:"));
    if loopback {
        return Ok(format!("ws://{addr}"));
    }
    Err(format!("--dev listens on {addr}, so RELAY_PUBLIC_URL is required: the address the phone and the computer connect to, such as ws://192.168.1.20:8787 (devices sign it when they log in)"))
}

/// The address devices sign when they log in. Anywhere: `wss://host`, or `ws://` for a relay on localhost. Only with `--dev`
/// also a plain `ws://` on a private network address (a development relay on the home Wi-Fi): a relay that keeps state and
/// sends real push never serves plain text beyond this computer.
pub fn check_public_url(url: &str, dev: bool) -> Result<(), String> {
    let Some(norm) = bobix_link::pairing::normalize_relay_url(url) else {
        return Err(format!("RELAY_PUBLIC_URL {url} is not an acceptable relay address: use wss://host, or ws://localhost for a relay on this computer{}", if dev { ", or ws:// plus a private network address (10.x.x.x, 172.16-31.x.x, 192.168.x.x)" } else { "" }));
    };
    if !dev && norm.starts_with("ws://") && !norm.starts_with("ws://localhost") {
        return Err(format!("RELAY_PUBLIC_URL {url}: a plain ws:// address is for a relay on this computer (ws://localhost); on a private network it is accepted only with --dev. Use wss://host"));
    }
    Ok(())
}

/// `--dev` keeps its revoked list in memory and swallows pushes, so it binds loopback only unless forced.
pub fn check_dev_bind(addr: &str, forced: bool) -> Result<(), String> {
    let loopback = addr.parse::<std::net::SocketAddr>().map(|a| a.ip().is_loopback()).unwrap_or_else(|_| addr.starts_with("localhost:"));
    if loopback || forced {
        return Ok(());
    }
    Err(format!("--dev refuses to listen on {addr}: a dev relay keeps nothing on disk and sends no real push. Use 127.0.0.1, or set RELAY_DEV_FORCE_BIND=1 to serve other machines on purpose"))
}

#[cfg(test)]
mod tests {
    use super::{check_dev_bind, check_public_url, dev_public_url, parse_invite_ids, Config, KeySource};
    use std::collections::HashMap;

    /// `--dev`-free production settings plus `extra`, looked up from a map instead of the process environment.
    fn from(extra: &[(&str, &str)]) -> Result<Config, String> {
        let mut m: HashMap<String, String> = [("RELAY_PUBLIC_URL", "wss://relay.example"), ("RELAY_ID", "r"), ("REVOKED_DIR", "d")].iter().chain(extra).map(|(k, v)| (k.to_string(), v.to_string())).collect();
        m.retain(|_, v| !v.is_empty());
        Config::from_vars(false, |n| m.get(n).cloned())
    }

    const ID_A: &str = "00112233445566778899aabbccddeeff";
    const ID_B: &str = "ffeeddccbbaa99887766554433221100";
    const APNS: [(&str, &str); 3] = [("APNS_KEY_ID", "K"), ("APNS_TEAM_ID", "T"), ("APNS_TOPIC", "app.example")];


    #[test]
    fn a_plain_private_address_is_for_dev_only() {
        for ok in ["wss://relay.example", "wss://relay.example/ws", "ws://localhost:8787", "ws://127.0.0.1:8787"] {
            assert!(check_public_url(ok, false).is_ok(), "{ok}");
            assert!(check_public_url(ok, true).is_ok(), "dev {ok}");
        }
        for lan in ["ws://192.168.1.20:8787", "ws://10.1.2.3:8787", "ws://172.16.0.9:1", "ws://[fd00::1]:8787"] {
            let e = check_public_url(lan, false).unwrap_err();
            assert!(e.contains("only with --dev"), "{lan}: {e}");
            assert!(check_public_url(lan, true).is_ok(), "dev {lan}");
        }
        for bad in ["ws://203.0.113.5:8787", "ws://relay.example", "ws://169.254.169.254", "ws://0.0.0.0:8787", "http://relay.example"] {
            assert!(check_public_url(bad, false).is_err() && check_public_url(bad, true).is_err(), "{bad}");
        }
        // Each message names the rule that applies: the production one does not offer private addresses, the dev one does.
        assert!(!check_public_url("ws://relay.example", false).unwrap_err().contains("private network address"));
        assert!(check_public_url("ws://relay.example", true).unwrap_err().contains("private network address"));
    }

    #[test]
    fn dev_derives_its_address_on_loopback_only() {
        assert_eq!(dev_public_url("127.0.0.1:8787").unwrap(), "ws://127.0.0.1:8787");
        assert_eq!(dev_public_url("[::1]:8787").unwrap(), "ws://[::1]:8787");
        assert!(dev_public_url("0.0.0.0:8787").unwrap_err().contains("RELAY_PUBLIC_URL"));
        assert!(dev_public_url("192.168.1.20:8787").is_err());
    }

    #[test]
    fn a_lan_address_given_for_dev_is_acceptable_and_a_public_plain_one_is_not() {
        for ok in ["ws://192.168.1.20:8787", "ws://10.1.2.3:8787", "ws://[fd00::1]:8787"] {
            assert!(bobix_link::pairing::normalize_relay_url(ok).is_some(), "{ok}");
        }
        for bad in ["ws://203.0.113.5:8787", "ws://relay.example:8787", "ws://0.0.0.0:8787"] {
            assert!(bobix_link::pairing::normalize_relay_url(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn dev_binds_loopback_unless_forced() {
        assert!(check_dev_bind("127.0.0.1:8787", false).is_ok());
        assert!(check_dev_bind("[::1]:8787", false).is_ok());
        assert!(check_dev_bind("localhost:8787", false).is_ok());
        assert!(check_dev_bind("0.0.0.0:8787", false).is_err());
        assert!(check_dev_bind("192.168.1.5:8787", false).is_err());
        assert!(check_dev_bind("0.0.0.0:8787", true).is_ok());
    }

    #[test]
    fn invite_ids_accept_spaced_lowercase_hex_and_refuse_everything_else() {
        let list = format!(" {ID_A} , {ID_B},{ID_A},");
        assert_eq!(parse_invite_ids(&list).unwrap(), vec![ID_A, ID_B]);
        assert_eq!(from(&[("INVITE_IDS", ID_A)]).unwrap().invite_ids, vec![ID_A]);
        assert!(from(&[]).unwrap().invite_ids.is_empty());
        for bad in [&ID_A.to_uppercase(), &ID_A[1..], "not-an-id", &format!("{ID_A}00"), &format!("{ID_A};{ID_B}"), &ID_A.replace('0', "g"), ","] {
            let e = parse_invite_ids(bad).unwrap_err();
            assert!(e.contains("INVITE_IDS"), "{e}");
            assert!(!e.contains(bad.trim_matches(',')) || bad == ",", "the message echoed the value: {e}");
        }
        assert!(parse_invite_ids(&format!("{ID_A},nope")).unwrap_err().contains("entry 2"));
        assert!(from(&[("INVITE_IDS", "nope")]).unwrap_err().contains("INVITE_IDS"));
    }

    #[test]
    fn the_apns_key_comes_from_a_file_or_from_base64_but_not_both() {
        let fake_pem = "-----BEGIN PRIVATE KEY-----\nnot a real key\n-----END PRIVATE KEY-----\n";
        let b64 = base64_of(fake_pem.as_bytes());
        // Wrapped lines and a trailing newline are tolerated.
        let wrapped = format!("{}\n{}\n", &b64[..20], &b64[20..]);
        for text in [b64.as_str(), wrapped.as_str()] {
            let mut v = vec![("APNS_KEY_BASE64", text)];
            v.extend(APNS);
            let cfg = from(&v).unwrap();
            let KeySource::Inline(bytes) = cfg.apns.unwrap().key else { panic!("expected an inline key") };
            assert_eq!(bytes, fake_pem.as_bytes());
        }
        let mut v = vec![("APNS_KEY_FILE", "/some/key.p8")];
        v.extend(APNS);
        assert!(matches!(from(&v).unwrap().apns.unwrap().key, KeySource::File(_)));

        let mut both = vec![("APNS_KEY_FILE", "/some/key.p8"), ("APNS_KEY_BASE64", b64.as_str())];
        both.extend(APNS);
        let e = from(&both).unwrap_err();
        assert!(e.contains("APNS_KEY_FILE") && e.contains("APNS_KEY_BASE64") && !e.contains(&b64), "{e}");
        // The other three stay required.
        assert!(from(&[("APNS_KEY_BASE64", b64.as_str())]).unwrap_err().contains("APNS_KEY_ID"));
    }

    #[test]
    fn a_base64_failure_names_the_variable_and_never_the_content() {
        let secret = "SECRETKEYMATERIAL!!not-base64";
        for var in ["APNS_KEY_BASE64", "FCM_SERVICE_ACCOUNT_BASE64"] {
            let mut v = vec![(var, secret)];
            v.extend(APNS);
            let e = from(&v).unwrap_err();
            assert!(e.contains(var) && e.contains("could not be decoded"), "{e}");
            assert!(!e.contains("SECRETKEY") && !e.contains("not-base64"), "the message echoed the value: {e}");
        }
        // Valid base64 of nothing is not a key either.
        assert!(from(&[("APNS_KEY_BASE64", "===")]).is_err());
    }

    #[test]
    fn the_fcm_service_account_comes_from_a_file_or_from_base64_but_not_both() {
        let b64 = base64_of(br#"{"fake":true}"#);
        assert!(matches!(from(&[("FCM_SERVICE_ACCOUNT_BASE64", b64.as_str())]).unwrap().fcm.unwrap().credential, KeySource::Inline(_)));
        assert!(from(&[("FCM_SERVICE_ACCOUNT_FILE", "/x.json"), ("FCM_SERVICE_ACCOUNT_BASE64", b64.as_str())]).unwrap_err().contains("both set"));
    }

    #[test]
    fn an_inline_key_never_prints() {
        let k = KeySource::Inline(b"SECRETKEYMATERIAL".to_vec());
        assert!(!format!("{k:?}").contains("SECRET"));
    }

    fn base64_of(bytes: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }
}
