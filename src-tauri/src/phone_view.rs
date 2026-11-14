// What a phone gets: `home` (the sessions and what waits), `detail` (one held item whole) and the texts of notices.
// Nothing leaves the computer that is not built here. Every string in a preview or a notice passes the shared redactor
// (room.rs) and loses its hidden characters; the whole input is only in `detail`, for a phone with the matching right.
// Never in anything built here: the transcript path, the prompt text, git remotes, usage.
use crate::{answers::Pending, room, state::Sessions};
use bobix_link::{messages::*, pairing::Rights};
use std::collections::HashMap;

const SESSIONS_MAX: usize = 40;
const DESIGNS: [&str; 5] = ["default", "nota", "tty", "miso", "ship"]; // creature.js
const COLORS: u32 = 6; // BODY in creature.js

// creature.js hashStr: FNV-1a over UTF-16 code units (an astral character counts by its high half only, as there).
fn hash_str(s: &str) -> u32 {
    s.chars().fold(2166136261u32, |h, c| {
        let u = if (c as u32) > 0xFFFF { 0xD800 + ((c as u32 - 0x10000) >> 10) } else { c as u32 };
        (h ^ u).wrapping_mul(16777619)
    })
}
pub fn pet_of(project: &str, id: &str) -> Pet {
    Pet { design: DESIGNS[hash_str(if project.is_empty() { id } else { project }) as usize % DESIGNS.len()].into(), color: (hash_str(&format!("{id}c")) % COLORS) as u8 }
}

// One line of at most `max` bytes: secrets replaced, hidden characters dropped, whitespace folded.
pub fn preview(text: &str, max: usize) -> String {
    let (red, _) = room::redact(text);
    let line: String = red.chars().filter(|c| !is_odd_char(*c) || c.is_whitespace()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    cut_bytes(&line, max)
}
pub fn cut_bytes(s: &str, max: usize) -> String {
    if s.len() <= max { return s.into(); }
    let mut n = max;
    while !s.is_char_boundary(n) { n -= 1; }
    s[..n].into()
}

fn folder(p: &str) -> String { p.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").into() }

// The sessions and what waits. `groups` stay empty here (the Home groups come with the remote actions).
pub fn home(sessions: &Sessions, pending: &HashMap<String, Pending>, computer: &str, now: u64) -> Home {
    let mut list: Vec<&crate::state::Session> = sessions.values().collect();
    list.sort_by(|a, b| b.updated.cmp(&a.updated));
    let unfinished = sessions.values().filter(|s| matches!(s.state.as_str(), "working" | "waiting")).count() as u32;
    let infos = list.iter().take(SESSIONS_MAX).map(|s| {
        let project = s.project.as_deref().map(folder).unwrap_or_else(|| folder(&s.cwd));
        SessionInfo { id: s.id.clone(), title: preview(if s.title.is_empty() { &s.name } else { &s.title }, 80), project: preview(&project, 60), state: s.state.clone(), pet: pet_of(&project, &s.id) }
    }).collect();
    let mut waiting: Vec<Waiting> = pending.iter().filter(|(_, p)| sessions.contains_key(&p.sid)).map(|(id, p)| Waiting {
        id: id.clone(), session: p.sid.clone(), what: if p.plan { WaitingKind::Plan } else { WaitingKind::Request },
        preview: preview(p.raw.lines().next().unwrap_or(""), MAX_PREVIEW), cut: p.cut, risky: p.risky || p.cut, at_computer: p.released,
        seconds_left: p.until.map(|u| u.saturating_sub(now) / 1000),
    }).collect();
    waiting.sort_by(|a, b| a.id.cmp(&b.id));
    fit(Home { sent_at: now, computer: Computer { name: preview(computer, 64), state: ComputerState::Online, unfinished }, sessions: infos, groups: vec![], waiting })
}

// A sealed `home` must fit a frame: previews go first, then the oldest sessions.
pub fn fit(mut h: Home) -> Home {
    const LIMIT: usize = MAX_FRAME / 4 * 3 - 2048; // base64 of the sealed packet, plus the frame around it
    let size = |h: &Home| DesktopMsg::Home(h.clone()).to_json().len();
    if size(&h) > LIMIT { for w in &mut h.waiting { w.preview.clear(); } }
    while size(&h) > LIMIT && h.sessions.len() > 1 { h.sessions.pop(); }
    h
}

// One held item, whole, with the hash of what is held. Err: "unknown" (not held) or "right" (the phone lacks the right).
pub fn detail(pending: &HashMap<String, Pending>, id: &str, rights: Rights) -> Result<Detail, &'static str> {
    let p = pending.get(id).ok_or("unknown")?;
    if !(if p.plan { rights.plan } else { rights.answer }) { return Err("right"); }
    let (what, limit) = if p.plan { (DetailKind::Plan, MAX_PLAN) } else { (DetailKind::ToolInput, MAX_TOOL_INPUT) };
    let (mut raw, mut cut) = (p.raw.clone(), p.cut);
    loop {
        let d = Detail::build(id, what, &raw, cut, vec![]);
        if d.text.len() <= limit { return Ok(d); }
        raw = cut_bytes(&raw, raw.len() * 3 / 4); // escapes made it longer than the limit: shorter, and cut says so
        cut = true;
    }
}

// The notice of a held request: the session and what it asks, redacted and without hidden characters.
pub fn waiting_notice(p: &Pending, id: &str, title: &str) -> Notice {
    Notice { what: NoticeKind::Waiting, reference: id.into(), session: p.sid.clone(), title: preview(title, 80), body: preview(&p.raw, 200) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_redacted_short_and_plain() {
        let tok = format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0k1L2m3N4");
        let p = preview(&format!("curl -H 'x: {tok}' https://x.y\u{202e}/a  b\n c"), MAX_PREVIEW);
        assert!(!p.contains(&tok) && !p.contains('\u{202e}') && !p.contains('\n'), "{p}");
        for t in ["lin_api_abcdefghijklmnopqrstuvwx", "glpat-abcdefghijklmnopqrstu", "ATATT3xFfGF0abcdefghijklmnopqrstuvwx", "sk_live_abcdefghijklmnop", "npm_abcdefghijklmnopqrstuvwxyz0123456789"] {
            assert!(!preview(&format!("export T={t}"), 300).contains(t), "{t}");
        }
        assert!(preview(&"é".repeat(500), MAX_PREVIEW).len() <= MAX_PREVIEW, "cut on a character boundary");
    }

    #[test]
    fn a_pet_is_the_pages_pet() {
        // creature.js: hashStr('x') = 2166136261 ^ 120 * 16777619; the design is that mod 5, the colour that of 'x'+'c' mod 6
        assert_eq!(hash_str("x"), (2166136261u32 ^ 120).wrapping_mul(16777619));
        assert!(pet_of("", "s1").color < 6 && DESIGNS.contains(&pet_of("app", "s1").design.as_str()));
    }

    fn held(raw: &str, plan: bool, cut: bool) -> HashMap<String, Pending> {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut m = HashMap::new();
        m.insert("r1".into(), Pending::for_test("s1", plan, tx, raw, cut));
        m
    }

    #[test]
    fn detail_is_whole_for_a_phone_with_the_right_and_marks_hidden_characters() {
        let all = Rights { answer: true, plan: true, ..Default::default() };
        let d = detail(&held("ls \u{202e}gpj.sh\nrm x", false, false), "r1", all).unwrap();
        assert!(d.odd && d.text.contains("\\u{202e}") && !d.text.contains('\u{202e}') && d.text.contains("rm x") && !d.cut);
        assert_eq!(d.shown, d.shown_now(), "the hash is of what is held");
        assert_eq!(detail(&held("ls", false, false), "r1", Rights { plan: true, ..Default::default() }), Err("right"), "no answer right");
        assert_eq!(detail(&held("plan", true, false), "r1", Rights { answer: true, ..Default::default() }), Err("right"), "no plan right");
        assert_eq!(detail(&held("ls", false, false), "nope", all), Err("unknown"));
        let big = detail(&held(&"\u{200b}".repeat(10_000), false, false), "r1", all).unwrap();
        assert!(big.cut && big.text.len() <= MAX_TOOL_INPUT, "escapes that grow past the limit are cut, and marked");
    }

    #[test]
    fn home_carries_no_forbidden_field() {
        let mut sessions = Sessions::new();
        let mut s = crate::state::Session::default();
        s.id = "s1".into(); s.cwd = "/Users/someone/code/app".into(); s.title = "Fix it".into(); s.state = "waiting".into();
        s.transcript = "/Users/someone/.claude/projects/x/s1.jsonl".into(); s.prompt = "SECRET PROMPT TEXT".into();
        s.usage = Some(serde_json::json!({ "cost": 1 }));
        s.git = Some(serde_json::json!({ "remotes": ["git@host:team/repo.git"] }));
        sessions.insert("s1".into(), s);
        let p = held("rm -rf build && echo ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4", false, false);
        let h = home(&sessions, &p, "Studio", 1000);
        let json = String::from_utf8(DesktopMsg::Home(h.clone()).to_json()).unwrap();
        for never in ["/Users/someone", "SECRET PROMPT", "remotes", "git@host", "usage", ".jsonl", "ghp_a1B2"] { assert!(!json.contains(never), "{never} in {json}"); }
        assert_eq!(h.sessions[0].project, "app");
        assert_eq!(h.waiting[0].what, WaitingKind::Request);
    }

    #[test]
    fn a_home_that_is_too_big_loses_previews_then_sessions() {
        let mut sessions = Sessions::new();
        for i in 0..40 { let mut s = crate::state::Session::default(); s.id = format!("s{i}"); s.title = "t".repeat(80); sessions.insert(s.id.clone(), s); }
        let mut p = HashMap::new();
        for i in 0..1200 { let (tx, _rx) = tokio::sync::oneshot::channel(); p.insert(format!("{i:032}"), Pending::for_test("s1", false, tx, &"x".repeat(300), false)); }
        sessions.get_mut("s1").unwrap().state = "waiting".into();
        let h = home(&sessions, &p, "Studio", 1);
        assert!(DesktopMsg::Home(h.clone()).to_json().len() <= MAX_FRAME / 4 * 3 - 2048 && h.waiting.iter().all(|w| w.preview.is_empty()));
    }
}
