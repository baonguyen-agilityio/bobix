//! The real push senders: APNs (HTTP/2, ES256 provider token), FCM (HTTP v1, service-account token) and the client
//! of another relay's push gateway. Each takes a base URL so the tests can point it at a local stand-in.

use crate::config::{ApnsConfig, Config, FcmConfig};
use crate::push::{apns_payload, apns_priority, collapse_id, fcm_message, BoxFuture, Outcome, PushRequest, PushSender};
use bobix_link::relay_frames::{ApnsEnv, Platform, PushKind};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn now_secs() -> u64 {
    crate::log::now_ms() / 1000
}

/// A device token is hex or base64url-ish text; anything else would be a path or header injection.
pub fn token_ok(t: &str) -> bool {
    !t.is_empty() && t.len() <= 512 && t.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_:.".contains(&b))
}

fn client_for(urls: &[&str]) -> reqwest::Client {
    let mut b = reqwest::Client::builder().timeout(Duration::from_secs(10));
    if urls.iter().any(|u| u.starts_with("http://")) {
        b = b.http2_prior_knowledge(); // a plain-HTTP stand-in speaks HTTP/2 without a TLS handshake
    }
    b.build().expect("an HTTP client")
}

type TokenFn = Box<dyn Fn() -> Result<String, String> + Send + Sync>;

pub struct Apns {
    cfg: ApnsConfig,
    client: reqwest::Client,
    jwt: TokenFn,
}

impl Apns {
    pub fn new(cfg: ApnsConfig) -> Result<Apns, String> {
        let pem = cfg.key.read("APNS_KEY_FILE")?;
        let key = EncodingKey::from_ec_pem(&pem).map_err(|e| format!("{} is not a .p8 key: {e}", cfg.key.var("APNS_KEY_FILE", "APNS_KEY_BASE64")))?;
        let (key_id, team) = (cfg.key_id.clone(), cfg.team_id.clone());
        let cache: Mutex<Option<(String, Instant)>> = Mutex::new(None);
        // Apple wants a provider token refreshed between 20 and 60 minutes.
        let jwt: TokenFn = Box::new(move || {
            let mut c = cache.lock().unwrap();
            if let Some((t, at)) = c.as_ref() {
                if at.elapsed() < Duration::from_secs(40 * 60) {
                    return Ok(t.clone());
                }
            }
            let mut h = Header::new(Algorithm::ES256);
            h.kid = Some(key_id.clone());
            let t = jsonwebtoken::encode(&h, &json!({"iss": team, "iat": now_secs()}), &key).map_err(|e| e.to_string())?;
            *c = Some((t.clone(), Instant::now()));
            Ok(t)
        });
        Ok(Apns::with_token(cfg, jwt))
    }

    /// For tests: the provider token comes from `jwt`.
    pub fn with_token(cfg: ApnsConfig, jwt: TokenFn) -> Apns {
        let client = client_for(&[&cfg.sandbox_url, &cfg.production_url]);
        Apns { cfg, client, jwt }
    }

    async fn call(&self, r: &PushRequest) -> Outcome {
        if !token_ok(&r.token) {
            return Outcome::Failed("token has a character a device token never has".into());
        }
        let base = if r.env == ApnsEnv::Sandbox { &self.cfg.sandbox_url } else { &self.cfg.production_url };
        let (push_type, topic) = match r.kind {
            PushKind::Alert => ("alert", self.cfg.topic.clone()),
            PushKind::Background => ("background", self.cfg.topic.clone()),
            PushKind::LiveActivity => ("liveactivity", format!("{}.push-type.liveactivity", self.cfg.topic)),
        };
        let jwt = match (self.jwt)() {
            Ok(j) => j,
            Err(e) => return Outcome::Failed(format!("provider token: {e}")),
        };
        let mut req = self
            .client
            .post(format!("{base}/3/device/{}", r.token))
            .bearer_auth(jwt)
            .header("apns-push-type", push_type)
            .header("apns-topic", topic)
            .header("apns-priority", apns_priority(r.kind, r.priority).to_string())
            .json(&apns_payload(r, now_secs()));
        if let Some(k) = &r.replaces {
            req = req.header("apns-collapse-id", collapse_id(k));
        }
        match req.send().await {
            Err(e) => Outcome::Failed(format!("apns unreachable: {}", e.without_url())),
            Ok(resp) => {
                let status = resp.status().as_u16();
                let reason = resp.json::<Value>().await.ok().and_then(|v| v["reason"].as_str().map(str::to_owned)).unwrap_or_default();
                match (status, reason.as_str()) {
                    (200, _) => Outcome::Sent,
                    (410, _) | (400, "BadDeviceToken") | (400, "DeviceTokenNotForTopic") => Outcome::TokenGone,
                    _ => Outcome::Failed(format!("apns {status} {reason}")),
                }
            }
        }
    }
}

impl PushSender for Apns {
    fn send(&self, req: PushRequest) -> BoxFuture<'_, Outcome> {
        Box::pin(async move { self.call(&req).await })
    }
}

enum FcmAuth {
    Static(String),
    Account { email: String, key: EncodingKey, token_uri: String, cache: tokio::sync::Mutex<Option<(String, Instant)>> },
}

pub struct Fcm {
    project: String,
    base: String,
    client: reqwest::Client,
    auth: FcmAuth,
}

impl Fcm {
    pub fn new(cfg: FcmConfig) -> Result<Fcm, String> {
        let var = cfg.credential.var("FCM_SERVICE_ACCOUNT_FILE", "FCM_SERVICE_ACCOUNT_BASE64");
        let bytes = cfg.credential.read("FCM_SERVICE_ACCOUNT_FILE")?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| format!("{var} is not JSON"))?;
        let field = |n: &str| v[n].as_str().map(str::to_owned).ok_or_else(|| format!("{var} has no {n}"));
        let key = EncodingKey::from_rsa_pem(field("private_key")?.as_bytes()).map_err(|e| format!("service account key: {e}"))?;
        Ok(Fcm {
            project: field("project_id")?,
            client: client_for(&[&cfg.base_url]),
            base: cfg.base_url,
            auth: FcmAuth::Account { email: field("client_email")?, key, token_uri: field("token_uri")?, cache: Default::default() },
        })
    }

    /// For tests: a fixed access token and a stand-in base URL.
    pub fn with_static_token(project: &str, base: &str, token: &str) -> Fcm {
        Fcm { project: project.into(), base: base.into(), client: client_for(&[base]), auth: FcmAuth::Static(token.into()) }
    }

    async fn access_token(&self) -> Result<String, String> {
        let (email, key, token_uri, cache) = match &self.auth {
            FcmAuth::Static(t) => return Ok(t.clone()),
            FcmAuth::Account { email, key, token_uri, cache } => (email, key, token_uri, cache),
        };
        let mut c = cache.lock().await;
        if let Some((t, at)) = c.as_ref() {
            if at.elapsed() < Duration::from_secs(50 * 60) {
                return Ok(t.clone());
            }
        }
        let claims = json!({"iss": email, "scope": "https://www.googleapis.com/auth/firebase.messaging", "aud": token_uri, "iat": now_secs(), "exp": now_secs() + 3600});
        let assertion = jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, key).map_err(|e| e.to_string())?;
        let resp = self
            .client
            .post(token_uri)
            .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", assertion.as_str())])
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        let v: Value = resp.json().await.map_err(|e| e.without_url().to_string())?;
        let t = v["access_token"].as_str().ok_or("no access token in the answer")?.to_owned();
        *c = Some((t.clone(), Instant::now()));
        Ok(t)
    }

    async fn call(&self, r: &PushRequest) -> Outcome {
        if !token_ok(&r.token) {
            return Outcome::Failed("token has a character a device token never has".into());
        }
        let access = match self.access_token().await {
            Ok(t) => t,
            Err(e) => return Outcome::Failed(format!("fcm access token: {e}")),
        };
        let url = format!("{}/v1/projects/{}/messages:send", self.base, self.project);
        match self.client.post(url).bearer_auth(access).json(&fcm_message(r)).send().await {
            Err(e) => Outcome::Failed(format!("fcm unreachable: {}", e.without_url())),
            Ok(resp) => match resp.status().as_u16() {
                200 => Outcome::Sent,
                404 => Outcome::TokenGone,
                s => Outcome::Failed(format!("fcm {s}")),
            },
        }
    }
}

impl PushSender for Fcm {
    fn send(&self, req: PushRequest) -> BoxFuture<'_, Outcome> {
        Box::pin(async move { self.call(&req).await })
    }
}

/// Pushes through another relay's gateway (`POST /v1/push`), for a relay with no APNs or FCM credentials.
pub struct GatewayClient {
    url: String,
    key: Option<String>,
    relay_id: String,
    client: reqwest::Client,
}

impl GatewayClient {
    pub fn new(url: &str, key: Option<String>, relay_id: &str) -> GatewayClient {
        GatewayClient { url: url.trim_end_matches('/').into(), key, relay_id: relay_id.into(), client: client_for(&[url]) }
    }
}

impl PushSender for GatewayClient {
    fn send(&self, r: PushRequest) -> BoxFuture<'_, Outcome> {
        Box::pin(async move {
            let body = json!({
                "platform": r.platform, "kind": r.kind, "event": r.event, "computer": r.computer, "env": r.env, "token": r.token, "grant": r.grant, "body": r.body,
                "relay": self.relay_id, "replaces": r.replaces, "priority": r.priority,
            });
            let mut req = self.client.post(format!("{}/v1/push", self.url)).json(&body);
            if let Some(k) = &self.key {
                req = req.bearer_auth(k);
            }
            match req.send().await {
                Err(e) => Outcome::Failed(format!("gateway unreachable: {}", e.without_url())),
                Ok(resp) => match resp.status().as_u16() {
                    200 => Outcome::Sent,
                    410 => Outcome::TokenGone,
                    s => Outcome::Failed(format!("gateway answered {s}")),
                },
            }
        })
    }
}

/// Sends each push to the service of its platform, or to the gateway when this relay has no credentials of its own.
pub struct Dispatch {
    apns: Option<Apns>,
    fcm: Option<Fcm>,
    gateway: Option<GatewayClient>,
}

impl PushSender for Dispatch {
    fn send(&self, req: PushRequest) -> BoxFuture<'_, Outcome> {
        let own: Option<&dyn PushSender> = match req.platform {
            Platform::Ios => self.apns.as_ref().map(|a| a as &dyn PushSender),
            Platform::Android => self.fcm.as_ref().map(|a| a as &dyn PushSender),
        };
        match own.or(self.gateway.as_ref().map(|g| g as &dyn PushSender)) {
            Some(s) => s.send(req),
            None => Box::pin(async { Outcome::Failed("no push service is configured for this platform".into()) }),
        }
    }
}

pub fn dispatch(cfg: &Config) -> Result<Arc<dyn PushSender>, String> {
    let key = match &cfg.gateway_key_file {
        Some(f) => Some(std::fs::read_to_string(f).map_err(|e| format!("cannot read PUSH_GATEWAY_KEY_FILE: {e}"))?.trim().to_owned()),
        None => None,
    };
    Ok(Arc::new(Dispatch {
        apns: cfg.apns.clone().map(Apns::new).transpose()?,
        fcm: cfg.fcm.clone().map(Fcm::new).transpose()?,
        gateway: cfg.gateway_url.as_deref().map(|u| GatewayClient::new(u, key, &cfg.relay_id)),
    }))
}
