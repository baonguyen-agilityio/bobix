//! The Bobix relay: forwards sealed packets between a desktop and its paired phones, holds a packet for an
//! offline device for at most ten minutes, tells each side whether the other is online, and sends pushes.
//! It never holds a key that opens a packet: it only moves the `body` strings of `bobix-link` frames.

pub mod config;
pub mod gateway;
pub mod hub;
pub mod limits;
pub mod log;
pub mod mailbox;
pub mod push;
pub mod revoked;
pub mod senders;
pub mod ws;

use axum::routing::{get, post};
use axum::Router;
use config::Config;
use hub::Hub;
use push::{FakeSender, PushSender};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

pub struct App {
    pub cfg: Config,
    pub hub: Mutex<Hub>,
    pub sender: Arc<dyn PushSender>,
    /// Set when the recording fake stands in for APNs and FCM (`--dev`, tests).
    pub fake: Option<Arc<FakeSender>>,
    pub log: log::Logger,
    pub gateway_limits: Mutex<gateway::Limits>,
    pub ips: Mutex<limits::Ips>,
}

impl App {
    pub fn new(cfg: Config, sender: Arc<dyn PushSender>, fake: Option<Arc<FakeSender>>) -> Arc<App> {
        Arc::new(App {
            log: log::Logger::new(cfg.log_dir.clone(), cfg.log_days),
            gateway_limits: Mutex::new(gateway::Limits::new(&cfg)),
            ips: Mutex::new(limits::Ips::new(cfg.max_unauth_per_ip, cfg.conns_per_ip_per_min)),
            hub: Mutex::new(Hub::new(cfg.clone())),
            sender,
            fake,
            cfg,
        })
    }

    pub fn with_fake(cfg: Config) -> Arc<App> {
        let fake = Arc::new(FakeSender::default());
        App::new(cfg, fake.clone(), Some(fake))
    }

    pub fn from_config(cfg: Config) -> Result<Arc<App>, String> {
        if cfg.dev {
            return Ok(App::with_fake(cfg));
        }
        let sender = senders::dispatch(&cfg)?;
        Ok(App::new(cfg, sender, None))
    }
}

pub fn router(app: Arc<App>) -> Router {
    let mut r = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/ws", get(ws::upgrade));
    // Only a relay with its own APNs or FCM credentials (or --dev) is a gateway; any other would be an open proxy.
    if app.cfg.dev || app.cfg.apns.is_some() || app.cfg.fcm.is_some() {
        r = r.route("/v1/push", post(gateway::push));
    }
    if app.cfg.dev {
        // The fake tools of bobix-link connect to the bare address; the pushes the recording fake saw are listed here.
        r = r.route("/", get(ws::upgrade)).route("/dev/pushes", get(dev_pushes));
    }
    r.with_state(app)
}

/// "ok", or 503 naming what a monitor must fix: the invitation file or the gateway keys file, when one is set and cannot be read
/// (a relay that lost its invitation file would otherwise look healthy). Names the file's role, never its path.
async fn healthz(axum::extract::State(app): axum::extract::State<Arc<App>>) -> (axum::http::StatusCode, &'static str) {
    let readable = |p: &Option<std::path::PathBuf>| p.as_ref().is_none_or(|p| std::fs::File::open(p).is_ok_and(|f| f.metadata().is_ok_and(|m| m.is_file())));
    match (readable(&app.cfg.invite_file), readable(&app.cfg.gateway_keys_file)) {
        (true, true) => (axum::http::StatusCode::OK, "ok"),
        (false, _) => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "invitation file missing or unreadable"),
        (true, false) => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "gateway keys file missing or unreadable"),
    }
}

async fn dev_pushes(axum::extract::State(app): axum::extract::State<Arc<App>>) -> axum::Json<serde_json::Value> {
    let sent = app.fake.as_ref().map(|f| f.sent.lock().unwrap().clone()).unwrap_or_default();
    // Kinds and sizes only: not the token, not the body.
    axum::Json(serde_json::json!(sent.iter().map(|r| serde_json::json!({
        "platform": format!("{:?}", r.platform), "env": format!("{:?}", r.env), "kind": format!("{:?}", r.kind),
        "priority": r.priority, "replaces": r.replaces, "bodyBytes": r.body.len(),
    })).collect::<Vec<_>>()))
}

/// Serves until the process ends. Sweeps expired packets in the background.
pub async fn run(app: Arc<App>, listener: TcpListener) -> std::io::Result<()> {
    let sweeper = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(sweeper.cfg.sweep_every);
        let mut last_prune = std::time::Instant::now();
        loop {
            tick.tick().await;
            sweeper.hub.lock().unwrap().sweep();
            if last_prune.elapsed().as_secs() >= 3600 {
                last_prune = std::time::Instant::now();
                sweeper.log.prune(log::now_ms());
            }
        }
    });
    axum::serve(listener, router(app).into_make_service_with_connect_info::<SocketAddr>()).await
}
