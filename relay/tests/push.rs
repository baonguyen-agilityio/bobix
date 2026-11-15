//! Push: registration with a grant, sending through the sender interface (the recording fake), the gateway, and the
//! real APNs / FCM senders against a local stand-in. No real credential exists here and none is used.

mod common;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post as post_route;
use axum::{Json, Router};
use bobix_link::messages::Who;
use bobix_link::relay_frames::*;
use bobix_relay::config::{ApnsConfig, Config};
use bobix_relay::push::{Outcome, PushRequest, PushSender};
use bobix_relay::senders::{Apns, Fcm, GatewayClient};
use bobix_relay::App;
use common::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RELAY: &str = "test-relay";
const DAY: u64 = 86_400_000;

fn is_err(code: ErrorCode) -> impl Fn(&Frame) -> bool {
    move |f| matches!(f, Frame::Error { code: c, .. } if *c == code)
}

fn push_token(kind: TokenKind, token: &str, grant: String) -> Frame {
    Frame::PushToken { platform: Platform::Ios, env: ApnsEnv::Sandbox, kind, r#ref: None, token: token.into(), grant }
}

fn with_push(to: &str, pid: &str, kind: PushKind, replaces: Option<&str>, body: &str) -> Frame {
    Frame::Send {
        pid: pid.into(),
        to: to.into(),
        ttl: 600,
        collapse: None,
        push: Some(Push { kind, priority: 10, replaces: replaces.map(Into::into) }),
        body: body.into(),
    }
}

fn sent(s: &Server) -> Vec<PushRequest> {
    s.app.fake.as_ref().unwrap().sent.lock().unwrap().clone()
}

async fn wait_sent(s: &Server, n: usize) {
    for _ in 0..40 {
        if sent(s).len() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("expected {n} pushes, saw {}", sent(s).len());
}

/// A desktop and a phone that registered `token` with a good grant.
async fn with_registered_phone(s: &Server, token: &str) -> (Dev, Dev) {
    let (d, mut p) = pair_up(s).await;
    let g = grant(&p.ident, token, RELAY, now_ms() + DAY);
    p.send(&push_token(TokenKind::Device, token, g)).await;
    p.sync().await;
    (d, p)
}

#[tokio::test]
async fn a_push_with_a_valid_grant_is_handed_to_the_sender_with_its_kind_priority_and_collapse_key() {
    let s = start(cfg()).await;
    let (mut d, mut p) = with_registered_phone(&s, "tok-ok").await;
    d.send(&with_push(&p.id(), "n1", PushKind::Alert, Some("home"), "SEALEDBODY")).await;
    p.expect(|f| matches!(f, Frame::Msg { .. }), 1000).await;
    wait_sent(&s, 1).await;
    let r = sent(&s).remove(0);
    assert_eq!((r.kind, r.token.as_str(), r.priority, r.replaces.as_deref(), r.env), (PushKind::Alert, "tok-ok", 10, Some("home"), ApnsEnv::Sandbox));
    assert_eq!(r.body, "SEALEDBODY");
    d.none(is_err(ErrorCode::Grant), 200).await;

    // A later notice with the same `replaces` carries the same key; a send without `push` pushes nothing.
    d.send(&with_push(&p.id(), "n2", PushKind::Background, Some("home"), "B2")).await;
    wait_sent(&s, 2).await;
    assert_eq!(sent(&s)[1].replaces.as_deref(), Some("home"));
    d.send(&send(&p.id(), "n3", 600, None, "B3")).await;
    p.expect(|f| matches!(f, Frame::Msg { pid, .. } if pid == "n3"), 1000).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(sent(&s).len(), 2);
}

#[tokio::test]
async fn a_token_without_a_valid_grant_is_not_stored() {
    let s = start(cfg()).await;
    let (mut d, mut p) = pair_up(&s).await;
    let now = now_ms();
    let other = ident();
    let bad = [
        ("garbage", "not-a-grant".to_owned()),
        ("expired", grant(&p.ident, "t1", RELAY, now - 1000)),
        ("over 30 days", grant(&p.ident, "t1", RELAY, now + 31 * DAY)),
        ("another token", grant(&p.ident, "t-other", RELAY, now + DAY)),
        ("another relay", grant(&p.ident, "t1", "someone-elses-relay", now + DAY)),
        ("another phone's key", grant(&other, "t1", RELAY, now + DAY)),
    ];
    for (why, g) in bad {
        p.send(&push_token(TokenKind::Device, "t1", g)).await;
        p.expect(is_err(ErrorCode::Grant), 1000).await;
        d.send(&with_push(&p.id(), &format!("n-{why}"), PushKind::Alert, None, "X")).await;
        p.expect(|f| matches!(f, Frame::Msg { .. }), 1000).await; // the packet itself still arrives
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(sent(&s).is_empty());
}

#[tokio::test]
async fn a_push_for_a_phone_whose_grant_has_ended_is_answered_with_a_grant_error() {
    let s = start(cfg()).await;
    let (mut d, mut p) = pair_up(&s).await;
    p.send(&push_token(TokenKind::Device, "short", grant(&p.ident, "short", RELAY, now_ms() + 2000))).await;
    p.sync().await;
    // The margin is wide so a slow runner still registers the token before the grant ends.
    tokio::time::sleep(Duration::from_millis(2200)).await;
    d.send(&with_push(&p.id(), "late", PushKind::Alert, None, "X")).await;
    d.expect(|f| matches!(f, Frame::Queued { .. }), 1000).await;
    let phone = p.id();
    d.expect(|f| matches!(f, Frame::Error { code: ErrorCode::Grant, who: Some(Who::Phone), peer: Some(x) } if *x == phone), 1000).await;
    p.expect(|f| matches!(f, Frame::Msg { .. }), 1000).await;
    assert!(sent(&s).is_empty());
}

#[tokio::test]
async fn a_live_activity_push_needs_an_activity_token_and_a_big_body_is_not_pushed() {
    let s = start(cfg()).await;
    let (mut d, mut p) = with_registered_phone(&s, "dev-tok").await;
    d.send(&with_push(&p.id(), "la", PushKind::LiveActivity, None, "X")).await;
    p.expect(|f| matches!(f, Frame::Msg { .. }), 1000).await;
    p.send(&push_token(TokenKind::Activity, "act-tok", grant(&p.ident, "act-tok", RELAY, now_ms() + DAY))).await;
    p.sync().await;
    d.send(&with_push(&p.id(), "la2", PushKind::LiveActivity, None, "X")).await;
    wait_sent(&s, 1).await;
    assert_eq!(sent(&s)[0].token, "act-tok");

    let big = "A".repeat(3_501);
    d.send(&with_push(&p.id(), "big", PushKind::Alert, None, &big)).await;
    p.expect(|f| matches!(f, Frame::Msg { pid, .. } if pid == "big"), 1000).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(sent(&s).len(), 1, "a body over the push limit is delivered over the socket but not pushed");
}

#[tokio::test]
async fn a_token_the_push_service_rejects_is_forgotten() {
    let s = start(cfg()).await;
    let (mut d, mut p) = with_registered_phone(&s, "dead-tok").await;
    *s.app.fake.as_ref().unwrap().answer.lock().unwrap() = Some(Outcome::TokenGone);
    d.send(&with_push(&p.id(), "a", PushKind::Alert, None, "X")).await;
    wait_sent(&s, 1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    d.send(&with_push(&p.id(), "b", PushKind::Alert, None, "X")).await;
    p.expect(|f| matches!(f, Frame::Msg { pid, .. } if pid == "b"), 1000).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(sent(&s).len(), 1);
}

#[tokio::test]
async fn logs_hold_ids_and_counts_never_a_body_or_a_token() {
    let dir = scratch("logs");
    let mut c = cfg();
    c.log_dir = Some(dir.clone());
    let s = start(c).await;
    let (mut d, mut p) = with_registered_phone(&s, "TOKEN-SECRET-12345").await;
    d.send(&with_push(&p.id(), "n1", PushKind::Alert, None, "BODY-SECRET-67890")).await;
    p.expect(|f| matches!(f, Frame::Msg { .. }), 1000).await;
    wait_sent(&s, 1).await;
    p.send(&push_token(TokenKind::Device, "x", "BAD-GRANT-SECRET".into())).await; // a refused registration is logged too
    p.sync().await;
    let all: String = std::fs::read_dir(&dir).unwrap().flatten().map(|e| std::fs::read_to_string(e.path()).unwrap()).collect();
    assert!(all.contains(&d.id()) && all.contains("push to"));
    for secret in ["TOKEN-SECRET-12345", "BODY-SECRET-67890", "BAD-GRANT-SECRET"] {
        assert!(!all.contains(secret), "{secret} found in the log");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

// ---- the gateway --------------------------------------------------------------------------------------------------

async fn gateway(keys: Option<&str>, tweak: impl FnOnce(&mut Config)) -> (Server, Option<std::path::PathBuf>) {
    let mut c = cfg();
    let file = keys.map(|k| {
        let f = scratch("keys");
        std::fs::write(&f, k).unwrap();
        f
    });
    c.gateway_keys_file = file.clone();
    tweak(&mut c);
    (start(c).await, file)
}

async fn post(s: &Server, key: Option<&str>, body: Value) -> (u16, Value) {
    let mut r = reqwest::Client::new().post(format!("http://{}/v1/push", s.addr)).json(&body);
    if let Some(k) = key {
        r = r.bearer_auth(k);
    }
    let resp = r.send().await.unwrap();
    (resp.status().as_u16(), resp.json().await.unwrap_or(Value::Null))
}

fn req(id: &bobix_link::keys::Identity, token: &str, relay: &str, exp: u64) -> Value {
    json!({"platform": "ios", "kind": "alert", "env": "sandbox", "token": token, "grant": grant(id, token, relay, exp), "body": "SEALED", "relay": relay})
}

#[tokio::test]
async fn the_gateway_serves_only_relays_with_a_key_from_the_file_when_one_is_set() {
    let (s, file) = gateway(Some("good-key relay-a\nother-key relay-b # a second self-hoster\n"), |_| {}).await;
    let phone = ident();
    let ok = req(&phone, "gw-tok", "relay-a", now_ms() + DAY);

    assert_eq!(post(&s, None, ok.clone()).await.0, 401, "no key");
    assert_eq!(post(&s, Some("guess"), ok.clone()).await.0, 401, "a key that is not in the file");
    assert_eq!(post(&s, Some("good-key"), ok.clone()).await.0, 200);
    assert_eq!(sent(&s).len(), 1);
    // The relay the key names is the relay the grant must name.
    assert_eq!(post(&s, Some("other-key"), ok).await.0, 403, "a grant for another relay");
    let now = now_ms();
    assert_eq!(post(&s, Some("good-key"), req(&phone, "gw-tok", "relay-a", now - 1)).await.0, 403, "expired");
    assert_eq!(post(&s, Some("good-key"), req(&phone, "gw-tok", "relay-a", now + 31 * DAY)).await.0, 403, "over 30 days");
    let mut wrong_token = req(&phone, "gw-tok", "relay-a", now + DAY);
    wrong_token["token"] = json!("gw-tok-2");
    assert_eq!(post(&s, Some("good-key"), wrong_token).await.0, 403, "a grant for another token");
    let mut big = req(&phone, "gw-tok", "relay-a", now + DAY);
    big["body"] = json!("A".repeat(3_501));
    assert_eq!(post(&s, Some("good-key"), big).await.0, 413);
    assert_eq!(sent(&s).len(), 1);
    std::fs::remove_file(file.unwrap()).unwrap();
}

#[tokio::test]
async fn an_open_gateway_needs_no_key_but_still_a_grant_and_the_relay_it_names() {
    let (s, _) = gateway(None, |_| {}).await;
    let phone = ident();
    assert_eq!(post(&s, None, req(&phone, "t", "relay-a", now_ms() + DAY)).await.0, 200);
    let mut no_relay = req(&phone, "t", "relay-a", now_ms() + DAY);
    no_relay.as_object_mut().unwrap().remove("relay");
    assert_eq!(post(&s, None, no_relay).await.0, 400);
    let mut wrong_relay = req(&phone, "t", "relay-a", now_ms() + DAY);
    wrong_relay["relay"] = json!("relay-b");
    assert_eq!(post(&s, None, wrong_relay).await.0, 403);
}

#[tokio::test]
async fn a_flood_from_one_token_or_one_calling_relay_is_limited_and_others_are_not() {
    let (s, file) = gateway(Some("k1 relay-a\nk2 relay-a\n"), |c| {
        c.gateway_per_token = 3;
        c.gateway_per_caller = 5;
    })
    .await;
    let phone = ident();
    let exp = now_ms() + DAY;
    for _ in 0..3 {
        assert_eq!(post(&s, Some("k1"), req(&phone, "flood", "relay-a", exp)).await.0, 200);
    }
    assert_eq!(post(&s, Some("k1"), req(&phone, "flood", "relay-a", exp)).await.0, 429, "the 4th push to one token");
    assert_eq!(post(&s, Some("k1"), req(&phone, "calm", "relay-a", exp)).await.0, 200, "another token is unaffected");
    // k1 has now made 5 calls; the 6th is limited whatever the token.
    assert_eq!(post(&s, Some("k1"), req(&phone, "calm2", "relay-a", exp)).await.0, 429, "the calling relay is limited");
    assert_eq!(post(&s, Some("k2"), req(&phone, "calm2", "relay-a", exp)).await.0, 200, "another calling relay is unaffected");
    std::fs::remove_file(file.unwrap()).unwrap();
}

#[tokio::test]
async fn pushes_with_bad_grants_do_not_use_up_a_real_tokens_budget() {
    let (s, _) = gateway(None, |c| c.gateway_per_token = 3).await;
    let phone = ident();
    for _ in 0..5 {
        post(&s, None, json!({"platform":"ios","kind":"alert","token":"victim","grant":"bad","body":"X","relay":"relay-a"})).await;
    }
    for _ in 0..3 {
        assert_eq!(post(&s, None, req(&phone, "victim", "relay-a", now_ms() + DAY)).await.0, 200);
    }
    assert_eq!(post(&s, None, req(&phone, "victim", "relay-a", now_ms() + DAY)).await.0, 429);
}

#[tokio::test]
async fn a_relay_without_credentials_pushes_through_a_gateway() {
    let (gw, file) = gateway(Some("shared-key\n"), |_| {}).await;
    let mut c = cfg();
    c.relay_id = "self-hosted".into();
    let client = GatewayClient::new(&format!("http://{}", gw.addr), Some("shared-key".into()), "self-hosted");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    c.public_url = format!("ws://{addr}");
    let app = App::new(c, Arc::new(client), None);
    tokio::spawn(bobix_relay::run(app.clone(), listener));
    let own = Server { addr, app };

    let (mut d, mut p) = pair_up(&own).await;
    p.send(&push_token(TokenKind::Device, "via-gw", grant(&p.ident, "via-gw", "self-hosted", now_ms() + DAY))).await;
    p.sync().await;
    d.send(&with_push(&p.id(), "n", PushKind::Alert, Some("r"), "SEALED")).await;
    wait_sent(&gw, 1).await;
    assert_eq!((sent(&gw)[0].token.as_str(), sent(&gw)[0].replaces.as_deref()), ("via-gw", Some("r")));
    std::fs::remove_file(file.unwrap()).unwrap();
}

// ---- the real senders, against a local stand-in --------------------------------------------------------------------

type Seen = Arc<Mutex<Vec<(String, HeaderMap, Value)>>>;

/// Answers every push with `status` (and APNs' `reason` body) and records what it was sent.
async fn stand_in(path: &'static str, status: u16) -> (String, Seen) {
    let seen: Seen = Arc::default();
    async fn handle(State((seen, status)): State<(Seen, u16)>, Path(rest): Path<String>, h: HeaderMap, Json(b): Json<Value>) -> (StatusCode, Json<Value>) {
        seen.lock().unwrap().push((rest, h, b));
        (StatusCode::from_u16(status).unwrap(), Json(json!({"reason": "Unregistered"})))
    }
    let app = Router::new().route(path, post_route(handle)).with_state((seen.clone(), status));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await });
    (url, seen)
}

fn pr(kind: PushKind, env: ApnsEnv, token: &str, replaces: Option<&str>, priority: u8) -> PushRequest {
    PushRequest { platform: Platform::Ios, env, kind, token: token.into(), priority, replaces: replaces.map(Into::into), body: "SEALED".into(), grant: String::new() }
}

fn header(h: &HeaderMap, n: &str) -> String {
    h.get(n).map(|v| v.to_str().unwrap().to_owned()).unwrap_or_default()
}

#[tokio::test]
async fn apns_gets_the_right_host_push_type_topic_priority_collapse_id_and_loc_keys() {
    let (sandbox, seen_sb) = stand_in("/3/device/{token}", 200).await;
    let (prod, seen_pr) = stand_in("/3/device/{token}", 200).await;
    let cfg = ApnsConfig {
        key_file: "unused".into(),
        key_id: "KEYID".into(),
        team_id: "TEAM".into(),
        topic: "app.example.bobix".into(),
        sandbox_url: sandbox,
        production_url: prod,
    };
    let apns = Apns::with_token(cfg, Box::new(|| Ok("PROVIDER-JWT".into())));

    assert_eq!(apns.send(pr(PushKind::Alert, ApnsEnv::Sandbox, "aaa", Some("home"), 10)).await, Outcome::Sent);
    assert_eq!((seen_sb.lock().unwrap().len(), seen_pr.lock().unwrap().len()), (1, 0), "a sandbox token goes to the sandbox host");
    apns.send(pr(PushKind::Alert, ApnsEnv::Production, "bbb", Some("home"), 10)).await;
    apns.send(pr(PushKind::Background, ApnsEnv::Production, "bbb", None, 10)).await;
    apns.send(pr(PushKind::LiveActivity, ApnsEnv::Production, "ccc", None, 5)).await;
    assert_eq!((seen_sb.lock().unwrap().len(), seen_pr.lock().unwrap().len()), (1, 3), "a production token goes to the production host");

    let sb = seen_sb.lock().unwrap();
    assert_eq!(sb[0].0, "aaa");
    assert_eq!(header(&sb[0].1, "authorization"), "Bearer PROVIDER-JWT");
    assert_eq!(header(&sb[0].1, "apns-push-type"), "alert");
    assert_eq!(header(&sb[0].1, "apns-topic"), "app.example.bobix");
    assert_eq!(header(&sb[0].1, "apns-priority"), "10");
    assert_eq!(header(&sb[0].1, "apns-collapse-id"), "home");
    assert!(sb[0].2["aps"]["alert"]["title-loc-key"].is_string() && sb[0].2["aps"]["alert"]["loc-key"].is_string());
    assert!(sb[0].2["aps"]["alert"].get("title").is_none() && sb[0].2["aps"]["alert"].get("body").is_none());
    assert_eq!(sb[0].2["sealed"], "SEALED");

    let p = seen_pr.lock().unwrap();
    assert_eq!(header(&p[0].1, "apns-collapse-id"), header(&sb[0].1, "apns-collapse-id"), "a later notice with the same `replaces` has the same collapse id");
    assert_eq!((header(&p[1].1, "apns-push-type"), header(&p[1].1, "apns-priority")), ("background".into(), "5".into()));
    assert_eq!(p[1].2["aps"]["content-available"], 1);
    assert_eq!(header(&p[1].1, "apns-collapse-id"), "");
    assert_eq!(header(&p[2].1, "apns-push-type"), "liveactivity");
    assert_eq!(header(&p[2].1, "apns-topic"), "app.example.bobix.push-type.liveactivity");
    assert_eq!(p[2].2["aps"]["content-state"]["sealed"], "SEALED");
}

#[tokio::test]
async fn apns_rejections_are_told_apart() {
    for (status, want) in [(200u16, Outcome::Sent), (410, Outcome::TokenGone), (500, Outcome::Failed("apns 500 Unregistered".into()))] {
        let (url, _) = stand_in("/3/device/{token}", status).await;
        let cfg = ApnsConfig { key_file: "unused".into(), key_id: "K".into(), team_id: "T".into(), topic: "t".into(), sandbox_url: url.clone(), production_url: url };
        let apns = Apns::with_token(cfg, Box::new(|| Ok("J".into())));
        assert_eq!(apns.send(pr(PushKind::Alert, ApnsEnv::Sandbox, "tok", None, 10)).await, want);
    }
    // A token that is not plain text is refused before it can reach a URL.
    let cfg = ApnsConfig { key_file: "unused".into(), key_id: "K".into(), team_id: "T".into(), topic: "t".into(), sandbox_url: "http://127.0.0.1:1".into(), production_url: "http://127.0.0.1:1".into() };
    let out = Apns::with_token(cfg, Box::new(|| Ok("J".into()))).send(pr(PushKind::Alert, ApnsEnv::Sandbox, "../x?y", None, 10)).await;
    assert!(matches!(out, Outcome::Failed(_)));
}

#[tokio::test]
async fn fcm_gets_a_data_message_with_the_given_priority_and_collapse_key() {
    let (url, seen) = stand_in("/v1/projects/{p}/messages:send", 200).await;
    let fcm = Fcm::with_static_token("proj", &url, "ACCESS");
    let mut r = pr(PushKind::Alert, ApnsEnv::Production, "fcm-tok", Some("home"), 10);
    r.platform = Platform::Android;
    assert_eq!(fcm.send(r.clone()).await, Outcome::Sent);
    r.priority = 5;
    fcm.send(r).await;
    let s = seen.lock().unwrap().clone();
    assert_eq!(header(&s[0].1, "authorization"), "Bearer ACCESS");
    assert_eq!(s[0].2["message"]["token"], "fcm-tok");
    assert_eq!(s[0].2["message"]["data"]["sealed"], "SEALED");
    assert_eq!(s[0].2["message"]["android"]["priority"], "HIGH");
    assert_eq!(s[1].2["message"]["android"]["priority"], "NORMAL");
    assert_eq!(s[0].2["message"]["android"]["collapse_key"], "home");
    assert!(s[0].2["message"].get("notification").is_none(), "a data message: the app builds the notification");

    let (gone, _) = stand_in("/v1/projects/{p}/messages:send", 404).await;
    let mut r = pr(PushKind::Alert, ApnsEnv::Production, "dead", None, 10);
    r.platform = Platform::Android;
    assert_eq!(Fcm::with_static_token("proj", &gone, "A").send(r).await, Outcome::TokenGone);
}

#[tokio::test]
async fn a_relay_without_credentials_of_its_own_is_not_a_gateway() {
    let mut c = cfg();
    c.dev = false; // no APNs or FCM configured, only a gateway to push through
    c.gateway_url = Some("http://127.0.0.1:1".into());
    let s = start(c).await;
    let phone = ident();
    assert_eq!(post(&s, None, req(&phone, "t", "relay-a", now_ms() + DAY)).await.0, 404, "else it would be an open proxy for its key");
}
