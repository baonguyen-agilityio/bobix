//! Limits and durability: floods, per-address caps, global bounds, what a restart keeps, invitation changes.

mod common;

use bobix_link::relay_frames::*;
use common::*;
use std::time::Duration;

fn is_err(code: ErrorCode) -> impl Fn(&Frame) -> bool {
    move |f| matches!(f, Frame::Error { code: c, .. } if *c == code)
}

#[tokio::test]
async fn a_device_that_floods_frames_is_disconnected_and_others_are_not_affected() {
    let mut c = cfg();
    c.device_frames_per_sec = 50;
    let s = start(c).await;
    let (mut d, mut p) = pair_up(&s).await;
    let mut flooder = Dev::connect(&s, ident()).await;
    let mut cut_off = false;
    for _ in 0..400 {
        if !flooder.try_send_text("x").await {
            cut_off = true; // the relay closed the socket while the flood was still going
            break;
        }
    }
    if !cut_off {
        flooder.closed(3000).await;
    }
    d.send(&send(&p.id(), "still", 600, None, "AAAA")).await;
    p.expect(|f| matches!(f, Frame::Msg { pid, .. } if pid == "still"), 1000).await;
}

#[tokio::test]
async fn a_frame_far_over_the_limit_ends_the_socket() {
    let s = start(cfg()).await;
    let mut d = Dev::connect(&s, ident()).await;
    let _ = d.try_send_text(&"x".repeat(400 * 1024)).await;
    d.closed(3000).await;
}

#[tokio::test]
async fn sockets_that_have_not_authenticated_are_capped_per_address() {
    let mut c = cfg();
    c.max_unauth_per_ip = 2;
    let s = start(c).await;
    let url = format!("ws://{}/v1/ws", s.addr);
    let (a, _b) = (Dev::raw(&s).await, Dev::raw(&s).await);
    assert!(tokio_tungstenite::connect_async(&url).await.is_err(), "a third unauthenticated socket from one address");
    // A socket that goes away frees its slot.
    drop(a);
    let mut tries = 0;
    while tokio_tungstenite::connect_async(&url).await.is_err() {
        tries += 1;
        assert!(tries < 20, "the slot never came back");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn new_connections_from_one_address_are_rate_limited() {
    let mut c = cfg();
    c.conns_per_ip_per_min = 3;
    let s = start(c).await;
    let url = format!("ws://{}/v1/ws", s.addr);
    for _ in 0..3 {
        drop(Dev::connect(&s, ident()).await);
    }
    assert!(tokio_tungstenite::connect_async(&url).await.is_err());
}

#[tokio::test]
async fn mailboxes_are_bounded_in_number_and_in_total_size() {
    let mut c = cfg();
    c.max_mailboxes = 1;
    let s = start(c).await;
    let (mut d, p) = pair_up(&s).await;
    let (mut d2, p2) = pair_up(&s).await;
    let (to1, to2) = (p.id(), p2.id());
    drop((p, p2));
    d.send(&send(&to1, "one", 600, None, "AAAA")).await;
    d.expect(|f| matches!(f, Frame::Queued { .. }), 1000).await;
    d2.send(&send(&to2, "two", 600, None, "AAAA")).await;
    d2.expect(|f| matches!(f, Frame::Dropped { pid } if pid == "two"), 1000).await;

    let mut c = cfg();
    c.max_total_bytes = 1000;
    let s = start(c).await;
    let (mut d, p) = pair_up(&s).await;
    let to = p.id();
    drop(p);
    d.send(&send(&to, "fits", 600, None, &"A".repeat(600))).await;
    d.expect(|f| matches!(f, Frame::Queued { .. }), 1000).await;
    d.send(&send(&to, "too-much", 600, None, &"A".repeat(600))).await;
    d.expect(|f| matches!(f, Frame::Dropped { pid } if pid == "too-much"), 1000).await;
}

#[tokio::test]
async fn what_a_device_set_is_forgotten_after_it_has_been_gone_a_while() {
    let mut c = cfg();
    c.state_keep = Duration::from_millis(300);
    let s = start(c).await;
    let (mut d, p) = pair_up(&s).await;
    let to = p.id();
    drop(p);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    d.send(&send(&to, "after", 600, None, "AAAA")).await;
    d.expect(is_err(ErrorCode::NoPeer), 1000).await; // its allow-list is gone until it connects again and sets it
}

#[tokio::test]
async fn presence_goes_only_to_devices_that_allow_each_other() {
    let s = start(cfg()).await;
    let mut a = Dev::connect(&s, ident()).await;
    let mut b = Dev::connect(&s, ident()).await;
    b.send(&Frame::Peers { allow: vec![a.id()] }).await; // b lets a send to it, but a did not allow b
    b.send(&Frame::State { asleep: true }).await;
    b.sync().await;
    a.none(|f| matches!(f, Frame::Presence { .. }), 300).await;
}

#[tokio::test]
async fn a_revoked_pair_stays_cut_after_a_relay_restart_and_can_still_be_revoked_after_one() {
    let dir = scratch("restart");
    let mut c = cfg();
    c.revoked_dir = Some(dir.clone());
    let s = start(c.clone()).await;
    let (desktop, phone) = (ident(), ident());
    let mut d = Dev::connect(&s, desktop.clone()).await;
    let mut p = Dev::connect(&s, phone.clone()).await;
    d.send(&Frame::Peers { allow: vec![p.id()] }).await;
    p.send(&Frame::Peers { allow: vec![d.id()] }).await;
    p.sync().await;
    p.send(&send(&d.id(), "paired", 600, None, "AAAA")).await; // the phone confirms the pairing
    d.expect(|f| matches!(f, Frame::Msg { .. }), 1000).await;
    drop((d, p));

    // The relay is redeployed; afterwards the desktop revokes the phone (what it paired survived the restart).
    let s2 = start(c.clone()).await;
    let mut d = Dev::connect(&s2, desktop.clone()).await;
    d.send(&Frame::Revoke { id: phone.device_id() }).await;
    d.sync().await;
    d.none(is_err(ErrorCode::NoPeer), 300).await;
    drop(d);

    // Another redeploy: the cut is still there. The phone is told on connect and cannot reach that desktop.
    let s3 = start(c).await;
    let mut d = Dev::connect(&s3, desktop).await;
    let mut p = Dev::connect(&s3, phone).await;
    p.expect(is_err(ErrorCode::Revoked), 1000).await;
    d.send(&Frame::Peers { allow: vec![p.id()] }).await;
    p.send(&Frame::Peers { allow: vec![d.id()] }).await;
    p.sync().await;
    p.send(&send(&d.id(), "x", 600, None, "AAAA")).await;
    p.expect(is_err(ErrorCode::Revoked), 1000).await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn an_open_pairing_token_cannot_be_taken_over_by_another_desktop() {
    let s = start(cfg()).await;
    let (mut d1, mut d2, mut p) = (Dev::connect(&s, ident()).await, Dev::connect(&s, ident()).await, Dev::connect(&s, ident()).await);
    let (token, hash) = token_pair(6);
    d1.send(&Frame::PairOpen { relay_token_hash: hash.clone(), ttl: 120 }).await;
    d1.sync().await;
    d2.send(&Frame::PairOpen { relay_token_hash: hash, ttl: 120 }).await;
    d2.expect(is_err(ErrorCode::Other), 1000).await;
    p.send(&Frame::PairSend { relay_token: token, body: "hello".into() }).await;
    d1.expect(|f| matches!(f, Frame::Msg { .. }), 1000).await;
    d2.none(|f| matches!(f, Frame::Msg { .. }), 300).await;
}

#[tokio::test]
async fn pushes_through_the_relays_own_credentials_are_rate_limited() {
    let mut c = cfg();
    c.push_per_token = 3;
    let s = start(c).await;
    let (mut d, mut p) = pair_up(&s).await;
    let g = grant(&p.ident, "limited", "test-relay", now_ms() + 86_400_000);
    p.send(&Frame::PushToken { platform: Platform::Ios, env: ApnsEnv::Sandbox, kind: TokenKind::Device, r#ref: None, token: "limited".into(), grant: g }).await;
    p.sync().await;
    for i in 0..6 {
        let f = Frame::Send { pid: format!("n{i}"), to: p.id(), ttl: 600, collapse: None, push: Some(Push { kind: PushKind::Alert, priority: 10, replaces: None }), body: "X".into() };
        d.send(&f).await;
        p.expect(|f| matches!(f, Frame::Msg { .. }), 1000).await; // the packets all arrive over the socket
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(s.app.fake.as_ref().unwrap().sent.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn behind_a_proxy_the_gateway_limits_by_the_address_the_proxy_reports() {
    let mut c = cfg();
    c.gateway_per_caller = 2;
    c.client_ip_header = Some("x-forwarded-for".into());
    let s = start(c).await;
    let phone = ident();
    let call = |ip: &'static str| {
        let body = serde_json::json!({
            "platform": "ios", "kind": "alert", "env": "sandbox", "token": "t", "relay": "r",
            "grant": grant(&phone, "t", "r", now_ms() + 86_400_000), "body": "X"
        });
        let url = format!("http://{}/v1/push", s.addr);
        async move { reqwest::Client::new().post(url).header("x-forwarded-for", format!("9.9.9.9, {ip}")).json(&body).send().await.unwrap().status().as_u16() }
    };
    assert_eq!((call("1.1.1.1").await, call("1.1.1.1").await, call("1.1.1.1").await), (200, 200, 429));
    assert_eq!(call("2.2.2.2").await, 200, "another client behind the same proxy has its own budget");
}

#[tokio::test]
async fn removing_a_desktop_from_the_invitation_file_takes_its_rights_away_while_connected() {
    let dir = scratch("invite-change");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("invites.txt");
    let desktop = ident();
    std::fs::write(&file, format!("{}\n", desktop.device_id())).unwrap();
    let mut c = cfg();
    c.invite_file = Some(file.clone());
    c.invite_recheck = Duration::from_millis(100);
    let s = start(c).await;
    let mut d = Dev::connect(&s, desktop.clone()).await;
    d.send(&Frame::PairOpen { relay_token_hash: token_pair(1).1, ttl: 120 }).await;
    d.sync().await;
    d.none(is_err(ErrorCode::Invite), 200).await;

    tokio::time::sleep(Duration::from_millis(50)).await;
    std::fs::write(&file, "# nobody\n").unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    d.send(&Frame::PairOpen { relay_token_hash: token_pair(2).1, ttl: 120 }).await;
    d.expect(is_err(ErrorCode::Invite), 1000).await;
    std::fs::remove_dir_all(dir).unwrap();
}
