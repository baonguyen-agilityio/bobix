//! End-to-end tests of the relay over real sockets: devices are real `bobix-link` identities, the relay runs
//! in-process on a free local port. All data is made up.

mod common;

use bobix_link::relay_frames::*;
use common::*;
use std::time::Duration;

const MS: fn(u64) -> Duration = Duration::from_millis;

fn is_err(code: ErrorCode) -> impl Fn(&Frame) -> bool {
    move |f| matches!(f, Frame::Error { code: c, .. } if *c == code)
}

fn is_msg(pid: &str) -> impl Fn(&Frame) -> bool + '_ {
    move |f| matches!(f, Frame::Msg { pid: p, .. } if p == pid)
}

fn presence(peer: &str, state: PresenceState) -> impl Fn(&Frame) -> bool + '_ {
    move |f| matches!(f, Frame::Presence { peer: p, state: s } if p == peer && *s == state)
}

#[tokio::test]
async fn healthz_answers_and_a_replayed_auth_signature_is_refused() {
    let s = start(cfg()).await;
    assert_eq!(reqwest::get(format!("http://{}/healthz", s.addr)).await.unwrap().text().await.unwrap(), "ok");

    let a = ident();
    let mut first = Dev::raw(&s).await;
    let Frame::Challenge { nonce } = first.next(1000).await else { panic!("no challenge") };
    let auth = auth_frame(&a, &first.url, &nonce);
    first.send(&auth).await;
    drop(first);

    // The same signature on a new connection answers an older challenge: refused.
    let mut second = Dev::raw(&s).await;
    let Frame::Challenge { nonce: n2 } = second.next(1000).await else { panic!("no challenge") };
    assert_ne!(nonce, n2);
    second.send(&auth).await;
    second.expect(is_err(ErrorCode::Auth), 1000).await;
    // A signature by another key, or for another id, is refused as well.
    let mut third = Dev::raw(&s).await;
    let Frame::Challenge { nonce: n3 } = third.next(1000).await else { panic!() };
    let Frame::Auth { key, relay, sig, versions, .. } = auth_frame(&ident(), &third.url, &n3) else { panic!() };
    third.send(&Frame::Auth { id: a.device_id(), key, relay, sig, versions }).await;
    third.expect(is_err(ErrorCode::Auth), 1000).await;
}

#[tokio::test]
async fn a_packet_is_acknowledged_end_to_end_and_the_sender_hears_queued_then_delivered() {
    let s = start(cfg()).await;
    let (mut d, mut p) = pair_up(&s).await;
    d.send(&send(&p.id(), "p1", 600, None, "AAAA")).await;
    d.expect(|f| matches!(f, Frame::Queued { pid } if pid == "p1"), 1000).await;
    p.expect(is_msg("p1"), 1000).await;
    p.send(&Frame::Ack { pid: "p1".into() }).await;
    d.expect(|f| matches!(f, Frame::Delivered { pid } if pid == "p1"), 1000).await;
}

#[tokio::test]
async fn an_unacknowledged_packet_is_sent_again_after_the_socket_dropped() {
    let s = start(cfg()).await;
    let (mut d, mut p) = pair_up(&s).await;
    d.send(&send(&p.id(), "p1", 600, None, "AAAA")).await;
    p.expect(is_msg("p1"), 1000).await;
    let phone = p.ident.clone();
    drop(p); // gone before the ack
    d.expect(presence(&phone.device_id(), PresenceState::Offline), 1000).await;
    let mut p = Dev::connect(&s, phone).await;
    p.expect(is_msg("p1"), 1000).await;
    p.send(&Frame::Ack { pid: "p1".into() }).await;
    d.expect(|f| matches!(f, Frame::Delivered { .. }), 1000).await;
}

#[tokio::test]
async fn a_packet_past_its_ttl_is_dropped_and_the_sender_told() {
    let s = start(cfg()).await;
    let (mut d, p) = pair_up(&s).await;
    let phone = p.ident.clone();
    drop(p);
    d.send(&send(&phone.device_id(), "old", 1, None, "AAAA")).await;
    d.expect(|f| matches!(f, Frame::Queued { .. }), 1000).await;
    d.expect(|f| matches!(f, Frame::Dropped { pid } if pid == "old"), 3000).await;
    // It is not delivered afterwards either.
    let mut p = Dev::connect(&s, phone).await;
    p.none(is_msg("old"), 400).await;
}

#[tokio::test]
async fn withdraw_before_delivery_succeeds_and_after_delivery_is_too_late() {
    let s = start(cfg()).await;
    let (mut d, p) = pair_up(&s).await;
    let phone = p.ident.clone();
    drop(p);
    d.send(&send(&phone.device_id(), "w1", 600, None, "AAAA")).await;
    d.expect(|f| matches!(f, Frame::Queued { .. }), 1000).await;
    d.send(&Frame::Withdraw { pid: "w1".into() }).await;
    d.expect(|f| matches!(f, Frame::Withdrawn { pid } if pid == "w1"), 1000).await;
    let mut p = Dev::connect(&s, phone).await;
    p.none(is_msg("w1"), 300).await;

    d.send(&send(&p.id(), "w2", 600, None, "AAAA")).await;
    p.expect(is_msg("w2"), 1000).await;
    p.send(&Frame::Ack { pid: "w2".into() }).await;
    d.expect(|f| matches!(f, Frame::Delivered { pid } if pid == "w2"), 1000).await;
    d.send(&Frame::Withdraw { pid: "w2".into() }).await;
    d.expect(|f| matches!(f, Frame::TooLate { pid } if pid == "w2"), 1000).await;
}

#[tokio::test]
async fn only_the_sender_can_withdraw_and_only_the_recipient_can_acknowledge() {
    let s = start(cfg()).await;
    let (mut d, p) = pair_up(&s).await;
    let mut m = Dev::connect(&s, ident()).await;
    let phone = p.ident.clone();
    drop(p);
    d.send(&send(&phone.device_id(), "x1", 600, None, "AAAA")).await;
    d.expect(|f| matches!(f, Frame::Queued { .. }), 1000).await;
    m.send(&Frame::Withdraw { pid: "x1".into() }).await;
    m.expect(|f| matches!(f, Frame::TooLate { .. }), 1000).await;
    m.send(&Frame::Ack { pid: "x1".into() }).await; // a stranger cannot make the relay say `delivered`
    d.none(|f| matches!(f, Frame::Delivered { .. }), 300).await;
}

#[tokio::test]
async fn collapse_replaces_the_older_packet_with_the_same_key_only() {
    let s = start(cfg()).await;
    let (mut d, p) = pair_up(&s).await;
    let phone = p.ident.clone();
    drop(p);
    let to = phone.device_id();
    for (pid, key) in [("a1", Some("home")), ("b1", Some("other")), ("a2", Some("home"))] {
        d.send(&send(&to, pid, 600, key, "AAAA")).await;
    }
    d.expect(|f| matches!(f, Frame::Dropped { pid } if pid == "a1"), 1000).await;
    let mut p = Dev::connect(&s, phone).await;
    p.expect(is_msg("b1"), 1000).await;
    p.expect(is_msg("a2"), 1000).await;
    p.none(is_msg("a1"), 300).await;
}

#[tokio::test]
async fn the_65th_packet_and_the_2_mib_mark_are_refused() {
    let s = start(cfg()).await;
    let (mut d, p) = pair_up(&s).await;
    let phone = p.ident.clone();
    drop(p);
    let to = phone.device_id();
    for i in 0..64 {
        d.send(&send(&to, &format!("n{i}"), 600, None, "AAAA")).await;
    }
    d.send(&send(&to, "n64", 600, None, "AAAA")).await;
    d.expect(|f| matches!(f, Frame::Dropped { pid } if pid == "n64"), 2000).await;

    // A second recipient: 8 × 250 000 bytes fit under 2 MiB, the 9th does not.
    let (mut d2, p2) = pair_up(&s).await;
    let to2 = p2.id();
    drop(p2);
    let big = "A".repeat(250_000);
    for i in 0..9 {
        d2.send(&send(&to2, &format!("b{i}"), 600, None, &big)).await;
    }
    d2.expect(|f| matches!(f, Frame::Dropped { pid } if pid == "b8"), 2000).await;
}

#[tokio::test]
async fn notices_wait_for_a_sender_that_is_offline() {
    let s = start(cfg()).await;
    let (mut d, mut p) = pair_up(&s).await;
    d.send(&send(&p.id(), "q1", 600, None, "AAAA")).await;
    p.expect(is_msg("q1"), 1000).await;
    let desktop = d.ident.clone();
    drop(d); // the sender leaves before the ack
    // Wait until the relay has noticed it is gone: a notice sent to a socket that is dying unseen is lost.
    p.expect(presence(&desktop.device_id(), PresenceState::Offline), 2000).await;
    p.send(&Frame::Ack { pid: "q1".into() }).await;
    p.sync().await;
    let mut d = Dev::connect(&s, desktop).await;
    d.expect(|f| matches!(f, Frame::Delivered { pid } if pid == "q1"), 1000).await;
}

#[tokio::test]
async fn an_oversize_frame_gets_an_error_and_the_connection_stays_usable() {
    let s = start(cfg()).await;
    let (mut d, mut p) = pair_up(&s).await;
    d.send_text(&format!("{{\"type\":\"ack\",\"pid\":\"{}\"}}", "x".repeat(300_000))).await;
    d.expect(is_err(ErrorCode::Oversize), 1000).await;
    d.send(&send(&p.id(), "after", 600, None, "AAAA")).await;
    p.expect(is_msg("after"), 1000).await;
    d.send_text("not json").await;
    d.expect(is_err(ErrorCode::Malformed), 1000).await;
}

#[tokio::test]
async fn before_authenticating_a_big_frame_and_silence_both_end_the_socket() {
    let mut c = cfg();
    c.auth_within = Duration::from_millis(600); // instead of 10 s
    let s = start(c).await;
    let mut big = Dev::raw(&s).await;
    big.next(1000).await;
    big.send_text(&format!("{{\"type\":\"ack\",\"pid\":\"{}\"}}", "x".repeat(5 * 1024))).await;
    big.expect(is_err(ErrorCode::Oversize), 1000).await;
    big.closed(1500).await;

    let mut quiet = Dev::raw(&s).await;
    quiet.next(1000).await;
    quiet.closed(2500).await; // the test config gives 600 ms instead of 10 s
}

#[tokio::test]
async fn a_packet_is_delivered_only_when_the_recipient_allowed_the_sender() {
    let s = start(cfg()).await;
    let mut a = Dev::connect(&s, ident()).await;
    let mut b = Dev::connect(&s, ident()).await;
    a.send(&send(&b.id(), "n1", 600, None, "AAAA")).await;
    a.expect(is_err(ErrorCode::NoPeer), 1000).await;
    b.none(is_msg("n1"), 300).await;
    // Allowing is one-way; sending to oneself is refused too.
    b.send(&Frame::Peers { allow: vec![a.id()] }).await;
    a.expect(presence(&b.id(), PresenceState::Online), 1000).await;
    a.send(&send(&b.id(), "n2", 600, None, "AAAA")).await;
    b.expect(is_msg("n2"), 1000).await;
    b.send(&send(&a.id(), "n3", 600, None, "AAAA")).await;
    b.expect(is_err(ErrorCode::NoPeer), 1000).await;
    a.send(&send(&a.id(), "self", 600, None, "AAAA")).await;
    a.expect(is_err(ErrorCode::NoPeer), 1000).await;
}

#[tokio::test]
async fn dropping_an_id_from_peers_stops_delivery_at_once() {
    let s = start(cfg()).await;
    let (mut d, mut p) = pair_up(&s).await;
    d.send(&send(&p.id(), "k1", 600, None, "AAAA")).await;
    p.expect(is_msg("k1"), 1000).await;
    p.send(&Frame::Peers { allow: vec![] }).await;
    p.sync().await;
    d.expect(|f| matches!(f, Frame::Dropped { pid } if pid == "k1"), 1000).await;
    d.send(&send(&p.id(), "k2", 600, None, "AAAA")).await;
    d.expect(is_err(ErrorCode::NoPeer), 1000).await;
}

#[tokio::test]
async fn pairing_forwards_up_to_four_answers_to_an_open_token_and_not_more_nor_late() {
    let s = start(cfg()).await;
    let mut d = Dev::connect(&s, ident()).await;
    let (token, hash) = token_pair(1);
    d.send(&Frame::PairOpen { relay_token_hash: hash, ttl: 120 }).await;
    let mut phones = Vec::new();
    for i in 0..5 {
        let mut p = Dev::connect(&s, ident()).await;
        p.send(&Frame::PairSend { relay_token: token.clone(), body: format!("hello{i}") }).await;
        p.sync().await; // the relay has handled this answer before the next phone sends its own
        phones.push(p);
    }
    for _ in 0..4 {
        d.expect(|f| matches!(f, Frame::Msg { body, .. } if body.starts_with("hello")), 1000).await;
    }
    phones[4].expect(is_err(ErrorCode::PairExpired), 1000).await;
    d.none(|f| matches!(f, Frame::Msg { .. }), 300).await;

    // A short-lived token forwards nothing once it ran out; an unknown one is refused outright.
    let (t2, h2) = token_pair(2);
    d.send(&Frame::PairOpen { relay_token_hash: h2, ttl: 1 }).await;
    tokio::time::sleep(MS(1300)).await;
    let mut p = Dev::connect(&s, ident()).await;
    p.send(&Frame::PairSend { relay_token: t2, body: "late".into() }).await;
    // Past its time the token is either remembered as expired or already forgotten: both refuse.
    p.expect(|f| is_err(ErrorCode::PairExpired)(f) || is_err(ErrorCode::NoPeer)(f), 1000).await;
    p.send(&Frame::PairSend { relay_token: token_pair(99).0, body: "who".into() }).await;
    p.expect(is_err(ErrorCode::NoPeer), 1000).await;
}

#[tokio::test]
async fn a_fifth_open_pairing_token_is_refused() {
    let s = start(cfg()).await;
    let mut d = Dev::connect(&s, ident()).await;
    for i in 0..5 {
        d.send(&Frame::PairOpen { relay_token_hash: token_pair(10 + i).1, ttl: 120 }).await;
    }
    d.expect(is_err(ErrorCode::Other), 1000).await;
    d.none(is_err(ErrorCode::Other), 300).await; // exactly one refusal
}

#[tokio::test]
async fn presence_online_offline_after_a_silent_death_and_asleep() {
    let mut c = cfg();
    c.dead_after = Duration::from_millis(1500); // instead of 30 s
    let s = start(c).await;
    let (mut d, mut p) = pair_up(&s).await;
    let desk = d.id();

    // The desktop says it sleeps: asleep, and still asleep (not offline) when the socket goes.
    d.send(&Frame::State { asleep: true }).await;
    p.expect(presence(&desk, PresenceState::Asleep), 1000).await;
    let desktop = d.ident.clone();
    drop(d);
    p.none(presence(&desk, PresenceState::Offline), 500).await;

    // It wakes up and then dies without a goodbye: no pong inside `dead_after` means offline.
    let d = Dev::connect(&s, desktop).await;
    p.expect(presence(&desk, PresenceState::Online), 1000).await;
    // `d` is not read from now on, so it answers no ping.
    p.expect(presence(&desk, PresenceState::Offline), 3000).await;
    drop(d);
}

#[tokio::test]
async fn a_packet_queued_while_a_desktop_was_gone_arrives_when_it_returns() {
    let s = start(cfg()).await;
    let (d, mut p) = pair_up(&s).await;
    let desktop = d.ident.clone();
    drop(d);
    p.expect(presence(&desktop.device_id(), PresenceState::Offline), 1000).await;
    p.send(&send(&desktop.device_id(), "while-away", 600, None, "AAAA")).await;
    p.expect(|f| matches!(f, Frame::Queued { .. }), 1000).await;
    let mut d = Dev::connect(&s, desktop).await;
    d.expect(is_msg("while-away"), 1000).await;
}

#[tokio::test]
async fn a_revoked_device_gets_revoked_on_connect_and_on_every_frame_also_after_a_restart() {
    let dir = scratch("revoked");
    let mut c = cfg();
    c.revoked_dir = Some(dir.clone());
    let s = start(c.clone()).await;
    let (mut d, mut p) = pair_up(&s).await;
    answer_pairing(&mut d, &mut p, 7).await; // only a device that answered one of its pairings can be revoked by a desktop
    let phone = p.ident.clone();
    d.send(&Frame::Revoke { id: p.id() }).await;
    p.expect(is_err(ErrorCode::Revoked), 1000).await;
    p.closed(1000).await;

    let mut again = Dev::connect(&s, phone.clone()).await;
    again.expect(is_err(ErrorCode::Revoked), 1000).await;
    again.send(&send(&d.id(), "x", 600, None, "AAAA")).await;
    again.expect(is_err(ErrorCode::Revoked), 1000).await;
    again.send(&Frame::Peers { allow: vec![d.id()] }).await;
    again.expect(is_err(ErrorCode::Revoked), 1000).await;
    d.none(|f| matches!(f, Frame::Msg { .. }), 300).await;

    // A new relay process reads the same directory.
    let s2 = start(c).await;
    let mut after = Dev::connect(&s2, phone).await;
    after.expect(is_err(ErrorCode::Revoked), 1000).await;
    std::fs::remove_dir_all(dir).unwrap();
}

async fn answer_pairing(d: &mut Dev, p: &mut Dev, n: u8) {
    let (token, hash) = token_pair(n);
    d.send(&Frame::PairOpen { relay_token_hash: hash, ttl: 120 }).await;
    d.sync().await;
    p.send(&Frame::PairSend { relay_token: token, body: "hello".into() }).await;
    d.expect(|f| matches!(f, Frame::Msg { body, .. } if body == "hello"), 1000).await;
}

#[tokio::test]
async fn nobody_can_revoke_a_device_that_did_not_answer_its_pairing() {
    let s = start(cfg()).await;
    let (mut d, mut p) = pair_up(&s).await;
    let stranger = ident();
    let mut ok = Dev::connect(&s, stranger.clone()).await;
    // A desktop that merely lists a stranger in `peers` cannot then ban it (a device sets its own allow-list).
    d.send(&Frame::Peers { allow: vec![p.id(), stranger.device_id()] }).await;
    d.sync().await;
    d.send(&Frame::Revoke { id: stranger.device_id() }).await;
    d.expect(is_err(ErrorCode::NoPeer), 1000).await;
    ok.none(is_err(ErrorCode::Revoked), 300).await;
    // A phone cannot revoke its desktop, and a desktop cannot revoke what it never paired with.
    p.send(&Frame::Revoke { id: d.id() }).await;
    p.expect(is_err(ErrorCode::NoPeer), 1000).await;
    d.send(&Frame::Revoke { id: p.id() }).await;
    d.expect(is_err(ErrorCode::NoPeer), 1000).await;
    let mut again = Dev::connect(&s, d.ident.clone()).await;
    again.none(is_err(ErrorCode::Revoked), 300).await;
}

#[tokio::test]
async fn invited_computers_only_when_an_invite_file_is_set() {
    let dir = scratch("invites");
    std::fs::create_dir_all(&dir).unwrap();
    let invited = ident();
    let file = dir.join("invites.txt");
    std::fs::write(&file, format!("# closed beta\n{}  # me\n", invited.device_id())).unwrap();
    let mut c = cfg();
    c.invite_file = Some(file);
    let s = start(c).await;

    // An uninvited desktop gets `invite` for what a desktop does (opening a pairing, allowing phones).
    let mut stranger = Dev::connect(&s, ident()).await;
    stranger.send(&Frame::PairOpen { relay_token_hash: token_pair(1).1, ttl: 120 }).await;
    stranger.expect(is_err(ErrorCode::Invite), 1000).await;
    stranger.send(&Frame::Peers { allow: vec![ident().device_id()] }).await;
    stranger.expect(is_err(ErrorCode::Invite), 1000).await;

    // An invited desktop is served, and the phone that answers its pairing is admitted through it.
    let mut d = Dev::connect(&s, invited).await;
    let (token, hash) = token_pair(2);
    d.send(&Frame::PairOpen { relay_token_hash: hash, ttl: 120 }).await;
    d.none(is_err(ErrorCode::Invite), 300).await;
    let mut p = Dev::connect(&s, ident()).await;
    p.send(&Frame::Peers { allow: vec![d.id()] }).await;
    p.send(&Frame::PairSend { relay_token: token, body: "hello".into() }).await;
    d.expect(|f| matches!(f, Frame::Msg { body, .. } if body == "hello"), 1000).await;
    d.send(&Frame::Peers { allow: vec![p.id()] }).await;
    d.send(&send(&p.id(), "hi", 600, None, "AAAA")).await;
    p.expect(is_msg("hi"), 1000).await;
    p.send(&send(&d.id(), "hi-back", 600, None, "AAAA")).await;
    d.expect(is_msg("hi-back"), 1000).await;

    // A phone whose desktop is not invited is not admitted through it: its `peers` list names no invited computer.
    let mut other_phone = Dev::connect(&s, ident()).await;
    other_phone.send(&Frame::Peers { allow: vec![stranger.id()] }).await;
    other_phone.expect(is_err(ErrorCode::Invite), 1000).await;

    // Two guests cannot use an invited id in their lists to reach each other.
    let mut g2 = Dev::connect(&s, ident()).await;
    g2.send(&Frame::Peers { allow: vec![d.id(), stranger.id()] }).await;
    g2.sync().await;
    stranger.send(&send(&g2.id(), "sneak", 600, None, "AAAA")).await;
    stranger.expect(is_err(ErrorCode::NoPeer), 1000).await;
    g2.none(is_msg("sneak"), 300).await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn without_an_invite_file_any_desktop_is_admitted() {
    let s = start(cfg()).await;
    let mut d = Dev::connect(&s, ident()).await;
    d.send(&Frame::PairOpen { relay_token_hash: token_pair(1).1, ttl: 120 }).await;
    d.none(is_err(ErrorCode::Invite), 300).await;
}

#[tokio::test]
async fn a_signature_made_for_another_relay_address_is_refused_with_its_own_error() {
    let s = start(cfg()).await;
    let id = ident();
    let log_path = s.addr.to_string();

    // Valid signature, but for another relay: the hostile-relay case, and the misconfigured-URL case.
    let mut d = Dev::raw(&s).await;
    let Frame::Challenge { nonce } = d.next(2000).await else { panic!() };
    d.send(&auth_frame(&id, "wss://some-other-relay.example", &nonce)).await;
    d.expect(is_err(ErrorCode::RelayUrl), 1000).await;
    d.closed(2000).await;

    // A frame that claims this relay but is signed for another is simply invalid.
    let mut liar = Dev::raw(&s).await;
    let Frame::Challenge { nonce } = liar.next(2000).await else { panic!() };
    let Frame::Auth { id: i, key, sig, versions, .. } = auth_frame(&id, "wss://some-other-relay.example", &nonce) else { panic!() };
    liar.send(&Frame::Auth { id: i, key, relay: s_url(&log_path), sig, versions }).await;
    liar.expect(is_err(ErrorCode::Auth), 1000).await;

    // The same relay written another way (`localhost` for 127.0.0.1) is the same address.
    let mut ok = Dev::raw(&s).await;
    let Frame::Challenge { nonce } = ok.next(2000).await else { panic!() };
    let port = s.addr.port();
    ok.send(&auth_frame(&id, &format!("ws://localhost:{port}/"), &nonce)).await;
    ok.sync().await;
    ok.none(|f| matches!(f, Frame::Error { code: ErrorCode::RelayUrl | ErrorCode::Auth, .. }), 300).await;
}

fn s_url(addr: &str) -> String {
    format!("ws://{addr}")
}
