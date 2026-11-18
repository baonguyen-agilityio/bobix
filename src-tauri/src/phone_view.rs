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
// What the panel's pet needs of a session (creature.js petColor reads project, id and parent only).
pub struct PetIn { pub id: String, pub project: String, pub parent: Option<String> }

// The pet of a session as the panel draws it: `list` is every shown session in the order they came (the sessions map's).
// Design: creature.js designOf over the full project (or the id), a side session by its parent's. Colour: petColor.
pub fn pet_of(s: &PetIn, list: &[PetIn]) -> Pet {
    let proj = |x: &PetIn| if x.project.is_empty() { x.id.clone() } else { x.project.clone() };
    let art = s.parent.as_ref().and_then(|p| list.iter().find(|x| &x.id == p)).unwrap_or(s);
    Pet { design: DESIGNS[hash_str(&proj(art)) as usize % DESIGNS.len()].into(), color: pet_color(s, list, 0) as u8 }
}
fn pet_color(s: &PetIn, list: &[PetIn], depth: u8) -> u32 {
    let proj = |x: &PetIn| if x.project.is_empty() { x.id.clone() } else { x.project.clone() };
    let top = |x: &PetIn| !x.parent.as_ref().is_some_and(|p| list.iter().any(|y| &y.id == p));
    let par = s.parent.as_ref().and_then(|p| list.iter().find(|x| &x.id == p));
    if let (Some(par), true) = (par, depth < 4) { // side sessions avoid the colours of their project's own sessions
        let pc = pet_color(par, list, depth + 1);
        let taken: Vec<u32> = list.iter().filter(|x| top(x) && proj(x) == proj(par)).map(|x| pet_color(x, list, depth + 1)).collect();
        let free: Vec<u32> = (0..COLORS).map(|i| (pc + 1 + i) % COLORS).filter(|i| !taken.contains(i)).collect();
        let k = list.iter().filter(|x| x.parent == s.parent).position(|x| x.id == s.id).unwrap_or(0) as u32;
        return if free.is_empty() { (pc + 1 + k % (COLORS - 1)) % COLORS } else { free[k as usize % free.len()] };
    }
    let mates = list.iter().filter(|x| top(x) && proj(x) == proj(s)).position(|x| x.id == s.id).unwrap_or(0) as u32;
    (hash_str(&format!("{}c", proj(s))) % COLORS + mates) % COLORS
}

// One line of at most `max` bytes: secrets replaced, hidden characters dropped, whitespace folded.
pub fn preview(text: &str, max: usize) -> String {
    // hidden characters go first: one inside a token must not hide it from the redactor, and then join it back together
    let clean: String = text.chars().zip(odd_flags(text)).filter(|(c, odd)| !odd || c.is_whitespace()).map(|(c, _)| c).collect();
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
    // the pets' list as the panel has it: every session that is not still waiting for its parent link, in arrival order, with
    // the parent as the model finds it (by the Claude app's session id; the later session of that id)
    let by_host: HashMap<&str, &str> = sessions.values().filter_map(|s| Some((s.host_session.as_deref()?, s.id.as_str()))).collect();
    let pet_in = |s: &crate::state::Session| PetIn { id: s.id.clone(), project: s.project.clone().unwrap_or_default(), parent: s.parent_host.as_deref().and_then(|h| by_host.get(h)).map(|p| p.to_string()) };
    let pets: Vec<PetIn> = sessions.values().filter(|s| !crate::state::linking(s, now)).map(pet_in).collect();
    let infos = list.iter().take(SESSIONS_MAX).map(|s| {
        let project = s.project.as_deref().map(folder).unwrap_or_else(|| folder(&s.cwd));
        // waiting with nothing held: it is answered at the computer
        let held = s.state == "waiting" && pending.values().any(|p| p.sid == s.id);
        let held_request = pending.values().any(|p| p.sid == s.id && p.kind() == Kind::Request); // Stop is only for a plain request
        let state = if s.state == "waiting" && !held { "atComputer".to_string() } else { s.state.clone() };
        let at_computer_why = if state == "atComputer" { s.at_computer_why } else { None };
        SessionInfo { id: s.id.clone(), title: preview(if s.title.is_empty() { &s.name } else { &s.title }, 80), project: preview(&project, 60), state, pet: pet_of(&pet_in(s), &pets), can_stop: held_request, lines: activity_lines(s).len().min(40) as u8, at_computer_why }
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

// How many sessions wait for a phone: those with a request, plan or question held for the phones and not yet released (the
// first input at the computer releases it). This is the phone's own count (`needsYou` with an item held for it, not handled at
// the computer), so the Live Activity pushed from here and the one the app keeps agree.
pub fn waiting_for_phones(sessions: &Sessions, pending: &HashMap<String, Pending>) -> u32 {
    let held: std::collections::HashSet<&str> = pending.values().filter(|p| !p.released && sessions.contains_key(&p.sid)).map(|p| p.sid.as_str()).collect();
    held.len() as u32
}

// The home as one phone gets it: it carries that phone's rights, so a change made on the computer reaches it, and Stop is
// offered only where the phone holds both the Stop and the Answer right (the desktop refuses it otherwise).
pub fn for_phone(mut h: Home, rights: Rights) -> Home {
    h.rights = Some(rights);
    if !(rights.stop && rights.answer) { for s in &mut h.sessions { s.can_stop = false; } }
    if !rights.activity { for s in &mut h.sessions { s.lines = 0; } }
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

// Whether a remote Approve (or Allow) is refused for what the phone is shown: the text is cut (`shown_cut`), a file that
// comes with the plan is shortened or only listed, or the plan names files that are not shown at all. Sending a plan back is
// never refused for it.
pub fn blocks_allow(p: &Pending) -> bool { shown_cut(p) || p.files_omitted > 0 || p.files.iter().any(|f| f.cut || !f.sent) }

// One held item, whole, with the hash of what is held (a plan with its files: they go to no other message, and only to a phone
// with the plan right, which is checked here). Err: "unknown" (not held) or "right" (the phone lacks the right).
pub fn detail(pending: &HashMap<String, Pending>, id: &str, rights: Rights) -> Result<Detail, &'static str> {
    let p = pending.get(id).ok_or("unknown")?;
    if !p.kind().answerable_with(rights) { return Err("right"); }
    let d = built(id, p);
    Ok(if p.kind() == Kind::Plan { d.with_files(p.files.clone()).with_omitted(p.files_omitted) } else { d })
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

// The lines of a session a phone may read, since the person's last prompt, each with its time (Unix ms): what it did, then
// what Claude said (its last reply, kept apart from the log the panel shows; it has no time of its own, so the session's
// last update stands for it), never what the person typed.
fn activity_lines(s: &crate::state::Session) -> Vec<(u64, String)> {
    let from = s.log.iter().rposition(|l| l.kind == "prompt").map_or(0, |i| i + 1);
    let mut lines: Vec<(u64, String)> = s.log[from..].iter().filter(|l| l.kind != "system" && !(l.text.is_empty() && l.tool.is_empty()))
        .map(|l| (l.t, if l.tool.is_empty() { l.text.clone() } else { format!("{} {}", l.tool, l.text) })).collect();
    if !s.summary.is_empty() { lines.push((s.updated, s.summary.clone())); }
    lines
}

// A session's last lines (at most 40, the prompt lines left out), redacted, for a phone with "See session activity". `times`
// has one entry per line of the text, kept equal to it by every cut.
pub fn session_detail(sessions: &Sessions, id: &str, rights: Rights) -> Result<Detail, &'static str> {
    let s = sessions.get(id).ok_or("unknown")?;
    if !rights.activity { return Err("right"); }
    let all = activity_lines(s);
    let last = &all[all.len().saturating_sub(40)..];
    let mut text = last.iter().map(|(_, line)| {
        room::redact(&line.chars().zip(odd_flags(&line)).filter(|(c, odd)| !odd || *c == '\n' || *c == '\t').map(|(c, _)| c).collect::<String>()).0.replace('\n', " ")
    }).collect::<Vec<_>>().join("\n");
    let mut times: Vec<u64> = last.iter().map(|(t, _)| *t).collect();
    let cut = text.len() > MAX_TOOL_INPUT;
    if cut { text = cut_bytes(&text, MAX_TOOL_INPUT / 2); } // escapes of hidden characters may grow it; half is safe for what remains
    times.truncate(text.split('\n').count());
    let mut d = Detail::build(id, DetailKind::LastLines, &text, cut, vec![]);
    while d.text.len() > MAX_TOOL_INPUT { text = cut_bytes(&text, text.len() * 3 / 4); times.truncate(text.split('\n').count()); d = Detail::build(id, DetailKind::LastLines, &text, true, vec![]); }
    d.times = times;
    Ok(d)
}

// The notice of a held request: the session and what it asks, redacted and without hidden characters.
pub fn waiting_notice(p: &Pending, id: &str, title: &str) -> Notice {
    Notice { what: NoticeKind::Waiting, reference: id.into(), session: p.sid.clone(), title: preview(title, 80), body: preview(&p.raw, 200), by: None }
}

// The reply to `issues`: the repositories ticked in Settings → Work (on the hosts the work view read), each with the open
// issues that view holds. Nothing is read from a tracker for it: what the Work list shows is what a phone gets, and the
// error says when the last read failed (`read`) or the first has not finished (`reading`). A phone without the Start right
// is refused. Titles are redacted and cut, a repository whose path or host is not plain text is left out (it could not be
// started either), and the repositories and issues are capped to what fits a frame.
// `view` builds the work view and is not called for a phone without the right.
pub fn issues_reply(view: impl FnOnce() -> serde_json::Value, settings: &serde_json::Value, rights: Rights) -> DesktopMsg {
    if !rights.start { return DesktopMsg::Error { code: "right".into(), who: Some(Who::Desktop) }; }
    DesktopMsg::Issues(issue_list(&view(), settings))
}

fn issue_list(view: &serde_json::Value, settings: &serde_json::Value) -> IssueList {
    let strs = |v: &serde_json::Value| -> Vec<String> { v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect() };
    let plain = |t: &str, max: usize| !t.is_empty() && t.len() <= max && preview(t, max) == t;
    let host_of = |url: &str| tauri::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_lowercase));
    let mut repos: Vec<IssueRepo> = vec![];
    for host in strs(&view["hosts"]) {
        for path in strs(&settings["workRepos"][host.as_str()]) {
            if !plain(&path, MAX_ISSUE_PATH) || !plain(&host, MAX_ISSUE_HOST) || repos.iter().any(|r| r.path == path && r.host == host) { continue; }
            let here = view["local"].as_array().into_iter().flatten().any(|g| g["path"] == path.as_str());
            repos.push(IssueRepo { cloned: here || view["dirs"].get(&path).is_some_and(|d| d.is_string()), path, host: host.clone(), issues: vec![] });
        }
    }
    repos.sort_by(|a, b| (&a.path, &a.host).cmp(&(&b.path, &b.host)));
    repos.truncate(MAX_ISSUE_REPOS);
    let mut total = 0;
    for group in ["local", "away"].iter().flat_map(|k| view[*k].as_array().into_iter().flatten()) {
        let path = group["path"].as_str().unwrap_or("");
        for i in group["issues"].as_array().into_iter().flatten() {
            let (Some(iid), Some(title)) = (i["iid"].as_u64(), i["title"].as_str()) else { continue };
            let url_host = i["url"].as_str().and_then(host_of);
            // the repository the issue's own link names; one path ticked on two hosts is told apart by it
            let Some(repo) = repos.iter_mut().find(|r| r.path == path && url_host.as_deref().is_none_or(|h| r.host.split(':').next() == Some(h))) else { continue };
            if total >= MAX_ISSUES { break; }
            total += 1;
            repo.issues.push(IssueInfo {
                iid, title: preview(title, MAX_ISSUE_TITLE), updated: i["updated"].as_str().filter(|u| !u.is_empty()).map(|u| cut_bytes(u, 40)),
                has_session: i["sessions"].as_array().is_some_and(|s| !s.is_empty()),
            });
        }
    }
    let error = match view["status"].as_str() { Some("error") => Some(IssuesError::Read), Some("loading") => Some(IssuesError::Reading), _ => None };
    IssueList { repos, error }
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

    // test/pet-cases.json is read by core.test.js too, against creature.js: the same inputs give the same design and colour.
    #[test]
    fn a_pet_is_the_same_as_creature_js_gives() {
        let cases: serde_json::Value = serde_json::from_str(include_str!("../../test/pet-cases.json")).unwrap();
        for c in cases.as_array().unwrap() {
            let list: Vec<PetIn> = c["list"].as_array().unwrap().iter().map(|x| PetIn { id: x["id"].as_str().unwrap().into(), project: x["project"].as_str().unwrap().into(), parent: x["parent"].as_str().map(String::from) }).collect();
            let got: Vec<serde_json::Value> = list.iter().map(|s| { let p = pet_of(s, &list); serde_json::json!([p.design, p.color]) }).collect();
            assert_eq!(serde_json::Value::Array(got), c["pets"], "{}", c["name"]);
        }
    }

    fn held(raw: &str, plan: bool, cut: bool) -> HashMap<String, Pending> {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut m = HashMap::new();
        m.insert("r1".into(), Pending::for_test("s1", plan, tx, raw, cut));
        m
    }

    #[test]
    fn the_count_for_phones_is_the_sessions_with_something_held_and_not_released() {
        let mut sessions = Sessions::default();
        for id in ["s1", "s2", "s3"] { let mut s = crate::state::Session::default(); s.id = id.into(); sessions.insert(id.into(), s); }
        let mut pending = HashMap::new();
        for (id, sid) in [("a", "s1"), ("b", "s1"), ("c", "s2"), ("d", "s3"), ("e", "gone")] {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            pending.insert(id.to_string(), Pending::for_test(sid, false, tx, "ls", false));
        }
        assert_eq!(waiting_for_phones(&sessions, &pending), 3, "two items of one session count once; a session that is gone does not count");
        pending.get_mut("c").unwrap().released = true;
        assert_eq!(waiting_for_phones(&sessions, &pending), 2, "a released item is handled at the computer");
        pending.get_mut("a").unwrap().released = true;
        assert_eq!(waiting_for_phones(&sessions, &pending), 2, "a session still has one that is not released");
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
    fn a_plans_files_reach_only_a_phone_with_the_plan_right_and_only_in_the_detail() {
        let file = |name: &str, text: &str, cut: bool, sent: bool| PlanFile { name: name.into(), path: format!("plans/{name}"), bytes: 9, text: text.into(), cut, sent };
        let mut m = held("the plan", true, false);
        m.get_mut("r1").unwrap().files = vec![file("a.md", "FILE-BODY-MARKER", false, true)];
        let plan_right = Rights { plan: true, ..Default::default() };
        let d = detail(&m, "r1", plan_right).unwrap();
        assert_eq!(d.files, m["r1"].files);
        assert_eq!(d.shown, d.shown_now());
        assert_ne!(d.shown, Detail::build("r1", DetailKind::Plan, "the plan", false, vec![]).shown, "the hash covers the files");
        // changing a file changes what the phone must have been shown
        m.get_mut("r1").unwrap().files = vec![file("a.md", "FILE-BODY-MARKER!", false, true)];
        assert_ne!(detail(&m, "r1", plan_right).unwrap().shown, d.shown);
        // no plan right: nothing, not even the plan
        assert_eq!(detail(&m, "r1", Rights { answer: true, question: true, ..Default::default() }), Err("right"));
        // a request never carries files, whatever it holds
        let mut req = held("ls", false, false);
        req.get_mut("r1").unwrap().files = m["r1"].files.clone();
        assert!(detail(&req, "r1", Rights { answer: true, ..Default::default() }).unwrap().files.is_empty());
        // the home and the notice of a held plan name no file and carry no text of one
        let mut sessions = Sessions::new();
        let mut s = crate::state::Session::default();
        s.id = "s1".into(); s.state = "waiting".into();
        sessions.insert("s1".into(), s);
        let json = String::from_utf8(DesktopMsg::Home(home(&sessions, &m, "Studio", 1, ComputerState::Online, vec![])).to_json()).unwrap();
        let notice = String::from_utf8(DesktopMsg::Notice(waiting_notice(&m["r1"], "r1", "title")).to_json()).unwrap();
        for out in [json, notice] { assert!(!out.contains("FILE-BODY") && !out.contains("a.md") && !out.contains("files"), "{out}"); }
        // a file that is cut or only listed blocks Approve; the same plan whole does not
        assert!(!blocks_allow(&m["r1"]));
        for bad in [file("a.md", "x", true, true), file("a.md", "", false, false)] {
            m.get_mut("r1").unwrap().files = vec![file("b.md", "y", false, true), bad];
            assert!(blocks_allow(&m["r1"]));
        }
        // so does a plan that names files which are not shown at all; the detail says how many, and the hash covers the count
        m.get_mut("r1").unwrap().files = vec![file("b.md", "y", false, true)];
        let whole = detail(&m, "r1", plan_right).unwrap();
        m.get_mut("r1").unwrap().files_omitted = 2;
        let more = detail(&m, "r1", plan_right).unwrap();
        assert!(blocks_allow(&m["r1"]) && (whole.omitted, more.omitted) == (0, 2) && more.shown != whole.shown && more.shown == more.shown_now());
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
    fn each_last_line_keeps_its_time_through_every_cut() {
        let rights = Rights { activity: true, ..Default::default() };
        let session = |n: u64, width: usize| {
            let mut s = crate::state::Session::default();
            s.id = "s1".into(); s.updated = 9999; s.summary = "done".into();
            s.log = (0..n).map(|i| crate::state::LogLine { t: 100 + i, kind: "tool".into(), text: "x".repeat(width), tool: "Bash".into() }).collect();
            let mut m = Sessions::new();
            m.insert("s1".into(), s);
            m
        };
        let aligned = |d: &Detail| assert_eq!(d.times.len(), d.text.split('\n').count(), "one time per line");
        // 40 lines at most: the newest, the reply last with the session's update time
        let d = session_detail(&session(50, 5), "s1", rights).unwrap();
        aligned(&d);
        assert_eq!((d.times[0], d.times[38], d.times[39]), (111, 149, 9999));
        // the byte cut drops the newest lines, and their times with them
        let d = session_detail(&session(40, 900), "s1", rights).unwrap();
        assert!(d.cut && d.times.len() < 40);
        aligned(&d);
        assert_eq!(d.times[0], 100 + 1, "the oldest kept line of the last 40 is the 2nd of 40 lines + reply");
        // no lines, no times (and a desktop that sends none leaves the field out)
        let mut empty = session(0, 1);
        empty.get_mut("s1").unwrap().summary.clear();
        assert!(session_detail(&empty, "s1", rights).unwrap().times.is_empty());
    }

    #[test]
    fn last_lines_have_no_prompts_and_no_secrets_and_need_the_right() {
        let mut sessions = Sessions::new();
        let mut s = crate::state::Session::default();
        s.id = "s1".into();
        let l = |kind: &str, text: &str, tool: &str| crate::state::LogLine { t: 1, kind: kind.into(), text: text.into(), tool: tool.into() };
        s.summary = "All tests pass now".into();
        s.log = vec![l("tool", "OLD TURN", "Read"), l("prompt", "SECRET PROMPT", ""), l("tool", "npm test ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4", "Bash"), l("tool", "src/a.rs", "Edit")];
        sessions.insert("s1".into(), s);
        let rights = Rights { activity: true, ..Default::default() };
        let d = session_detail(&sessions, "s1", rights).unwrap();
        assert_eq!(d.what, DetailKind::LastLines);
        assert!(d.text.contains("Bash npm test") && d.text.contains("Edit src/a.rs") && !d.text.contains("SECRET PROMPT") && !d.text.contains("ghp_a1B2"), "{}", d.text);
        assert!(!d.text.contains("OLD TURN"), "only the lines since the last prompt");
        assert_eq!(d.text.lines().last(), Some("All tests pass now"), "Claude's reply comes last, as a bare line");
        assert_eq!(session_detail(&sessions, "s1", Rights::default()), Err("right"));
        assert_eq!(session_detail(&sessions, "nope", rights), Err("unknown"));
        // the count Home carries is what `detail` returns, and only a phone that may read the lines gets it
        let h = home(&sessions, &HashMap::new(), "Studio", 1, ComputerState::Online, vec![]);
        assert_eq!(h.sessions[0].lines, 3, "two tools and the reply; not the prompt, not the old turn");
        assert_eq!(for_phone(h.clone(), rights).sessions[0].lines, 3);
        assert_eq!(for_phone(h, Rights::default()).sessions[0].lines, 0, "no right: no count");
        sessions.get_mut("s1").unwrap().log.clear();
        sessions.get_mut("s1").unwrap().summary.clear();
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

    #[test]
    fn the_issue_list_is_the_work_views_ticked_repos_redacted_and_capped() {
        use serde_json::json;
        let tok = format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0k1L2m3N4");
        let issue = |n: i64, title: &str, sessions: serde_json::Value| json!({ "iid": n, "project": "acme/web", "title": title, "url": format!("https://github.com/acme/web/issues/{n}"), "updated": "2026-11-16T09:00:00Z", "sessions": sessions });
        let view = json!({
            "status": "ok", "hosts": ["github.com", "gitlab.example.test"],
            "local": [{ "path": "acme/web", "dir": "/x", "issues": [issue(7, &format!("Fix login with {tok}\u{202e}"), json!(["s1"])), issue(8, "Second", json!([]))] }],
            "away": [{ "path": "acme/docs", "issues": [json!({ "iid": 3, "project": "acme/docs", "title": "Typo", "url": "https://github.com/acme/docs/issues/3", "sessions": [] })] },
                     { "path": "other/not-ticked", "issues": [json!({ "iid": 1, "project": "other/not-ticked", "title": "x", "url": "https://github.com/other/not-ticked/issues/1", "sessions": [] })] }],
            "dirs": { "acme/web": "/x", "acme/empty": "/y" },
        });
        let settings = json!({ "workRepos": { "github.com": ["acme/web", "acme/docs", "acme/empty"], "gitlab.example.test": ["g/api"], "unread.example.test": ["z/z"] } });
        let all = Rights { start: true, ..Default::default() };
        let DesktopMsg::Issues(l) = issues_reply(|| view.clone(), &settings, all) else { panic!("a phone with the right gets the list") };
        let names: Vec<(&str, &str, bool, usize)> = l.repos.iter().map(|r| (r.path.as_str(), r.host.as_str(), r.cloned, r.issues.len())).collect();
        assert_eq!(names, [("acme/docs", "github.com", false, 1), ("acme/empty", "github.com", true, 0), ("acme/web", "github.com", true, 2), ("g/api", "gitlab.example.test", false, 0)],
            "ticked repos on the hosts read, with no issues too; one that is not ticked, or on a host not read, is left out");
        let first = &l.repos[2].issues[0];
        assert!(!first.title.contains(&tok) && !first.title.contains('\u{202e}') && first.has_session && !l.repos[2].issues[1].has_session, "{first:?}");
        assert_eq!((first.iid, first.updated.as_deref()), (7, Some("2026-11-16T09:00:00Z")));
        assert_eq!(l.error, None);
        assert!(DesktopMsg::Issues(l).validate().is_ok());
        // a failed read says so and keeps what it has; the first read says it is not done
        for (status, want) in [("error", IssuesError::Read), ("loading", IssuesError::Reading)] {
            let DesktopMsg::Issues(l) = issues_reply(|| json!({ "status": status, "hosts": ["github.com"], "local": [], "away": [] }), &settings, all) else { panic!() };
            assert_eq!((l.error, l.repos.len()), (Some(want), 3));
        }
        // Work off: nothing is listed, and it is not an error
        assert_eq!(issues_reply(|| json!({ "status": "off" }), &settings, all), DesktopMsg::Issues(IssueList::default()));
        // a phone without the Start right is refused
        // (and the work view is not even built for it)
        let built = std::cell::Cell::new(false);
        assert_eq!(issues_reply(|| { built.set(true); view.clone() }, &settings, Rights { answer: true, ..Default::default() }), DesktopMsg::Error { code: "right".into(), who: Some(Who::Desktop) });
        assert!(!built.get());
        // caps: the issues over all repos, the repos
        let many: Vec<serde_json::Value> = (1..=150).map(|n| issue(n, &"t".repeat(300), json!([]))).collect();
        let big = json!({ "status": "ok", "hosts": ["github.com"], "local": [{ "path": "acme/web", "issues": many }], "away": [] });
        let DesktopMsg::Issues(l) = issues_reply(|| big.clone(), &settings, all) else { panic!() };
        assert_eq!(l.repos.iter().map(|r| r.issues.len()).sum::<usize>(), MAX_ISSUES);
        assert!(l.repos.iter().flat_map(|r| &r.issues).all(|i| i.title.len() <= MAX_ISSUE_TITLE));
        let paths: Vec<String> = (0..60).map(|n| format!("o/r{n:02}")).collect();
        let DesktopMsg::Issues(l) = issues_reply(|| json!({ "status": "ok", "hosts": ["github.com"], "local": [], "away": [] }), &json!({ "workRepos": { "github.com": paths } }), all) else { panic!() };
        assert_eq!(l.repos.len(), MAX_ISSUE_REPOS);
    }
}
