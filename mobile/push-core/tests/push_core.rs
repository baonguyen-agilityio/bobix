mod support;

use bobix_link::envelope::{seal_with_tail, Direction, Lane, Stream};
use bobix_push::*;
use crypto_box::{PublicKey, SecretKey};
use std::path::PathBuf;
use support::*;

fn store(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("bobix-push-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("push.json").to_string_lossy().into_owned()
}

fn paired(name: &str) -> String {
    let path = store(name);
    store_set_computer(path.clone(), "desk1".into(), desktop_box_public().to_vec(), 1, 1).unwrap();
    path
}

fn open(path: &str, p: &[u8]) -> Result<OpenedNotice, PushError> {
    open_notice(path.into(), PHONE_BOX_SECRET.to_vec(), sealed(p))
}

fn waiting(counter: u64) -> Vec<u8> {
    packet(Stream::Event, counter, 1, DESKTOP_BOX_SECRET, &notice("waiting", "r1", "shop-app needs you", "Bash wants to run: curl -H [redacted]"))
}

#[test]
fn a_notice_opens_with_its_fields() {
    let path = paired("opens");
    let n = open(&path, &waiting(100)).unwrap();
    assert_eq!((n.what.as_str(), n.reference.as_str(), n.computer_id.as_str(), n.counter), ("waiting", "r1", "desk1", 100));
    assert_eq!((n.title.as_str(), n.body.as_str()), ("shop-app needs you", "Bash wants to run: curl -H [redacted]"));
}

#[test]
fn the_three_kinds_open() {
    let path = paired("kinds");
    for (i, what) in ["waiting", "closed", "done"].iter().enumerate() {
        let p = packet(Stream::Event, 10 + i as u64, 1, DESKTOP_BOX_SECRET, &notice(what, "r", "t", "b"));
        assert_eq!(open(&path, &p).unwrap().what, *what);
    }
}

#[test]
fn a_replayed_notice_is_refused_and_a_second_distinct_one_still_opens() {
    let path = paired("replay");
    open(&path, &waiting(100)).unwrap();
    assert!(matches!(open(&path, &waiting(100)), Err(PushError::Replayed)));
    assert!(open(&path, &waiting(101)).is_ok());
}

#[test]
fn a_notice_older_than_the_window_is_refused_and_changes_nothing() {
    let path = paired("late");
    open(&path, &waiting(5000)).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(open(&path, &waiting(3)), Err(PushError::Replayed)));
    assert_eq!(std::fs::read(&path).unwrap(), before, "a refused push leaves the file as it was");
    // Inside the window and not yet seen: accepted (pushes can arrive out of order).
    assert!(open(&path, &waiting(4900)).is_ok());
}

#[test]
fn a_changed_packet_and_a_stranger_are_unreadable() {
    let path = paired("forged");
    let mut p = waiting(100);
    let last = p.len() - 1;
    p[last] ^= 1;
    assert!(matches!(open(&path, &p), Err(PushError::Unreadable)));
    let stranger = packet(Stream::Event, 100, 1, STRANGER_BOX_SECRET, &notice("waiting", "r", "t", "b"));
    assert!(matches!(open(&path, &stranger), Err(PushError::Unreadable)));
    // A forged packet moved nothing: the real one still opens.
    assert!(open(&path, &waiting(100)).is_ok());
}

#[test]
fn nothing_opens_without_a_paired_computer_or_with_a_wrong_key_or_garbage() {
    let path = store("none");
    assert!(matches!(open(&path, &waiting(1)), Err(PushError::Unreadable)));
    let path = paired("garbage");
    assert!(matches!(open_notice(path.clone(), PHONE_BOX_SECRET.to_vec(), "!!not base64!!".into()), Err(PushError::Unreadable)));
    assert!(matches!(open_notice(path.clone(), vec![1; 5], "AAAA".into()), Err(PushError::Invalid { .. })));
    assert!(matches!(open(&path, &[1, 2, 3]), Err(PushError::Unreadable)));
    assert!(matches!(open_notice(path, vec![7; 32], sealed(&waiting(1))), Err(PushError::Unreadable)));
}

#[test]
fn a_packet_that_is_not_a_notice_is_left_for_the_app() {
    let path = paired("other");
    let home = packet(Stream::State, 7, 1, DESKTOP_BOX_SECRET, r#"{"kind":"home"}"#);
    let paired_msg = packet(Stream::Event, 8, 1, DESKTOP_BOX_SECRET, r#"{"kind":"paired"}"#);
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(open(&path, &home), Err(PushError::NotANotice)));
    assert!(matches!(open(&path, &paired_msg), Err(PushError::NotANotice)));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // The app can still take both, because the push did not consume them.
    assert_eq!(open_packet(path.clone(), PHONE_BOX_SECRET.to_vec(), paired_msg).unwrap().stream, "event");
    assert_eq!(open_packet(path, PHONE_BOX_SECRET.to_vec(), home).unwrap().stream, "state");
}

#[test]
fn what_the_app_took_is_a_replay_for_the_extension_and_the_other_way_round() {
    let path = paired("shared");
    open_packet(path.clone(), PHONE_BOX_SECRET.to_vec(), waiting(50)).unwrap();
    assert!(matches!(open(&path, &waiting(50)), Err(PushError::Replayed)));
    open(&path, &waiting(51)).unwrap();
    assert!(matches!(open_packet(path, PHONE_BOX_SECRET.to_vec(), waiting(51)), Err(PushError::Replayed)));
}

#[test]
fn a_packet_in_the_phones_own_direction_and_a_version_outside_the_range_are_refused() {
    let path = paired("lane");
    let reflected = seal_with_tail(
        1,
        Lane { dir: Direction::ToDesktop, stream: Stream::Event },
        100,
        [0xb0; 14],
        &SecretKey::from(DESKTOP_BOX_SECRET),
        &PublicKey::from(phone_box_public()),
        notice("waiting", "r", "t", "b").as_bytes(),
    );
    assert!(matches!(open(&path, &reflected), Err(PushError::Unreadable)));
    let v9 = packet(Stream::Event, 100, 9, DESKTOP_BOX_SECRET, &notice("waiting", "r", "t", "b"));
    assert!(matches!(open(&path, &v9), Err(PushError::Version)));
}

#[test]
fn two_computers_are_told_apart_and_removing_one_leaves_the_other() {
    let path = paired("two");
    let other_secret = [3u8; 32];
    let other_public = PublicKey::from(&SecretKey::from(other_secret)).to_bytes();
    store_set_computer(path.clone(), "desk2".into(), other_public.to_vec(), 1, 1).unwrap();
    let from2 = packet(Stream::Event, 5, 1, other_secret, &notice("done", "r2", "t", "b"));
    assert_eq!(open(&path, &from2).unwrap().computer_id, "desk2");
    assert_eq!(open(&path, &waiting(5)).unwrap().computer_id, "desk1");
    store_remove_computer(path.clone(), "desk2".into()).unwrap();
    assert!(matches!(open(&path, &packet(Stream::Event, 6, 1, other_secret, &notice("done", "r2", "t", "b"))), Err(PushError::Unreadable)));
    assert!(open(&path, &waiting(6)).is_ok());
}

#[test]
fn setting_a_computer_again_keeps_its_window_unless_its_key_changed() {
    let path = paired("keep");
    open(&path, &waiting(100)).unwrap();
    store_set_computer(path.clone(), "desk1".into(), desktop_box_public().to_vec(), 1, 1).unwrap();
    assert!(matches!(open(&path, &waiting(100)), Err(PushError::Replayed)), "the same key keeps what was seen");
    let new_secret = [8u8; 32];
    let new_public = PublicKey::from(&SecretKey::from(new_secret)).to_bytes();
    store_set_computer(path.clone(), "desk1".into(), new_public.to_vec(), 1, 1).unwrap();
    assert!(open(&path, &packet(Stream::Event, 100, 1, new_secret, &notice("waiting", "r", "t", "b"))).is_ok(), "a new key starts fresh");
}

#[test]
fn hide_details_defaults_off_and_is_kept() {
    let path = store("hide");
    assert!(!store_hide_details(path.clone()).unwrap());
    store_set_hide_details(path.clone(), true).unwrap();
    assert!(store_hide_details(path.clone()).unwrap());
    store_set_computer(path.clone(), "desk1".into(), desktop_box_public().to_vec(), 1, 1).unwrap();
    assert!(store_hide_details(path).unwrap(), "changing a computer does not touch the switch");
}

#[test]
fn a_corrupt_file_is_reported_and_never_overwritten() {
    let path = store("corrupt");
    std::fs::create_dir_all(PathBuf::from(&path).parent().unwrap()).unwrap();
    std::fs::write(&path, b"{not json").unwrap();
    assert!(matches!(open(&path, &waiting(1)), Err(PushError::Store { .. })));
    assert!(matches!(store_set_hide_details(path.clone(), true), Err(PushError::Store { .. })));
    assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
}

#[test]
fn a_bad_computer_is_refused() {
    let path = store("bad");
    assert!(store_set_computer(path.clone(), "d".into(), vec![1; 31], 1, 1).is_err());
    assert!(store_set_computer(path, "d".into(), vec![1; 32], 2, 1).is_err());
}

/// Four processes, each taking its own notices and adding its own computers at the same time: no update is lost.
/// Each child is this test binary again (`worker_entry`), so the lock being tested is the real cross-process one.
#[test]
fn four_processes_updating_at_once_lose_nothing() {
    let path = paired("race");
    let exe = std::env::current_exe().unwrap();
    let children: Vec<_> = (0..4u64)
        .map(|k| {
            std::process::Command::new(&exe)
                .args(["--ignored", "--exact", "worker_entry", "--nocapture"])
                .env("BOBIX_PUSH_WORKER", format!("{path}|{k}"))
                .spawn()
                .unwrap()
        })
        .collect();
    for mut c in children {
        assert!(c.wait().unwrap().success(), "a worker failed");
    }
    // 4 workers x 60 notices (counters 1000+k, 1004+k, ...), all inside one window: every one was taken exactly once.
    for k in 0..4u64 {
        for i in 0..60u64 {
            let c = 1000 + k + 4 * i;
            assert!(matches!(open(&path, &waiting(c)), Err(PushError::Replayed)), "counter {c} should have been taken");
        }
    }
    // And their computers all made it into the file.
    let text = std::fs::read_to_string(&path).unwrap();
    for k in 0..4 {
        for i in 0..20 {
            assert!(text.contains(&format!("\"w{k}-{i}\"")), "computer w{k}-{i} was lost");
        }
    }
}

#[test]
#[ignore]
fn worker_entry() {
    let Ok(spec) = std::env::var("BOBIX_PUSH_WORKER") else { return };
    let (path, k) = spec.split_once('|').unwrap();
    let k: u64 = k.parse().unwrap();
    for i in 0..60u64 {
        open(path, &waiting(1000 + k + 4 * i)).unwrap();
        if i < 20 {
            store_set_computer(path.into(), format!("w{k}-{i}"), vec![k as u8 + 20; 32], 1, 1).unwrap();
        }
    }
}

#[test]
fn the_grant_is_the_bytes_the_relay_checks() {
    // Made by `relay/src/push.rs` (`grant_bytes`, `encode_grant`) from the same seeds, token, relay id and expiry; the
    // relay's `verify_grant` accepts it. See fixtures/push-fixtures.json for how it was produced.
    let g = make_grant(vec![3; 32], vec![4; 32], "ab12".repeat(16), "relay.example.test".into(), 1_800_000_000_000).unwrap();
    let fx: serde_json::Value = serde_json::from_str(include_str!("../fixtures/push-fixtures.json")).unwrap();
    assert_eq!(g, fx["grant"]["expected"].as_str().unwrap());
    assert!(make_grant(vec![3; 5], vec![4; 32], "t".into(), "r".into(), 1).is_err());
}

// ---- the fixtures the Swift and Dart tests read ---------------------------------------------------------------------

fn fixtures() -> String {
    use bobix_link::hex;
    let n = |what: &str, r: &str, t: &str, b: &str| notice(what, r, t, b);
    let ev = |c: u64, plain: &str| sealed(&packet(Stream::Event, c, 1, DESKTOP_BOX_SECRET, plain));
    let mut tampered = waiting(104);
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    let doc = serde_json::json!({
        "note": "GENERATED by mobile/push-core/tests/push_core.rs (BLESS=1 cargo test bless_fixtures -- --ignored). Packets are sealed from the desktop to the phone with the keys of crates/bobix-link/tests/vectors.json; counters are what the order of the Swift and Dart tests relies on. `grant.expected` was made by relay/src/push.rs (grant_bytes, encode_grant) and accepted by its verify_grant.",
        "phoneBoxSecretHex": hex(&PHONE_BOX_SECRET),
        "computer": { "id": "desk1", "boxPublicB64": sealed(&desktop_box_public()), "versionMin": 1, "versionMax": 1 },
        "packets": {
            "waiting": { "counter": 100, "sealed": ev(100, &n("waiting", "r1", "shop-app needs you", "Bash wants to run: curl -H [redacted]")) },
            "closed": { "counter": 101, "sealed": ev(101, &n("closed", "r1", "shop-app", "Answered at the computer")) },
            "done": { "counter": 102, "sealed": ev(102, &n("done", "r2", "blog is done", "Update the sign-up email")) },
            "waitingOtherRef": { "counter": 103, "sealed": ev(103, &n("waiting", "r3", "blog needs you", "Write wants to change a file")) },
            "tampered": { "counter": 104, "sealed": sealed(&tampered) },
            "stranger": { "counter": 105, "sealed": sealed(&packet(Stream::Event, 105, 1, STRANGER_BOX_SECRET, &n("waiting", "r1", "t", "b"))) },
            "home": { "counter": 7, "sealed": sealed(&packet(Stream::State, 7, 1, DESKTOP_BOX_SECRET, r#"{"kind":"home"}"#)) },
            "paired": { "counter": 8, "sealed": ev(8, r#"{"kind":"paired"}"#) },
            "version9": { "counter": 106, "sealed": sealed(&packet(Stream::Event, 106, 9, DESKTOP_BOX_SECRET, &n("waiting", "r1", "t", "b"))) },
            "high": { "counter": 5000, "sealed": ev(5000, &n("waiting", "r9", "shop-app needs you", "Bash wants to run: ls")) },
            "late": { "counter": 3, "sealed": ev(3, &n("waiting", "r9", "old", "old")) },
            "inWindow": { "counter": 4900, "sealed": ev(4900, &n("waiting", "r9", "shop-app needs you", "Bash wants to run: pwd")) }
        },
        "grant": {
            "authSeedHex": hex(&[3u8; 32]), "boxSecretHex": hex(&[4u8; 32]),
            "token": "ab12".repeat(16), "relayId": "relay.example.test", "expiresMs": 1_800_000_000_000u64,
            "expected": "7UkoxijRwsbq6QM4kFmVYSlZJzpcY_k2NsFGFKyHN9EAAAGjGFxQAL96B7Pg6uZH0ZVE30MjW6dGsO667VpistnsoTUbsV2PEq-0CiucZ2F776vEwsLXa_OpTQej6OBMz73g4sfXKQ8"
        }
    });
    serde_json::to_string_pretty(&doc).unwrap() + "\n"
}

#[test]
#[ignore]
fn bless_fixtures() {
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/push-fixtures.json"), fixtures()).unwrap();
}

#[test]
fn the_fixtures_are_current() {
    let have = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/push-fixtures.json")).unwrap_or_default();
    assert_eq!(have, fixtures(), "run: BLESS=1 cargo test --manifest-path mobile/push-core/Cargo.toml bless_fixtures -- --ignored");
}
