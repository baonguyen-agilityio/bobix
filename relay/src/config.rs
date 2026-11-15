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
    /// Outbound frames queued for one socket; a socket whose queue is full is closed.
    pub out_queue: usize,
    /// Frames one device may send per second; more closes the socket.
    pub device_frames_per_sec: u32,
    /// Unauthenticated sockets one client address may hold, and new connections it may open per minute.
    pub max_unauth_per_ip: usize,
    pub conns_per_ip_per_min: u32,
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
            out_queue: 256,
            device_frames_per_sec: 100,
            max_unauth_per_ip: 8,
            conns_per_ip_per_min: 120,
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
        let mut c = Config::base(dev);
        let var = |n: &str| std::env::var(n).ok().filter(|v| !v.is_empty());
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
            None if dev => format!("ws://{}", c.addr),
            None => return Err("RELAY_PUBLIC_URL is required: the address devices connect to, such as wss://relay.example (a device signs it when it logs in)".into()),
        };
        if bobix_link::pairing::normalize_relay_url(&c.public_url).is_none() {
            return Err(format!("RELAY_PUBLIC_URL {} is not an acceptable relay address (wss://host, or ws:// on localhost)", c.public_url));
        }
        c.client_ip_header = var("CLIENT_IP_HEADER");
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
    use super::check_dev_bind;

    #[test]
    fn dev_binds_loopback_unless_forced() {
        assert!(check_dev_bind("127.0.0.1:8787", false).is_ok());
        assert!(check_dev_bind("[::1]:8787", false).is_ok());
        assert!(check_dev_bind("localhost:8787", false).is_ok());
        assert!(check_dev_bind("0.0.0.0:8787", false).is_err());
        assert!(check_dev_bind("192.168.1.5:8787", false).is_err());
        assert!(check_dev_bind("0.0.0.0:8787", true).is_ok());
    }
}
