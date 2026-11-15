// What a phone gets: `home` (the sessions and what waits), `detail` (one held item whole) and the texts of notices.
// Nothing leaves the computer that is not built here. Every string in a preview or a notice passes the shared redactor
// (room.rs) and loses its hidden characters; the whole input is only in `detail`, for a phone with the matching right.
// Never in anything built here: the transcript path, the prompt text, git remotes, usage.
use crate::{answers::{Kind, Pending}, room, state::Sessions};
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
    // hidden characters go first: one inside a token must not hide it from the redactor, and then join it back together
    let clean: String = text.chars().filter(|c| !is_odd_char(*c) || c.is_whitespace()).collect();
    let (red, _) = room::redact(&clean);
    let line: String = red.split_whitespace().collect::<Vec<_>>().join(" ");
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
pub fn home(sessions: &Sessions, pending: &HashMap<String, Pending>, computer: &str, now: u64, state: ComputerState, groups: Vec<Group>) -> Home {
    let mut list: Vec<&crate::state::Session> = sessions.values().collect();
    list.sort_by(|a, b| b.updated.cmp(&a.updated));
    let unfinished = sessions.values().filter(|s| matches!(s.state.as_str(), "working" | "waiting")).count() as u32;
    let infos = list.iter().take(SESSIONS_MAX).map(|s| {
        let project = s.project.as_deref().map(folder).unwrap_or_else(|| folder(&s.cwd));
        // waiting with nothing held: it is answered at the computer
        let held = s.state == "waiting" && pending.values().any(|p| p.sid == s.id);
        let held_request = pending.values().any(|p| p.sid == s.id && p.kind() == Kind::Request); // Stop is only for a plain request
        let state = if s.state == "waiting" && !held { "atComputer".to_string() } else { s.state.clone() };
        SessionInfo { id: s.id.clone(), title: preview(if s.title.is_empty() { &s.name } else { &s.title }, 80), project: preview(&project, 60), state, pet: pet_of(&project, &s.id), can_stop: held_request }
    }).collect();
    let mut waiting: Vec<Waiting> = pending.iter().filter(|(_, p)| sessions.contains_key(&p.sid)).map(|(id, p)| Waiting {
        id: id.clone(), session: p.sid.clone(), what: match p.kind() { Kind::Plan => WaitingKind::Plan, Kind::Question => WaitingKind::Question, Kind::Request => WaitingKind::Request },
        preview: preview(p.raw.lines().next().unwrap_or(""), MAX_PREVIEW), cut: shown_cut(p), risky: p.risky || shown_cut(p), at_computer: p.released,
        seconds_left: p.until.map(|u| u.saturating_sub(now) / 1000),
    }).collect();
    waiting.sort_by(|a, b| a.id.cmp(&b.id));
    fit(Home { sent_at: now, computer: Computer { name: preview(computer, 64), state, unfinished }, sessions: infos, groups, waiting, rights: None })
}

// The Home groups a phone can carry: its entries are session or waiting ids, so an issue with no session (To do, Done) has
// no entry yet (the crate's `home` has no issue list). Needs you: what waits; In progress and Waiting for review: the sessions
// on those issues; Stuck: the session of each stuck item. Empty groups are left out.
pub fn groups(work: &crate::work::Work, settings: &serde_json::Value, sessions: &Sessions, pending: &HashMap<String, Pending>, now: u64) -> Vec<Group> {
    let model = crate::model::Model::build(settings, sessions, work);
    let view = work.view(settings, &model);
    let list: Vec<serde_json::Value> = sessions.values().map(|s| serde_json::json!({ "id": s.id, "state": s.state, "st": s.state, "since": s.since, "updated": s.updated, "git": s.git })).collect();
    let st = crate::model::stages(&view, &list, now as f64, &crate::model::local_day);
    let ids_of = |k: &str| -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for x in st[k].as_array().into_iter().flatten() { for id in x[2].as_array().into_iter().flatten().filter_map(|i| i.as_str()) { if !out.iter().any(|o| o == id) { out.push(id.into()); } } }
        out
    };
    let mut needs: Vec<String> = pending.iter().filter(|(_, p)| sessions.contains_key(&p.sid)).map(|(id, _)| id.clone()).collect();
    needs.sort();
    let stuck: Vec<String> = crate::model::stuck(&model.json(), &list, &settings.clone(), now as f64).iter().filter_map(|x| x["session"].as_str().map(String::from)).fold(vec![], |mut a, s| { if !a.contains(&s) { a.push(s); } a });
    [(GroupKind::NeedsYou, needs), (GroupKind::InProgress, ids_of("doing")), (GroupKind::WaitingForReview, ids_of("verify")), (GroupKind::Stuck, stuck)]
        .into_iter().filter(|(_, ids)| !ids.is_empty()).map(|(group, ids)| Group { group, ids }).collect()
}

// The home as one phone gets it: it carries that phone's rights, so a change made on the computer reaches it, and Stop is
// offered only where the phone holds both the Stop and the Answer right (the desktop refuses it otherwise).
pub fn for_phone(mut h: Home, rights: Rights) -> Home {
    h.rights = Some(rights);
    if !(rights.stop && rights.answer) { for s in &mut h.sessions { s.can_stop = false; } }
    h
}

// A sealed `home` must fit a frame: previews go first, then the oldest sessions.
pub fn fit(mut h: Home) -> Home {
    const LIMIT: usize = MAX_FRAME / 4 * 3 - 2048; // base64 of the sealed packet, plus the frame around it
    let size = |h: &Home| DesktopMsg::Home(h.clone()).to_json().len();
    if size(&h) > LIMIT { for w in &mut h.waiting { w.preview.clear(); } }
    while size(&h) > LIMIT && h.sessions.len() > 1 { h.sessions.pop(); }
    h
}

// Whether what a phone is shown is cut: the hook dropped part of the input, or escaping hidden characters made the text
// longer than the cap and `detail` shortened it. The one place that says so; a remote Allow or approve is refused for it.
pub fn shown_cut(p: &Pending) -> bool { built("", p).cut }

// One held item, whole, with the hash of what is held. Err: "unknown" (not held) or "right" (the phone lacks the right).
pub fn detail(pending: &HashMap<String, Pending>, id: &str, rights: Rights) -> Result<Detail, &'static str> {
    let p = pending.get(id).ok_or("unknown")?;
    if !(match p.kind() { Kind::Plan => rights.plan, Kind::Question => rights.question, Kind::Request => rights.answer }) { return Err("right"); }
    Ok(built(id, p))
}

// The text a phone is shown for a held item. The desktop's escaper runs first (it knows more hidden characters than the
// crate's), so the phone shows what the panel shows; text that escaping makes longer than the cap is shortened, and cut says so.
fn built(id: &str, p: &Pending) -> Detail {
    let (what, limit) = match p.kind() { Kind::Plan => (DetailKind::Plan, MAX_PLAN), Kind::Question => (DetailKind::Question, MAX_TOOL_INPUT), Kind::Request => (DetailKind::ToolInput, MAX_TOOL_INPUT) };
    // the questions as shown: hidden characters escaped in each text and option
    let questions: Vec<Question> = p.questions.iter().map(|q| Question { text: crate::answers::escape_hidden(&q.text), options: q.options.iter().map(|o| crate::answers::escape_hidden(o)).collect(), multi_select: q.multi_select }).collect();
    let (mut raw, mut cut) = (p.raw.clone(), p.cut);
    loop {
        let mut d = Detail::build(id, what, &crate::answers::escape_hidden(&raw), cut, questions.clone());
        d.odd |= crate::answers::odd(&serde_json::Value::String(raw.clone())) || p.questions.iter().any(|q| crate::answers::odd(&serde_json::Value::String(q.text.clone())) || q.options.iter().any(|o| crate::answers::odd(&serde_json::Value::String(o.clone()))));
        if d.text.len() <= limit { return d; }
        raw = cut_bytes(&raw, raw.len() * 3 / 4);
        cut = true;
    }
}

// A session's last lines (at most 40, the prompt lines left out), redacted, for a phone with "See session activity".
pub fn session_detail(sessions: &Sessions, id: &str, rights: Rights) -> Result<Detail, &'static str> {
    let s = sessions.get(id).ok_or("unknown")?;
    if !rights.activity { return Err("right"); }
    let mut text = s.log.iter().filter(|l| l.kind != "prompt" && l.kind != "system").map(|l| {
        let line = if l.tool.is_empty() { l.text.clone() } else { format!("{} {}", l.tool, l.text) };
        room::redact(&line.chars().filter(|c| !is_odd_char(*c) || *c == '\n' || *c == '\t').collect::<String>()).0.replace('\n', " ")
    }).collect::<Vec<_>>().join("\n");
    let cut = text.len() > MAX_TOOL_INPUT;
    if cut { text = cut_bytes(&text, MAX_TOOL_INPUT / 2); } // escapes of hidden characters may grow it; half is safe for what remains
    let mut d = Detail::build(id, DetailKind::LastLines, &text, cut, vec![]);
    while d.text.len() > MAX_TOOL_INPUT { text = cut_bytes(&text, text.len() * 3 / 4); d = Detail::build(id, DetailKind::LastLines, &text, true, vec![]); }
    Ok(d)
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
        let split = format!("ghp_{}\u{200b}{}", "a1B2c3D4e5", "F6g7H8i9J0k1L2m3N4");
        assert!(!preview(&split, 300).contains("F6g7H8") && !preview(&format!("x \u{feff}{split}"), 300).contains("a1B2c3"), "a token split by a hidden character is still redacted");
        assert!(!p.contains(&tok) && !p.contains('\u{202e}') && !p.contains('\n'), "{p}");
        for line in [
            "export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMIK7MDENGbPxRfiCY", "PGPASSWORD=hunter2hunter psql -h db", "GITHUB_TOKEN=abcd1234efgh", "DB_PASSWORD: s3cr3tvalue",
            "MYSQL_PWD=hunter2hunter mysql", "curl --password hunter2hunter x", "mysql -u root -phunter2hunter db", "docker login --password=hunter2hunter",
        ] {
            let p = preview(line, 300);
            assert!(!p.contains("hunter2") && !p.contains("wJalr") && !p.contains("abcd1234") && !p.contains("s3cr3t"), "{line} -> {p}");
        }
        assert_eq!(preview("password=hunter2hunter", 300), "password=[redacted]");
        assert!(preview("the token_count was 5 and the secretary left", 300).contains("secretary left"), "plain words stay");
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
        assert!(d.odd && d.text.contains("\\u202E") && !d.text.contains('\u{202e}') && d.text.contains("rm x") && !d.cut);
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
        let h = home(&sessions, &p, "Studio", 1000, ComputerState::Online, vec![]);
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
        let h = home(&sessions, &p, "Studio", 1, ComputerState::Online, vec![]);
        assert!(DesktopMsg::Home(h.clone()).to_json().len() <= MAX_FRAME / 4 * 3 - 2048 && h.waiting.iter().all(|w| w.preview.is_empty()));
    }

    #[test]
    fn last_lines_have_no_prompts_and_no_secrets_and_need_the_right() {
        let mut sessions = Sessions::new();
        let mut s = crate::state::Session::default();
        s.id = "s1".into();
        let l = |kind: &str, text: &str, tool: &str| crate::state::LogLine { t: 1, kind: kind.into(), text: text.into(), tool: tool.into() };
        s.log = vec![l("prompt", "SECRET PROMPT", ""), l("tool", "npm test ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4", "Bash"), l("tool", "src/a.rs", "Edit")];
        sessions.insert("s1".into(), s);
        let rights = Rights { activity: true, ..Default::default() };
        let d = session_detail(&sessions, "s1", rights).unwrap();
        assert_eq!(d.what, DetailKind::LastLines);
        assert!(d.text.contains("Bash npm test") && d.text.contains("Edit src/a.rs") && !d.text.contains("SECRET PROMPT") && !d.text.contains("ghp_a1B2"), "{}", d.text);
        assert_eq!(session_detail(&sessions, "s1", Rights::default()), Err("right"));
        assert_eq!(session_detail(&sessions, "nope", rights), Err("unknown"));
        sessions.get_mut("s1").unwrap().log.clear();
        assert_eq!(session_detail(&sessions, "s1", rights).unwrap().text, "", "no lines: an empty answer");
    }

    #[test]
    fn a_sleeping_computer_says_asleep_with_what_is_unfinished() {
        let mut sessions = Sessions::new();
        for (id, st) in [("a", "working"), ("b", "waiting"), ("c", "idle")] { let mut s = crate::state::Session::default(); s.id = id.into(); s.state = st.into(); sessions.insert(id.into(), s); }
        let h = home(&sessions, &HashMap::new(), "Studio", 1, ComputerState::Asleep, vec![]);
        assert_eq!((h.computer.state, h.computer.unfinished), (ComputerState::Asleep, 2));
    }

    #[test]
    fn stop_is_offered_only_for_a_plain_request_and_a_phone_with_both_rights() {
        let mut sessions = Sessions::new();
        for id in ["r", "p", "q", "n"] { let mut s = crate::state::Session::default(); s.id = id.into(); s.state = "waiting".into(); sessions.insert(id.into(), s); }
        let mut pending = HashMap::new();
        for (id, sid, plan, q) in [("1", "r", false, false), ("2", "p", true, false), ("3", "q", false, true)] {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            let mut p = Pending::for_test(sid, plan, tx, "x", false);
            if q { p.input = Some(serde_json::json!({})); }
            pending.insert(id.to_string(), p);
        }
        let h = home(&sessions, &pending, "Studio", 1, ComputerState::Online, vec![]);
        let stop = |h: &Home, id: &str| h.sessions.iter().find(|s| s.id == id).unwrap().can_stop;
        assert!(stop(&h, "r") && !stop(&h, "p") && !stop(&h, "q") && !stop(&h, "n"), "a plain request only");
        let both = Rights { stop: true, answer: true, ..Default::default() };
        assert!(stop(&for_phone(h.clone(), both), "r"));
        assert_eq!(h.rights, None, "the shared home has no phone's rights");
        assert_eq!(for_phone(h.clone(), both).rights, Some(both), "a phone's home carries its own rights");
        for r in [Rights { stop: true, ..Default::default() }, Rights { answer: true, ..Default::default() }, Rights::default()] {
            assert!(!stop(&for_phone(h.clone(), r), "r"), "{r:?}: the desktop would refuse it");
        }
    }
}
