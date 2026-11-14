// Did a change help? Each change Bobix makes for you (a skill removed, disabled or back, a Tests line, the rule block,
// a habit skill's version, the model split of a ticket) is noted here with its date and project (changes.json), and
// compared with your own sessions in that project: the ones just before it against the ones just after it. Bobix's
// own `claude -p` calls are counted too (own.json). Everything stays in the data folder of this computer: nothing is
// sent, no number is built in, and below MIN_SIDE sessions a side there is no number at all.
use crate::usage::{Past, Prices};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, path::{Path, PathBuf}};

// Sessions compared on each side of a change: the nearest WINDOW, and no number below MIN_SIDE.
pub const WINDOW: usize = 20;
pub const MIN_SIDE: usize = 10;
// A difference this small (or a range that crosses zero) is noise, and said so.
const NOISE: f64 = 0.05;
const KEEP: usize = 500;
// Changes of one kind in one project this close together are one step (three skills removed in one sitting).
const BATCH_MS: u64 = 3600 * 1000;

// The change log in steps: [{ at (the last one's), kind, project, names, n }], oldest first.
fn steps(changes: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    let mut cs: Vec<&Value> = changes.iter().filter(|c| c["at"].as_u64().is_some()).collect();
    cs.sort_by_key(|c| c["at"].as_u64());
    for c in cs {
        let (at, kind, proj) = (c["at"].as_u64().unwrap_or(0), c["kind"].as_str().unwrap_or(""), c["project"].as_str().unwrap_or(""));
        let name = c["name"].as_str().unwrap_or("");
        if let Some(l) = out.last_mut().filter(|l| l["kind"] == kind && l["project"] == proj && kind != "split" && at.saturating_sub(l["at"].as_u64().unwrap_or(0)) <= BATCH_MS) {
            l["at"] = json!(at);
            l["names"].as_array_mut().unwrap().push(json!(name));
            l["n"] = json!(l["names"].as_array().unwrap().len());
            continue;
        }
        out.push(json!({ "at": at, "kind": kind, "project": proj, "names": [name], "n": 1 }));
    }
    out
}

fn changes_file(data: &Path) -> PathBuf { data.join("changes.json") }
fn own_file(data: &Path) -> PathBuf { data.join("own-calls.json") }
fn read_list(f: &Path) -> Vec<Value> { fs::read_to_string(f).ok().and_then(|t| serde_json::from_str::<Vec<Value>>(&t).ok()).unwrap_or_default() }
fn append(f: &Path, v: Value) {
    let mut l = read_list(f);
    l.push(v);
    if l.len() > KEEP { l.drain(..l.len() - KEEP); }
    if let Some(d) = f.parent() { let _ = fs::create_dir_all(d); }
    if let Err(e) = crate::settings::save(f, &json!(l)) { eprintln!("[bobix] effects: {e}"); }
}

// The repository a folder belongs to (a worktree counts as its main repository); None outside git. Sessions and
// changes are matched by it.
pub fn project_of(dir: &str) -> Option<String> {
    if dir.is_empty() { return None; }
    let p = Path::new(dir);
    // <repo>/.claude/worktrees/<name>/… (Claude's worktrees) and <repo>/.bobix/worktrees/<name>/… (a ticket flow's):
    // the repository above it (by path parts, so "\\" and "/" both count)
    let main = p.ancestors().find(|a| a.file_name().is_some_and(|n| n == "worktrees") && a.parent().and_then(Path::file_name).is_some_and(|n| n == ".claude" || n == ".bobix"))
        .and_then(|w| w.parent()?.parent()).unwrap_or(p);
    main.ancestors().find(|d| d.join(".git").exists()).map(|d| d.to_string_lossy().into_owned())
}

// kind: skill-disable | skill-remove | undo | tests | rules | habit | split. project: a repository folder, or "" for
// every project (a personal skill). name: what changed (a skill's name, "on" / "off" for the split).
pub fn record(data: &Path, kind: &str, project: Option<&str>, name: &str, at: u64) {
    let p = project.and_then(project_of).unwrap_or_default();
    append(&changes_file(data), json!({ "at": at, "kind": kind, "project": p, "name": crate::js::head16(name, 120) }));
}
pub fn changes(data: &Path) -> Vec<Value> { read_list(&changes_file(data)) }

// One of Bobix's own `claude -p` calls: what for (scan, draft, size, summary, room), the model, and its tokens when
// claude said them (`--output-format json`).
pub fn own(data: &Path, what: &str, model: &str, answer: Option<&Value>, at: u64) {
    let u = answer.map(|v| &v["usage"]);
    let tokens = u.and_then(|u| u.as_object().map(|_| ["input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"].iter().map(|k| u[*k].as_u64().unwrap_or(0)).sum::<u64>()));
    append(&own_file(data), json!({ "at": at, "what": what, "model": model, "tokens": tokens, "usd": answer.and_then(|v| v["total_cost_usd"].as_f64()) }));
}

// ---------- numbers ----------
fn sorted(mut v: Vec<f64>) -> Vec<f64> { v.sort_by(|a, b| a.total_cmp(b)); v }
fn q(v: &[f64], p: f64) -> f64 {
    if v.is_empty() { return 0.0; }
    let x = p * (v.len() - 1) as f64;
    let (i, f) = (x.floor() as usize, x - x.floor());
    if i + 1 < v.len() { v[i] + (v[i + 1] - v[i]) * f } else { v[i] }
}
pub fn median(v: &[f64]) -> f64 { q(&sorted(v.to_vec()), 0.5) }
pub fn tokens(p: &Past) -> u64 { p.models.values().map(|t| t.input + t.output + t.cw5 + t.cw1h + t.cr).sum() }
fn reread(p: &Past) -> f64 {
    let (cr, ctx): (u64, u64) = p.models.values().fold((0, 0), |(a, b), t| (a + t.cr, b + t.input + t.cw5 + t.cw1h + t.cr));
    if ctx == 0 { 0.0 } else { cr as f64 / ctx as f64 }
}
// The model that did most of a set of sessions' tokens.
fn main_model(s: &[&Past]) -> String {
    let mut by: HashMap<&str, u64> = HashMap::new();
    for p in s { for (m, t) in &p.models { *by.entry(m.as_str()).or_default() += t.input + t.output + t.cw5 + t.cw1h + t.cr; } }
    by.into_iter().max_by_key(|x| x.1).map(|x| x.0.to_string()).unwrap_or_default()
}

// Before against after, on medians: the change of the median and a range from the quartiles (lower quartile against
// lower, upper against upper). noise: the range crosses zero or the change is under NOISE.
pub fn compare(before: &[f64], after: &[f64]) -> Value {
    let (b, a) = (sorted(before.to_vec()), sorted(after.to_vec()));
    let rel = |x: f64, y: f64| if x > 0.0 { y / x - 1.0 } else { 0.0 };
    let (mb, ma) = (q(&b, 0.5), q(&a, 0.5));
    let d = rel(mb, ma);
    let (r1, r2) = (rel(q(&b, 0.25), q(&a, 0.25)), rel(q(&b, 0.75), q(&a, 0.75)));
    let (lo, hi) = (r1.min(r2).min(d), r1.max(r2).max(d));
    json!({ "before": mb, "after": ma, "change": d, "low": lo, "high": hi, "noise": d.abs() < NOISE || (lo < 0.0 && hi > 0.0) })
}

// The measure a change is judged by: what a session reads before it starts (the first turn's input) for what sits in
// every session's context (skills, the rule block, a habit skill); tokens per session for the rest.
fn first_turn(kind: &str) -> bool { kind.starts_with("skill") || kind == "rules" || kind == "habit" || kind == "undo" }

// ---------- the view ----------
// past: your sessions (usage.json), oldest first. flows: (session id, ticket key, split on). This computer only.
pub fn view(past: &[Past], changes: &[Value], own_calls: &[Value], flows: &[(String, String, bool)], prices: &Prices, now: u64) -> Value {
    let key = |p: &str| crate::paths::dir_key(p);
    let mut ps: Vec<&Past> = past.iter().filter(|p| !p.project.is_empty() && p.turns > 0).collect();
    ps.sort_by_key(|p| p.end);
    // per project: your usual session, and your usual ticket
    let mut by: Vec<(String, Vec<&Past>)> = vec![];
    for p in &ps { match by.iter_mut().find(|x| key(&x.0) == key(&p.project)) { Some(x) => x.1.push(p), None => by.push((p.project.clone(), vec![p])) } }
    let flow_of: HashMap<&str, (&str, bool)> = flows.iter().map(|(s, k, split)| (s.as_str(), (k.as_str(), *split))).collect();
    let med = |v: &[&Past], f: &dyn Fn(&Past) -> f64| median(&v.iter().map(|p| f(p)).collect::<Vec<_>>());
    let projects: Vec<Value> = by.iter().map(|(name, v)| {
        let tickets: Vec<f64> = { let mut t: HashMap<&str, u64> = HashMap::new(); for p in v { if let Some((k, _)) = flow_of.get(p.id.as_str()) { *t.entry(k).or_default() += tokens(p); } } t.values().map(|x| *x as f64).collect() };
        json!({ "project": name, "n": v.len(), "enough": v.len() >= MIN_SIDE, "tokens": med(v, &|p| tokens(p) as f64), "first": med(v, &|p| p.first as f64),
            "reread": med(v, &|p| reread(p)), "turns": med(v, &|p| p.turns as f64), "prompts": med(v, &|p| p.prompts as f64), "activeMs": med(v, &|p| p.active_ms as f64),
            "tickets": tickets.len(), "ticketTokens": if tickets.is_empty() { Value::Null } else { json!(median(&tickets)) } })
    }).collect();
    // each step against your sessions around it
    let changes = steps(changes);
    let changes = &changes;
    let mut list: Vec<Value> = changes.iter().filter_map(|c| Some((c["at"].as_u64()?, c))).map(|(at, c)| {
        let kind = c["kind"].as_str().unwrap_or("");
        let proj = c["project"].as_str().unwrap_or("");
        let mut v = json!({ "at": at, "kind": kind, "project": proj, "name": c["names"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default(), "n": c["n"] });
        if kind == "split" { v["status"] = json!("ticket"); return v; } // judged per ticket: see `split` below
        let mine: Vec<&Past> = ps.iter().copied().filter(|p| proj.is_empty() || key(&p.project) == key(proj)).collect();
        let before: Vec<&Past> = { let b: Vec<&Past> = mine.iter().copied().filter(|p| p.end < at).collect(); b[b.len().saturating_sub(WINDOW)..].to_vec() };
        let after: Vec<&Past> = mine.iter().copied().filter(|p| p.end >= at).take(WINDOW).collect();
        v["sessions"] = json!([before.len(), after.len()]);
        v["need"] = json!(MIN_SIDE);
        if before.len() < MIN_SIDE || after.len() < MIN_SIDE { v["status"] = json!("short"); return v; }
        // another change in the same sessions' time: they overlap, and no number goes to this one alone
        let (from, to) = (before[0].end, after[after.len() - 1].end);
        let others: Vec<Value> = changes.iter().filter(|o| !std::ptr::eq(*o, c) && o["kind"] != "split")
            .filter(|o| { let p = o["project"].as_str().unwrap_or(""); proj.is_empty() || p.is_empty() || key(p) == key(proj) })
            .filter(|o| o["at"].as_u64().is_some_and(|t| t > from && t <= to)).map(|o| json!({ "at": o["at"], "kind": o["kind"], "n": o["n"] })).collect();
        if !others.is_empty() { v["status"] = json!("overlap"); v["with"] = json!(others); return v; }
        let metric = |p: &Past| if first_turn(kind) { p.first as f64 } else { tokens(p) as f64 };
        v["status"] = json!("ok");
        v["metric"] = json!(if first_turn(kind) { "first" } else { "tokens" });
        v["result"] = compare(&before.iter().map(|p| metric(p)).collect::<Vec<_>>(), &after.iter().map(|p| metric(p)).collect::<Vec<_>>());
        v["tokens"] = compare(&before.iter().map(|p| tokens(p) as f64).collect::<Vec<_>>(), &after.iter().map(|p| tokens(p) as f64).collect::<Vec<_>>());
        // the kind of work: tickets with the model split are the bigger ones; a mix that moved a lot is warned about
        let share = |s: &[&Past]| { let k: Vec<bool> = s.iter().filter_map(|p| flow_of.get(p.id.as_str()).map(|x| x.1)).collect(); (k.len() >= 5).then(|| k.iter().filter(|x| **x).count() as f64 / k.len() as f64) };
        if let (Some(x), Some(y)) = (share(&before), share(&after)) { if (x - y).abs() > 0.3 { v["mix"] = json!({ "before": x, "after": y }); } }
        // another model between the two: tokens still compare, money would not
        let (mb, ma) = (main_model(&before), main_model(&after));
        if mb != ma { v["models"] = json!([mb, ma]); }
        v
    }).collect();
    list.reverse(); // newest first
    // the model split: tickets with it against tickets without, tokens measured, money from the price table
    // (split on, tokens, usd); a model the price table doesn't price leaves money out (named in `unpriced`)
    let mut tk: HashMap<&str, (bool, u64, f64)> = HashMap::new();
    let mut unpriced: Vec<String> = vec![];
    for p in &ps {
        let Some((k, split)) = flow_of.get(p.id.as_str()) else { continue };
        let mut usd = 0.0;
        for (m, t) in &p.models {
            match prices.of(m) {
                Some(pr) => usd += (t.input as f64 * pr.input + t.output as f64 * pr.output + t.cw5 as f64 * pr.cw5 + t.cw1h as f64 * pr.cw1h + t.cr as f64 * pr.cr) / 1e6,
                None => if !unpriced.contains(m) { unpriced.push(m.clone()) },
            }
        }
        let e = tk.entry(k).or_insert((*split, 0, 0.0));
        e.1 += tokens(p); e.2 += usd;
    }
    let side = |on: bool| tk.values().filter(|x| x.0 == on).map(|x| (x.1 as f64, x.2)).collect::<Vec<_>>();
    let (with, without) = (side(true), side(false));
    let mut split = json!({ "n": [with.len(), without.len()], "need": MIN_SIDE, "date": prices.date, "unpriced": unpriced });
    if with.len() >= MIN_SIDE && without.len() >= MIN_SIDE {
        split["tokens"] = compare(&without.iter().map(|x| x.0).collect::<Vec<_>>(), &with.iter().map(|x| x.0).collect::<Vec<_>>());
        if unpriced.is_empty() { split["usd"] = compare(&without.iter().map(|x| x.1).collect::<Vec<_>>(), &with.iter().map(|x| x.1).collect::<Vec<_>>()); }
    }
    // Bobix's own calls, the last 30 days
    let recent: Vec<&Value> = own_calls.iter().filter(|c| c["at"].as_u64().is_some_and(|t| now.saturating_sub(t) < 30 * 86_400_000)).collect();
    let mut what: HashMap<String, u64> = HashMap::new();
    for c in &recent { *what.entry(c["what"].as_str().unwrap_or("?").to_string()).or_default() += 1; }
    let known: Vec<u64> = recent.iter().filter_map(|c| c["tokens"].as_u64()).collect();
    let own = json!({ "calls": recent.len(), "by": what, "tokens": known.iter().sum::<u64>(), "withTokens": known.len() });
    json!({ "projects": projects, "changes": list, "split": split, "own": own, "minSide": MIN_SIDE, "window": WINDOW, "sessions": ps.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Tok;
    use std::collections::BTreeMap;

    // A made-up past session: `tok` tokens (all cache reads but `first` of first-turn input), in `proj`, ending at `end`.
    fn s(id: &str, proj: &str, end: u64, tok: u64, first: u64, model: &str) -> Past {
        Past { id: id.into(), end, active_ms: 600_000, turns: 12, models: BTreeMap::from([(model.into(), Tok { input: 0, output: 0, cw5: 0, cw1h: 0, cr: tok, turns: 12 })]),
            tasks: Default::default(), project: proj.into(), first, prompts: 3 }
    }
    fn change(at: u64, kind: &str, proj: &str) -> Value { json!({ "at": at, "kind": kind, "project": proj, "name": "demo-skill" }) }
    fn series(proj: &str, from: u64, n: u64, tok: impl Fn(u64) -> u64, model: &str) -> Vec<Past> { (0..n).map(|i| s(&format!("{proj}-{from}-{i}"), proj, from + i, tok(i), tok(i) / 10, model)).collect() }
    fn prices() -> Prices { crate::usage::parse_prices(crate::usage::SEED).unwrap() }

    #[test]
    fn no_history_and_too_few_sessions_give_no_number() {
        let v = view(&[], &[change(100, "skill-remove", "/p")], &[], &[], &prices(), 1000);
        assert_eq!((&v["changes"][0]["status"], &v["changes"][0]["sessions"]), (&json!("short"), &json!([0, 0])), "a new user: not enough data");
        assert!(v["changes"][0].get("result").is_none());
        let mut past = series("/p", 0, 6, |_| 1000, "claude-sonnet-5-5");
        past.extend(series("/p", 200, 14, |_| 800, "claude-sonnet-5-5"));
        let v = view(&past, &[change(100, "skill-remove", "/p")], &[], &[], &prices(), 1000);
        assert_eq!((&v["changes"][0]["status"], &v["changes"][0]["sessions"]), (&json!("short"), &json!([6, 14])), "6 of 10 before: no number");
        assert_eq!(v["projects"][0]["n"], 20);
    }

    #[test]
    fn before_and_after_on_medians_with_a_range() {
        // a few huge outliers do not move a median
        let mut past = series("/p", 0, 14, |i| if i < 2 { 50_000_000 } else { 1_000_000 + i * 1000 }, "claude-sonnet-5-5");
        past.extend(series("/p", 200, 14, |i| 880_000 + i * 1000, "claude-sonnet-5-5"));
        past.extend(series("/other", 50, 30, |_| 9, "claude-sonnet-5-5")); // another project: not counted
        let v = view(&past, &[change(100, "tests", "/p")], &[], &[], &prices(), 1000);
        let c = &v["changes"][0];
        assert_eq!((&c["status"], &c["sessions"], &c["metric"]), (&json!("ok"), &json!([14, 14]), &json!("tokens")));
        let d = c["result"]["change"].as_f64().unwrap();
        assert!((-0.13..=-0.11).contains(&d), "about −12%: {d}");
        assert_eq!(c["result"]["noise"], false);
        assert!(c["result"]["low"].as_f64().unwrap() <= d && c["result"]["high"].as_f64().unwrap() >= d);
        // a skill change is judged by the first turn's input
        let v = view(&past, &[change(100, "skill-disable", "/p")], &[], &[], &prices(), 1000);
        assert_eq!(v["changes"][0]["metric"], "first");
        // the same sessions either side: noise, said so
        let flat: Vec<Past> = series("/p", 0, 12, |i| 1000 + (i % 3) * 100, "m").into_iter().chain(series("/p", 200, 12, |i| 1000 + ((i + 1) % 3) * 100, "m")).collect();
        assert_eq!(view(&flat, &[change(100, "tests", "/p")], &[], &[], &prices(), 1000)["changes"][0]["result"]["noise"], true);
    }

    #[test]
    fn overlapping_changes_and_undo_are_not_put_on_one_change() {
        let mut past = series("/p", 0, 12, |_| 1000, "m");
        past.extend(series("/p", 200, 12, |_| 800, "m"));
        let v = view(&past, &[change(100, "skill-remove", "/p"), change(205, "undo", "/p")], &[], &[], &prices(), 1000);
        let first = v["changes"].as_array().unwrap().iter().find(|c| c["kind"] == "skill-remove").unwrap();
        assert_eq!(first["status"], "overlap", "the undo falls inside its sessions");
        assert_eq!(first["with"][0]["kind"], "undo");
        // a personal skill (every project) overlaps a project's change too
        let v = view(&past, &[change(100, "tests", "/p"), change(101, "skill-remove", "")], &[], &[], &prices(), 1000);
        assert!(v["changes"].as_array().unwrap().iter().all(|c| c["status"] == "overlap"));
    }

    #[test]
    fn a_new_model_and_another_mix_of_work_are_said() {
        let mut past = series("/p", 0, 12, |_| 1000, "claude-sonnet-4-5");
        past.extend(series("/p", 200, 12, |_| 900, "claude-sonnet-5-5"));
        let flows: Vec<(String, String, bool)> = past.iter().enumerate().map(|(i, p)| (p.id.clone(), format!("t{i}"), i >= 12)).collect(); // small before, split after
        let v = view(&past, &[change(100, "tests", "/p")], &[], &flows, &prices(), 1000);
        let c = &v["changes"][0];
        assert_eq!(c["models"], json!(["claude-sonnet-4-5", "claude-sonnet-5-5"]), "tokens compare, money would not");
        assert_eq!((&c["mix"]["before"], &c["mix"]["after"]), (&json!(0.0), &json!(1.0)));
    }

    #[test]
    fn the_model_split_per_ticket_and_bobix_own_calls() {
        let past: Vec<Past> = (0..24).map(|i| s(&format!("s{i}"), "/p", i, if i < 12 { 2_000_000 } else { 1_500_000 }, 0, "claude-sonnet-5-5")).collect();
        let flows: Vec<(String, String, bool)> = (0..24).map(|i| (format!("s{i}"), format!("t{i}"), i >= 12)).collect();
        let own = vec![json!({ "at": 990, "what": "size", "model": "haiku", "tokens": 1200 }), json!({ "at": 995, "what": "scan", "model": "sonnet", "tokens": null }), json!({ "at": 1, "what": "old" })];
        let v = view(&past, &[change(5, "split", "/p")], &own, &flows, &prices(), 1000 + 31 * 86_400_000 - 100_000_000);
        assert_eq!(v["changes"][0]["status"], "ticket");
        assert_eq!(v["split"]["n"], json!([12, 12]));
        assert!((v["split"]["tokens"]["change"].as_f64().unwrap() + 0.25).abs() < 1e-9, "split tickets: −25% tokens");
        assert!(v["split"]["usd"]["after"].as_f64().unwrap() > 0.0, "money from the price table");
        // a ticket on a model the price table doesn't know: tokens still compare, money is left out and the model named
        let mut odd = past.clone();
        odd[0].models = BTreeMap::from([("made-up-model".into(), Tok { cr: 2_000_000, turns: 12, ..Default::default() })]);
        let w = view(&odd, &[], &[], &flows, &prices(), 1000);
        assert!(w["split"]["tokens"].is_object() && w["split"].get("usd").is_none());
        assert_eq!(w["split"]["unpriced"], json!(["made-up-model"]));
        let v = view(&past, &[], &own, &flows, &prices(), 1000);
        assert_eq!((&v["own"]["calls"], &v["own"]["tokens"], &v["own"]["withTokens"]), (&json!(3), &json!(1200), &json!(1)));
        let v = view(&past, &[], &own, &flows[..5], &prices(), 1000);
        assert!(v["split"].get("tokens").is_none(), "too few tickets: no number");
    }

    // Three skills removed in one sitting are one step ("after removing 3 skills"), not three that overlap.
    #[test]
    fn changes_in_one_sitting_are_one_step() {
        let mut past = series("/p", 0, 12, |_| 1000, "m");
        past.extend(series("/p", 20_000_000, 12, |_| 800, "m"));
        let cs = [change(10_000_000, "skill-remove", "/p"), change(10_000_000 + 600_000, "skill-remove", "/p"), change(10_000_000 + 1_200_000, "skill-remove", "/p")];
        let v = view(&past, &cs, &[], &[], &prices(), 1);
        assert_eq!(v["changes"].as_array().unwrap().len(), 1);
        assert_eq!((&v["changes"][0]["n"], &v["changes"][0]["status"]), (&json!(3), &json!("ok")));
        let apart = [change(10_000_000, "skill-remove", "/p"), change(10_000_000 + 2 * BATCH_MS, "skill-remove", "/p")];
        assert_eq!(view(&past, &apart, &[], &[], &prices(), 1)["changes"].as_array().unwrap().len(), 2, "hours apart: two steps");
    }

    #[test]
    fn changes_and_calls_are_kept_locally() {
        let d = std::env::temp_dir().join(format!("bobix-effects-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let repo = d.join("code/shop");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join(".claude/worktrees/bobix-7/src")).unwrap();
        let wt = repo.join(".claude").join("worktrees").join("bobix-7").join("src");
        assert!(project_of(&wt.to_string_lossy()).is_some_and(|p| crate::paths::same_dir(&p, &repo.to_string_lossy())), "a worktree is its repository");
        let flow = repo.join(".bobix").join("worktrees").join("shop-7");
        fs::create_dir_all(&flow).unwrap();
        fs::write(flow.join(".git"), "gitdir: x\n").unwrap();
        assert!(project_of(&flow.join("src").to_string_lossy()).is_some_and(|p| crate::paths::same_dir(&p, &repo.to_string_lossy())), "a ticket flow's worktree too");
        assert!(project_of(&repo.join("src").to_string_lossy()).is_some_and(|p| crate::paths::same_dir(&p, &repo.to_string_lossy())));
        assert_eq!(project_of(&d.to_string_lossy()), None);
        record(&d, "tests", Some(&repo.to_string_lossy()), "npm test", 5);
        record(&d, "skill-remove", None, "x", 6);
        let c = changes(&d);
        assert_eq!((c.len(), &c[1]["project"]), (2, &json!("")));
        assert!(crate::paths::same_dir(c[0]["project"].as_str().unwrap(), &repo.to_string_lossy()));
        own(&d, "size", "haiku", Some(&json!({ "usage": { "input_tokens": 10, "output_tokens": 5 }, "total_cost_usd": 0.001 })), 7);
        own(&d, "scan", "sonnet", None, 8);
        let o = read_list(&own_file(&d));
        assert_eq!((&o[0]["tokens"], &o[1]["tokens"]), (&json!(15), &Value::Null));
        fs::write(changes_file(&d), "not json").unwrap();
        assert!(changes(&d).is_empty(), "a broken file reads as none, never a crash");
        let _ = fs::remove_dir_all(&d);
    }
}
