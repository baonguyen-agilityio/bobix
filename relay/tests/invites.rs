//! Invite codes over real sockets: an owner (in `OWNER_IDS`) signs a code, a guest computer presents it, the relay
//! admits it and keeps it admitted across a restart. All identities are made up.

mod common;

use bobix_link::invite::{InviteCode, DAY_MS};
use bobix_link::keys::Identity;
use bobix_link::relay_frames::*;
use common::*;
use std::time::Duration;

fn code(owner: &Identity, relay: &str, expires_ms: u64) -> String {
    InviteCode::create(owner, relay, "tester", expires_ms).unwrap()
}

fn week() -> u64 {
    now_ms() + 7 * DAY_MS
}

fn result(ok: bool, reason: Option<InviteReason>) -> impl Fn(&Frame) -> bool {
    move |f| matches!(f, Frame::InviteResult { ok: o, reason: r } if *o == ok && *r == reason)
}

fn is_err(code: ErrorCode) -> impl Fn(&Frame) -> bool {
    move |f| matches!(f, Frame::Error { code: c, .. } if *c == code)
}

/// A relay whose owner is `owner`, keeping its invitations in `file` (memory only when `None`).
async fn relay(owner: &Identity, file: Option<std::path::PathBuf>) -> Server {
    let mut c = cfg();
    c.owner_ids = vec![owner.device_id()];
    c.invited_file = file;
    start(c).await
}

async fn redeem(d: &mut Dev, code: &str) {
    d.send(&Frame::InviteRedeem { code: code.into() }).await;
}

#[tokio::test]
async fn a_valid_code_lets_a_guest_computer_in_and_the_owner_sees_it() {
    let owner = ident();
    let s = relay(&owner, None).await;
    let mut guest = Dev::connect(&s, ident()).await;
    guest.send(&Frame::InviteQuery {}).await;
    guest.expect(|f| matches!(f, Frame::InviteState { owner: false, invited: false, relay_id: None, invitees } if invitees.is_empty()), 1000).await;
    guest.send(&Frame::PairOpen { relay_token_hash: token_pair(1).1, ttl: 120 }).await;
    guest.expect(is_err(ErrorCode::Invite), 1000).await;

    // An owner that is connected hears of the new computer without asking again.
    let mut o = Dev::connect(&s, owner.clone()).await;
    redeem(&mut guest, &code(&owner, "test-relay", week())).await;
    guest.expect(result(true, None), 1000).await;
    guest.expect(|f| matches!(f, Frame::InviteState { invited: true, owner: false, .. }), 1000).await;
    // The same socket may now do what an invited computer does.
    guest.send(&Frame::PairOpen { relay_token_hash: token_pair(2).1, ttl: 120 }).await;
    guest.none(is_err(ErrorCode::Invite), 300).await;

    let gid = guest.id();
    let Frame::InviteState { owner: true, invited: true, relay_id, invitees } = o.expect(|f| matches!(f, Frame::InviteState { invitees, .. } if !invitees.is_empty()), 1000).await else { panic!("not an owner") };
    assert_eq!(relay_id.as_deref(), Some("test-relay"));
    assert_eq!(invitees.len(), 1);
    assert_eq!((invitees[0].id.as_str(), invitees[0].label.as_str(), invitees[0].by.clone()), (gid.as_str(), "tester", owner.device_id()));
}

#[tokio::test]
async fn expired_reused_other_relay_not_owner_and_forged_codes_are_refused() {
    let owner = ident();
    let mut c = cfg();
    c.owner_ids = vec![owner.device_id()];
    c.redeem_per_min = 100; // six tries on one socket here
    let s = start(c).await;
    let mut g = Dev::connect(&s, ident()).await;

    redeem(&mut g, &code(&owner, "test-relay", now_ms() - 1)).await;
    g.expect(result(false, Some(InviteReason::Expired)), 1000).await;
    redeem(&mut g, &code(&owner, "test-relay", now_ms() + 31 * DAY_MS)).await;
    g.expect(result(false, Some(InviteReason::Invalid)), 1000).await;
    redeem(&mut g, &code(&owner, "staging-relay", week())).await;
    g.expect(result(false, Some(InviteReason::OtherRelay)), 1000).await;
    redeem(&mut g, &code(&Identity::generate(), "test-relay", week())).await;
    g.expect(result(false, Some(InviteReason::NotOwner)), 1000).await;
    // A code whose expiry was pushed out after signing.
    let good = code(&owner, "test-relay", week());
    let mut bytes = bobix_link::b64_decode(good.strip_prefix("bobix-invite:").unwrap()).unwrap();
    let at = 2 + "test-relay".len() + 32 + 16;
    bytes[at + 7] ^= 1;
    redeem(&mut g, &format!("bobix-invite:{}", bobix_link::b64(&bytes))).await;
    g.expect(result(false, Some(InviteReason::Invalid)), 1000).await;
    assert!(s.app.hub.lock().unwrap().is_guest(&g.id()), "no refusal let it in");

    // Used once, a code is spent for everyone.
    redeem(&mut g, &good).await;
    g.expect(result(true, None), 1000).await;
    let mut other = Dev::connect(&s, ident()).await;
    redeem(&mut other, &good).await;
    other.expect(result(false, Some(InviteReason::Used)), 1000).await;
}

#[tokio::test]
async fn an_invited_computer_does_not_spend_a_code() {
    let owner = ident();
    let s = relay(&owner, None).await;
    let c = code(&owner, "test-relay", week());
    let mut o = Dev::connect(&s, owner.clone()).await;
    redeem(&mut o, &c).await;
    o.expect(result(true, Some(InviteReason::Already)), 1000).await;
    let mut g = Dev::connect(&s, ident()).await;
    redeem(&mut g, &c).await;
    g.expect(result(true, None), 1000).await;
}

#[tokio::test]
async fn the_owner_removes_a_computer_and_it_is_a_guest_again() {
    let owner = ident();
    let s = relay(&owner, None).await;
    let mut g = Dev::connect(&s, ident()).await;
    redeem(&mut g, &code(&owner, "test-relay", week())).await;
    g.expect(result(true, None), 1000).await;

    // Only an owner may remove.
    let mut stranger = Dev::connect(&s, ident()).await;
    stranger.send(&Frame::InviteRemove { id: g.id() }).await;
    stranger.expect(is_err(ErrorCode::Invite), 1000).await;
    assert!(!s.app.hub.lock().unwrap().is_guest(&g.id()));

    let mut o = Dev::connect(&s, owner.clone()).await;
    o.send(&Frame::InviteRemove { id: g.id() }).await;
    o.expect(|f| matches!(f, Frame::InviteState { owner: true, invitees, .. } if invitees.is_empty()), 1000).await;
    g.closed(2000).await;
    assert!(s.app.hub.lock().unwrap().is_guest(&g.id()));
    // An owner is not on the list and cannot be removed by it.
    o.send(&Frame::InviteRemove { id: owner.device_id() }).await;
    o.expect(|f| matches!(f, Frame::InviteState { .. }), 1000).await;
    assert!(!s.app.hub.lock().unwrap().is_guest(&owner.device_id()));
}

#[tokio::test]
async fn invitations_and_spent_codes_survive_a_restart() {
    let dir = scratch("invited");
    let file = dir.join("invited.json");
    let owner = ident();
    let c = code(&owner, "test-relay", week());
    let guest = ident();
    {
        let s = relay(&owner, Some(file.clone())).await;
        let mut g = Dev::connect(&s, guest.clone()).await;
        redeem(&mut g, &c).await;
        g.expect(result(true, None), 1000).await;
    }
    let s = relay(&owner, Some(file.clone())).await;
    assert!(!s.app.hub.lock().unwrap().is_guest(&guest.device_id()), "still invited after the restart");
    let mut other = Dev::connect(&s, ident()).await;
    redeem(&mut other, &c).await;
    other.expect(result(false, Some(InviteReason::Used)), 1000).await;
    assert!(!dir.join("invited.json.tmp").exists(), "no temporary file is left behind");
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_corrupt_invited_file_stops_the_relay_at_start() {
    let dir = scratch("invited-bad");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("invited.json");
    std::fs::write(&file, b"{ not json").unwrap();
    let mut c = cfg();
    c.owner_ids = vec![ident().device_id()];
    c.invited_file = Some(file);
    let e = bobix_relay::App::from_config(c).err().expect("refused");
    assert!(e.contains("INVITED_FILE"), "{e}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_socket_may_present_only_a_few_codes_a_minute() {
    let owner = ident();
    let mut c = cfg();
    c.owner_ids = vec![owner.device_id()];
    c.redeem_per_min = 2;
    let s = start(c).await;
    let mut g = Dev::connect(&s, ident()).await;
    for _ in 0..2 {
        redeem(&mut g, "bobix-invite:nope").await;
        g.expect(result(false, Some(InviteReason::Invalid)), 1000).await;
    }
    redeem(&mut g, &code(&owner, "test-relay", week())).await;
    g.expect(result(false, Some(InviteReason::TooMany)), 1000).await;
}

#[tokio::test]
async fn a_guest_that_redeems_in_time_is_not_closed_by_the_guest_rule() {
    let owner = ident();
    let mut c = cfg();
    c.owner_ids = vec![owner.device_id()];
    c.guest_within = Duration::from_millis(300);
    let s = start(c).await;
    let mut lazy = Dev::connect(&s, ident()).await;
    let mut g = Dev::connect(&s, ident()).await;
    redeem(&mut g, &code(&owner, "test-relay", week())).await;
    g.expect(result(true, None), 1000).await;
    lazy.closed(5000).await; // the guest rule is checked every `dead_after / 10` (3 s here)
    // Long past the guest window, the redeemer still answers.
    tokio::time::sleep(Duration::from_millis(500)).await;
    g.send(&Frame::InviteQuery {}).await;
    g.expect(|f| matches!(f, Frame::InviteState { invited: true, .. }), 1000).await;
}

#[tokio::test]
async fn a_removed_computer_loses_its_phones_too() {
    let owner = ident();
    let mut c = cfg();
    c.owner_ids = vec![owner.device_id()];
    c.guest_within = Duration::from_millis(500);
    let s = start(c).await;
    let (d_id, p_id) = (ident(), ident());
    let mut d = Dev::connect(&s, d_id.clone()).await;
    redeem(&mut d, &code(&owner, "test-relay", week())).await;
    d.expect(result(true, None), 1000).await;
    // The computer and its phone (a guest) allow each other and talk.
    let mut p = Dev::connect(&s, p_id.clone()).await;
    d.send(&Frame::Peers { allow: vec![p.id()] }).await;
    p.send(&Frame::Peers { allow: vec![d.id()] }).await;
    d.expect(|f| matches!(f, Frame::Presence { state: PresenceState::Online, .. }), 2000).await;
    p.expect(|f| matches!(f, Frame::Presence { state: PresenceState::Online, .. }), 2000).await;
    // A packet waits for the phone's acknowledgement when the owner removes the computer.
    d.send(&send(&p.id(), "before", 600, None, "AAAA")).await;
    p.expect(|f| matches!(f, Frame::Msg { pid, .. } if pid == "before"), 1000).await;

    let mut o = Dev::connect(&s, owner.clone()).await;
    o.send(&Frame::InviteRemove { id: d.id() }).await;
    d.closed(2000).await;
    p.expect(|f| matches!(f, Frame::Presence { state: PresenceState::Offline, .. }), 1000).await;

    // Back as a guest, the computer cannot reach its phone nor be reached by it, and the guest rule closes it.
    let mut d = Dev::connect(&s, d_id).await;
    d.send(&Frame::Peers { allow: vec![p.id()] }).await;
    d.expect(is_err(ErrorCode::Invite), 1000).await;
    d.send(&send(&p.id(), "after", 600, None, "AAAA")).await;
    d.expect(is_err(ErrorCode::NoPeer), 1000).await;
    p.send(&send(&d.id(), "back", 600, None, "AAAA")).await;
    p.expect(is_err(ErrorCode::NoPeer), 1000).await;
    p.none(|f| matches!(f, Frame::Msg { pid, .. } if pid == "after"), 300).await;
    let gone = d.id();
    // The guest rule is checked every `dead_after / 10` (3 s here).
    d.closed(5000).await;
    // The phone reconnecting and naming the computer gets nowhere either.
    drop(p);
    let mut p = Dev::connect(&s, p_id).await;
    p.send(&Frame::Peers { allow: vec![gone.clone()] }).await;
    p.expect(is_err(ErrorCode::Invite), 1000).await;
    assert!(!s.app.hub.lock().unwrap().has_peers(&p.id()), "the phone's old list does not outlive the invitation");
    p.closed(5000).await;
}
