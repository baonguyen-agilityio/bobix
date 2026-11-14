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
    pub gateway_per_token: u32,
    pub gateway_per_caller: u32,
    pub gateway_window: Duration,
}

#[derive(Clone, Debug)]
pub struct ApnsConfig {
    pub key_file: PathBuf,
    pub key_id: String,
    pub team_id: String,
    /// The app's bundle id.
    pub topic: String,
    pub sandbox_url: String,
    pub production_url: String,
}

#[derive(Clone, Debug)]
pub struct FcmConfig {
    pub service_account_file: PathBuf,
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
            gateway_per_token: 60,
            gateway_per_caller: 600,
            gateway_window: Duration::from_secs(60),
        }
    }

    /// Reads the environment. `--dev` only changes defaults (localhost, nothing persisted, no credentials needed).
    pub fn from_env(dev: bool) -> Result<Config, String> {
        let mut c = Config::base(dev);
        let var = |n: &str| std::env::var(n).ok().filter(|v| !v.is_empty());
        let need = |n: &str| var(n).ok_or_else(|| format!("{n} is required (set it, or run with --dev to try the relay on this machine)"));
        if let Some(a) = var("RELAY_ADDR") {
            c.addr = a;
        } else if !dev {
            c.addr = "0.0.0.0:8787".into();
        }
        c.public_url = match var("RELAY_PUBLIC_URL") {
            Some(u) => u,
            None if dev => format!("ws://{}", c.addr),
            None => return Err("RELAY_PUBLIC_URL is required: the address devices connect to, such as wss://relay.example (a device signs it when it logs in)".into()),
        };
        if bobix_link::pairing::normalize_relay_url(&c.public_url).is_none() {
            return Err(format!("RELAY_PUBLIC_URL {} is not an acceptable relay address (wss://host, or ws:// on localhost)", c.public_url));
        }
        c.relay_id = if dev { var("RELAY_ID").unwrap_or(c.relay_id) } else { need("RELAY_ID")? };
        c.revoked_dir = if dev { var("REVOKED_DIR").map(Into::into) } else { Some(need("REVOKED_DIR")?.into()) };
        c.invite_file = var("INVITE_FILE").map(Into::into);
        c.gateway_keys_file = var("GATEWAY_KEYS_FILE").map(Into::into);
        c.log_dir = var("LOG_DIR").map(Into::into);
        c.gateway_url = var("PUSH_GATEWAY_URL");
        c.gateway_key_file = var("PUSH_GATEWAY_KEY_FILE").map(Into::into);
        if let Some(key_file) = var("APNS_KEY_FILE") {
            c.apns = Some(ApnsConfig {
                key_file: key_file.into(),
                key_id: need("APNS_KEY_ID")?,
                team_id: need("APNS_TEAM_ID")?,
                topic: need("APNS_TOPIC")?,
                sandbox_url: "https://api.sandbox.push.apple.com".into(),
                production_url: "https://api.push.apple.com".into(),
            });
        }
        if let Some(f) = var("FCM_SERVICE_ACCOUNT_FILE") {
            c.fcm = Some(FcmConfig { service_account_file: f.into(), base_url: "https://fcm.googleapis.com".into() });
        }
        Ok(c)
    }
}
