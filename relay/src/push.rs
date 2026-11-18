//! Push: the interface the relay sends through, the grant a phone signs for its token, and the payloads.
//! The body is a sealed packet: the relay sends it without being able to read it.

use bobix_link::keys::{device_id, verify_auth};
use bobix_link::relay_frames::{ActivityEvent, ApnsEnv, Platform, PushKind};
use bobix_link::{b64, b64_decode, Fields};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A grant is good for at most this long.
pub const MAX_GRANT_MS: u64 = 30 * 24 * 3600 * 1000;
/// The keys the phone's app resolves to the text iOS shows when the notification extension cannot open a payload.
pub const TITLE_LOC_KEY: &str = "bobix.push.title";
pub const BODY_LOC_KEY: &str = "bobix.push.body";
/// The Swift type of the Live Activity (`BobixActivityAttributes`): a push that starts an activity names it.
pub const ACTIVITY_ATTRIBUTES_TYPE: &str = "BobixActivityAttributes";
/// How long a pushed Live Activity's content is current: the app's own 15 minutes. After that the system draws it stale.
pub const ACTIVITY_STALE_SECS: u64 = 15 * 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushRequest {
    pub platform: Platform,
    pub env: ApnsEnv,
    pub kind: PushKind,
    /// For `LiveActivity` only: what the push does to the activity, after the relay chose it from the tokens it holds.
    pub event: Option<ActivityEvent>,
    /// The computer that sent the push: a pushed Live Activity names it in its attributes.
    pub computer: String,
    pub token: String,
    pub priority: u8,
    pub replaces: Option<String>,
    /// URL-safe base64 of the sealed packet.
    pub body: String,
    /// Passed to a gateway, which checks it again; a direct sender ignores it.
    pub grant: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Sent,
    /// The push service says the token is no longer valid: forget it.
    TokenGone,
    Failed(String),
}

pub trait PushSender: Send + Sync {
    fn send(&self, req: PushRequest) -> BoxFuture<'_, Outcome>;
}

/// The recording fake `--dev` and the tests use instead of APNs and FCM.
#[derive(Default)]
pub struct FakeSender {
    pub sent: Mutex<Vec<PushRequest>>,
    /// What the fake answers; `Sent` unless a test says otherwise.
    pub answer: Mutex<Option<Outcome>>,
}

impl PushSender for FakeSender {
    fn send(&self, req: PushRequest) -> BoxFuture<'_, Outcome> {
        self.sent.lock().unwrap().push(req);
        let out = self.answer.lock().unwrap().clone().unwrap_or(Outcome::Sent);
        Box::pin(async move { out })
    }
}

/// What a phone signs with its `auth` key for one push token and one relay.
pub fn grant_bytes(token: &str, relay_id: &str, expires_ms: u64) -> Vec<u8> {
    Fields::new("bobix-push-grant-v1").field(token.as_bytes()).field(relay_id.as_bytes()).num(expires_ms).done()
}

#[derive(Debug, PartialEq, Eq)]
pub enum GrantError {
    Malformed,
    Signature,
    Expired,
    TooLong,
}

/// A grant is `auth public key (32) ‖ expiry in unix ms (8, big endian) ‖ Ed25519 signature (64)`, URL-safe base64.
/// The key travels in it so a gateway, which has never seen the phone, can check it; the relay a phone is connected
/// to also requires the key to be the phone's own. Returns the signer's device id and the expiry.
pub fn verify_grant(grant: &str, token: &str, relay_id: &str, now_ms: u64) -> Result<(String, u64), GrantError> {
    let raw = b64_decode(grant).filter(|r| r.len() == 104).ok_or(GrantError::Malformed)?;
    let key: [u8; 32] = raw[..32].try_into().map_err(|_| GrantError::Malformed)?;
    let expires = u64::from_be_bytes(raw[32..40].try_into().map_err(|_| GrantError::Malformed)?);
    if !verify_auth(&key, &grant_bytes(token, relay_id, expires), &raw[40..]) {
        return Err(GrantError::Signature);
    }
    if expires <= now_ms {
        return Err(GrantError::Expired);
    }
    if expires > now_ms + MAX_GRANT_MS + 60_000 {
        return Err(GrantError::TooLong);
    }
    Ok((device_id(&key), expires))
}

pub fn encode_grant(auth_public: &[u8; 32], expires_ms: u64, sig: &[u8]) -> String {
    let mut raw = auth_public.to_vec();
    raw.extend_from_slice(&expires_ms.to_be_bytes());
    raw.extend_from_slice(sig);
    b64(&raw)
}

/// The APNs `apns-priority`: a background push must be 5, anything else is 10 or 5.
pub fn apns_priority(kind: PushKind, given: u8) -> u8 {
    if kind == PushKind::Background || given < 10 { 5 } else { 10 }
}

/// The APNs JSON body. The sealed packet rides in `sealed`; `aps` carries only the localised-text keys.
pub fn apns_payload(r: &PushRequest, now_secs: u64) -> Value {
    match r.kind {
        PushKind::Alert => json!({
            "aps": {"alert": {"title-loc-key": TITLE_LOC_KEY, "loc-key": BODY_LOC_KEY}, "mutable-content": 1, "sound": "default"},
            "sealed": r.body,
        }),
        PushKind::Background => json!({"aps": {"content-available": 1}, "sealed": r.body}),
        PushKind::LiveActivity => live_activity_payload(r, now_secs),
    }
}

/// The ActivityKit payload. The content state is only the sealed `activity` packet: the relay holds no key, so no count and
/// no name is written in the clear (the widget opens the packet itself). `start` also carries the attributes (the
/// computer's id) and an alert, which the system requires to start an activity from a push.
fn live_activity_payload(r: &PushRequest, now_secs: u64) -> Value {
    let event = r.event.unwrap_or(ActivityEvent::Update);
    let mut aps = json!({"timestamp": now_secs, "content-state": {"sealed": r.body}});
    match event {
        ActivityEvent::Start => {
            aps["event"] = json!("start");
            aps["stale-date"] = json!(now_secs + ACTIVITY_STALE_SECS);
            aps["attributes-type"] = json!(ACTIVITY_ATTRIBUTES_TYPE);
            aps["attributes"] = json!({"computerId": r.computer});
            aps["input-push-token"] = json!(1);
            aps["alert"] = json!({"title-loc-key": TITLE_LOC_KEY, "loc-key": BODY_LOC_KEY});
        }
        ActivityEvent::Update => {
            aps["event"] = json!("update");
            aps["stale-date"] = json!(now_secs + ACTIVITY_STALE_SECS);
        }
        ActivityEvent::End => {
            aps["event"] = json!("end");
            aps["dismissal-date"] = json!(now_secs);
        }
    }
    json!({ "aps": aps })
}

/// `apns-collapse-id` allows 64 bytes; a longer key is shortened the same way every time so it still collapses.
pub fn collapse_id(replaces: &str) -> String {
    if replaces.len() <= 64 { replaces.to_owned() } else { bobix_link::hex(&bobix_link::sha256(&[replaces.as_bytes()])[..16]) }
}

/// The FCM v1 message: a data message (the app builds the notification itself), with the given priority.
pub fn fcm_message(r: &PushRequest) -> Value {
    let mut android = json!({"priority": if r.priority >= 10 { "HIGH" } else { "NORMAL" }});
    if let Some(k) = &r.replaces {
        android["collapse_key"] = json!(collapse_id(k));
    }
    json!({"message": {"token": r.token, "data": {"sealed": r.body, "kind": format!("{:?}", r.kind).to_lowercase()}, "android": android}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(kind: PushKind, priority: u8, replaces: Option<&str>) -> PushRequest {
        PushRequest { platform: Platform::Ios, env: ApnsEnv::Sandbox, kind, event: None, computer: "desk".into(), token: "tok".into(), priority, replaces: replaces.map(Into::into), body: "SEALED".into(), grant: String::new() }
    }

    #[test]
    fn alert_carries_loc_keys_not_fixed_text() {
        let p = apns_payload(&req(PushKind::Alert, 10, None), 1).to_string();
        assert!(p.contains("title-loc-key") && p.contains("\"loc-key\"") && p.contains("SEALED"));
        assert!(!p.contains("\"title\"") && !p.contains("\"body\""));
    }

    #[test]
    fn priority_rules() {
        assert_eq!(apns_priority(PushKind::Background, 10), 5);
        assert_eq!(apns_priority(PushKind::Alert, 10), 10);
        assert_eq!(apns_priority(PushKind::Alert, 5), 5);
        assert_eq!(fcm_message(&req(PushKind::Alert, 10, None))["message"]["android"]["priority"], "HIGH");
        assert_eq!(fcm_message(&req(PushKind::Alert, 5, Some("k")))["message"]["android"]["collapse_key"], "k");
    }

    #[test]
    fn long_collapse_keys_map_to_the_same_short_id() {
        let long = "x".repeat(200);
        assert_eq!(collapse_id(&long), collapse_id(&long));
        assert!(collapse_id(&long).len() <= 64);
    }
}
