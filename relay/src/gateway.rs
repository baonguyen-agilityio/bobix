//! `POST /v1/push { platform, kind, token, grant, body }` for relays that have no Apple or Google credentials.
//! A grant only proves that the caller knows a push token, so with `GATEWAY_KEYS_FILE` set only relays that present
//! a key from that file are served; unset means open.

use crate::config::Config;
use crate::limits::RateLimit;
use crate::push::{verify_grant, Outcome, PushRequest};
use crate::senders::token_ok;
use crate::App;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::Json;
use bobix_link::messages::MAX_PUSH_BODY;
use bobix_link::relay_frames::{ActivityEvent, ApnsEnv, Platform, PushKind};
use bobix_link::{hex, sha256};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;

pub struct Limits {
    per_token: RateLimit,
    per_caller: RateLimit,
}

impl Limits {
    pub fn new(cfg: &Config) -> Limits {
        Limits { per_token: RateLimit::new(cfg.gateway_per_token, cfg.gateway_window), per_caller: RateLimit::new(cfg.gateway_per_caller, cfg.gateway_window) }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Req {
    platform: Platform,
    kind: PushKind,
    /// A Live Activity push: what it does (start, update, end). Absent reads as update.
    #[serde(default)]
    event: Option<ActivityEvent>,
    /// The computer that sent the push (a started activity names it).
    #[serde(default)]
    computer: String,
    #[serde(default = "production")]
    env: ApnsEnv,
    token: String,
    grant: String,
    body: String,
    /// The relay id the grant names. Needed to check a grant when no gateway key says which relay is calling.
    #[serde(default)]
    relay: Option<String>,
    #[serde(default)]
    replaces: Option<String>,
    #[serde(default = "high")]
    priority: u8,
}

fn production() -> ApnsEnv {
    ApnsEnv::Production
}

fn high() -> u8 {
    10
}

fn answer(status: StatusCode, why: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "ok": status == StatusCode::OK, "why": why })))
}

/// The caller a presented key belongs to: a short id for the rate limit, and the relay id the line names, if any.
fn key_entry(cfg: &Config, headers: &HeaderMap) -> Option<(String, Option<String>)> {
    let file = cfg.gateway_keys_file.as_ref()?;
    let presented = headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")?;
    let wanted = sha256(&[presented.as_bytes()]);
    for line in std::fs::read_to_string(file).unwrap_or_default().lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        if let Some(k) = parts.next() {
            if sha256(&[k.as_bytes()]) == wanted {
                return Some((hex(&wanted[..8]), parts.next().map(str::to_owned)));
            }
        }
    }
    None
}

pub async fn push(
    State(app): State<Arc<App>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<Req>,
) -> (StatusCode, Json<Value>) {
    // A gateway never forwards to another gateway: the platform needs this relay's own credentials (or --dev's fake).
    let own = app.cfg.dev || if req.platform == Platform::Ios { app.cfg.apns.is_some() } else { app.cfg.fcm.is_some() };
    if !own {
        return answer(StatusCode::NOT_IMPLEMENTED, "platform");
    }
    // Who is calling: a relay with a key from the file, or (open gateway) just an address.
    let (caller, caller_relay) = if app.cfg.gateway_keys_file.is_some() {
        match key_entry(&app.cfg, &headers) {
            Some(c) => c,
            None => return answer(StatusCode::UNAUTHORIZED, "key"),
        }
    } else {
        (crate::limits::client_ip(&headers, addr, &app.cfg.client_ip_header), None)
    };
    if !app.gateway_limits.lock().unwrap().per_caller.allow(&caller) {
        return answer(StatusCode::TOO_MANY_REQUESTS, "caller");
    }
    if req.body.len() > MAX_PUSH_BODY || !token_ok(&req.token) {
        return answer(StatusCode::PAYLOAD_TOO_LARGE, "body");
    }
    let Some(relay) = caller_relay.or(req.relay.clone()) else { return answer(StatusCode::BAD_REQUEST, "relay") };
    if verify_grant(&req.grant, &req.token, &relay, crate::hub::now_ms()).is_err() {
        return answer(StatusCode::FORBIDDEN, "grant");
    }
    // Only a push with a good grant counts against its token, so a stranger cannot use up someone else's budget.
    if !app.gateway_limits.lock().unwrap().per_token.allow(&hex(&sha256(&[req.token.as_bytes()])[..8])) {
        return answer(StatusCode::TOO_MANY_REQUESTS, "token");
    }
    let r = PushRequest { platform: req.platform, env: req.env, kind: req.kind, event: req.event, computer: req.computer, token: req.token, priority: req.priority, replaces: req.replaces, body: req.body, grant: req.grant };
    match app.sender.send(r).await {
        Outcome::Sent => {
            app.log.line("gateway push sent");
            answer(StatusCode::OK, "sent")
        }
        Outcome::TokenGone => answer(StatusCode::GONE, "token"),
        Outcome::Failed(why) => {
            app.log.line(&format!("gateway push failed: {why}"));
            answer(StatusCode::BAD_GATEWAY, "push service")
        }
    }
}
