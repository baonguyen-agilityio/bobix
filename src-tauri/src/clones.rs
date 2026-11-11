// Local clones of the projects whose issues Bobix lists, so their tickets can start even where Claude never ran yet:
// found by their git remotes in the usual code folders (only `.git/config` is read, never a file of the project),
// chosen by you (a folder whose remote is checked first), or cloned on your click with your own glab / gh. They are
// remembered in Bobix's settings (`workDirs`) and can be forgotten in Settings → Work.
use crate::{model, procs};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, fs, path::{Path, PathBuf}, sync::LazyLock};

// Where people keep their clones, under the home folder; the parents of the folders sessions ran in are added.
pub const ROOTS: [&str; 10] = ["code", "projects", "src", "dev", "Desktop", "Documents", "work", "Developer", "repos", "git"];
const DEPTH: usize = 3; // ~/code/<group>/<repo>, ~/Documents/a/b/<repo>
const MAX_DIRS: usize = 20_000; // a home with a huge Documents folder stops here
// Folders never worth walking into for a clone.
const SKIP: [&str; 9] = ["node_modules", "target", "dist", "build", "vendor", "Pods", "Library", "Applications", "Pictures"];

// `~/.ssh/config` aliases → their real host ("Host gh-work" … "HostName github.com"): a remote through an alias counts
// for that host. Only Host and HostName lines; a pattern (* or ?) is no alias; the first HostName of a block wins, as
// in ssh. Read once per run (ponytail: an alias added later counts after a restart).
pub fn ssh_aliases(text: &str) -> HashMap<String, String> {
    let (mut out, mut names): (HashMap<String, String>, Vec<String>) = (HashMap::new(), vec![]);
    for l in text.lines() {
        let l = l.trim();
        let Some((k, v)) = l.split_once(|c: char| c.is_whitespace() || c == '=') else { continue };
        let v = v.trim_start_matches(|c: char| c.is_whitespace() || c == '=').trim();
        match k.to_lowercase().as_str() {
            "host" => names = v.split_whitespace().filter(|n| !n.contains(['*', '?', '!'])).map(str::to_lowercase).collect(),
            "match" => names.clear(),
            "hostname" => for n in names.drain(..) { out.entry(n).or_insert_with(|| v.to_lowercase()); },
            _ => {}
        }
    }
    out
}
static SSH: LazyLock<HashMap<String, String>> = LazyLock::new(|| ssh_aliases(&fs::read_to_string(crate::paths::home().join(".ssh").join("config")).unwrap_or_default()));
// A remote's URL with an ssh alias for its host replaced by the real one (git@gh-work:a/b.git → git@github.com:a/b.git,
// ssh://git@gh-work/a/b → ssh://git@github.com/a/b). https URLs and unknown hosts stay as they are.
pub fn unalias(url: &str) -> String { unalias_with(url, &SSH) }
fn unalias_with(url: &str, aliases: &HashMap<String, String>) -> String {
    let (pre, rest) = match url.split_once("://") { Some((s, r)) if s.eq_ignore_ascii_case("ssh") || s.eq_ignore_ascii_case("git+ssh") => (format!("{s}://"), r), Some(_) => return url.into(), None => (String::new(), url) };
    let (user, rest) = match rest.split_once('@') { Some((u, r)) if !u.contains(['/', ':']) => (format!("{u}@"), r), _ => (String::new(), rest) };
    let end = rest.find([':', '/']).unwrap_or(rest.len());
    match aliases.get(&rest[..end].to_lowercase()) { Some(h) => format!("{pre}{user}{h}{}", &rest[end..]), None => url.into() }
}

// The URLs of every remote in a repository's `.git/config` (a `.git` folder; a worktree's or submodule's `.git` file is
// left out: its main checkout is found instead).
pub fn remotes_of(repo: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(repo.join(".git").join("config")) else { return vec![] };
    let (mut in_remote, mut out) = (false, Vec::new());
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') { in_remote = t.starts_with("[remote "); continue; }
        if !in_remote { continue; }
        if let Some((k, v)) = t.split_once('=') {
            let v = v.trim().trim_matches('"').to_string();
            let v = unalias(&v);
            if matches!(k.trim(), "url" | "pushurl") && !v.is_empty() && !out.contains(&v) { out.push(v); }
        }
    }
    out
}

// The folders to look in: ROOTS under `home`, then the parents of `known` (folders sessions ran in), each once.
pub fn roots(home: &Path, known: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ROOTS.iter().map(|r| home.join(r)).filter(|p| p.is_dir()).collect();
    for k in known {
        if let Some(p) = Path::new(k).parent().filter(|p| p.is_dir() && *p != home) {
            if !out.iter().any(|o| p.starts_with(o)) { out.push(p.to_path_buf()); }
        }
    }
    out
}

// Walks `roots` (DEPTH levels, MAX_DIRS folders at most, hidden folders and SKIP left out) for repositories whose
// remote names one of `wanted` (project paths, lower case) on one of `hosts`. → project path → its folder, the first
// (shallowest) found.
pub fn scan(roots: &[PathBuf], hosts: &[String], wanted: &HashSet<String>) -> HashMap<String, String> {
    let mut found = HashMap::new();
    if wanted.is_empty() { return found; }
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    let (mut seen, mut visited) = (HashSet::new(), 0usize);
    while let Some((dir, depth)) = queue.pop_front() {
        visited += 1;
        if visited > MAX_DIRS || found.len() == wanted.len() { break; }
        if !seen.insert(dir.clone()) { continue; }
        if dir.join(".git").is_dir() {
            if let Some(p) = model::path_in(&remotes_of(&dir), hosts).filter(|p| wanted.contains(p)) {
                found.entry(p).or_insert_with(|| dir.to_string_lossy().into_owned());
            }
            continue; // a repository's own folders hold no other clone worth finding
        }
        if depth >= DEPTH { continue; }
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut subs: Vec<PathBuf> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| { let n = e.file_name().to_string_lossy().into_owned(); !n.starts_with('.') && !SKIP.contains(&n.as_str()) })
            .map(|e| e.path()).collect();
        subs.sort();
        queue.extend(subs.into_iter().map(|s| (s, depth + 1)));
    }
    found
}

// A folder you chose for `path`: its repository's top folder when one of its remotes names that project on one of
// `hosts`. Err: { reason: notgit } (not in a git repository) or { reason: other, found } (another project, or none).
pub fn check(dir: &Path, path: &str, hosts: &[String]) -> Result<String, Value> {
    let Some((g, remotes)) = crate::work::git_status(&dir.to_string_lossy()) else { return Err(json!({ "reason": "notgit" })) };
    let top = g["top"].as_str().unwrap_or("").to_string();
    match model::path_in(&remotes, hosts) {
        Some(p) if p == path => Ok(top),
        found => Err(json!({ "reason": "other", "found": found, "dir": top })),
    }
}

// Where a clone of `path` goes in `parent`: its last part ("acme/web-shop" → <parent>/web-shop).
pub fn clone_dest(parent: &Path, path: &str) -> PathBuf { parent.join(path.rsplit('/').next().unwrap_or(path)) }

// Free space a clone needs at least before Bobix starts it (the repository's own size is not known beforehand).
pub const CLONE_MIN_BYTES: u64 = 1 << 30;

// Clones `path` into `dest` with your own glab (GitLab) or gh (GitHub), signed in on `host`. → the clone's folder.
// Never into or over a folder that is there; a clone that fails leaves nothing behind (the folder it began is removed).
pub fn clone(bin: &str, gitlab: bool, host: &str, path: &str, dest: &Path) -> Result<String, String> { clone_within(bin, gitlab, host, path, dest, CLONE_SECS) }
pub const CLONE_SECS: u64 = 600;
// A clone that runs past `secs` is stopped, but its folder is left as it is: something it started (git, on Windows)
// may still be writing there. Only a clone that ended with a failure has its folder removed.
fn clone_within(bin: &str, gitlab: bool, host: &str, path: &str, dest: &Path, secs: u64) -> Result<String, String> {
    if dest.exists() { return Err(format!("{} is already there", dest.display())); }
    let d = dest.to_string_lossy();
    let t = std::time::Instant::now();
    let r = if gitlab {
        procs::exec(bin, &["repo", "clone", &format!("https://{host}/{path}"), &d], secs, &[("GITLAB_HOST", host), ("NO_PROMPT", "1"), ("NO_COLOR", "1"), ("GIT_TERMINAL_PROMPT", "0")])
    } else {
        procs::exec(bin, &["repo", "clone", path, &d], secs, &[("GH_HOST", host), ("GH_PROMPT_DISABLED", "1"), ("NO_COLOR", "1"), ("GIT_TERMINAL_PROMPT", "0")])
    };
    if !r.ok && t.elapsed().as_secs() >= secs {
        return Err(format!("no answer in {secs} s, stopped; {} was left as it is", dest.display()));
    }
    if !r.ok || !dest.join(".git").exists() {
        if dest.exists() { let _ = fs::remove_dir_all(dest); } // made by this clone: it was not there before
        let t = format!("{} {}", r.err, r.out);
        return Err(crate::js::head16(&t.split_whitespace().collect::<Vec<_>>().join(" "), 300).to_string());
    }
    Ok(d.into_owned())
}

// The folder you chose or cloned for `path` (Choose folder…, Change folder…, Clone…), while it is there: Start works
// there, whatever folder a session runs in.
pub fn pinned(settings: &Value, path: &str) -> Option<String> {
    let e = &settings["workDirs"][path];
    matches!(e["how"].as_str(), Some("chosen" | "cloned")).then(|| e["dir"].as_str().map(String::from)).flatten().filter(|d| Path::new(d).is_dir())
}

// The remembered folders as the Work view uses them: project path → folder, for the ones that still exist.
pub fn dirs(settings: &Value) -> HashMap<String, String> {
    settings["workDirs"].as_object().into_iter().flatten()
        .filter_map(|(p, e)| Some((p.clone(), e["dir"].as_str()?.to_string()))).filter(|(_, d)| Path::new(d).is_dir()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(root: &Path, rel: &str, remotes: &[(&str, &str)]) -> PathBuf {
        let d = root.join(rel);
        fs::create_dir_all(d.join(".git")).unwrap();
        let cfg: String = remotes.iter().map(|(n, u)| format!("[remote \"{n}\"]\n\turl = {u}\n\tfetch = +refs/heads/*:refs/remotes/{n}/*\n")).collect();
        fs::write(d.join(".git/config"), format!("[core]\n\tbare = false\n{cfg}[branch \"main\"]\n\tremote = origin\n")).unwrap();
        d
    }

    #[test]
    fn finds_clones_by_their_remotes_only() {
        let home = std::env::temp_dir().join(format!("bobix-clones-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let shop = repo(&home, "code/acme/web-shop", &[("origin", "git@gitlab.example.test:acme/web-shop.git")]);
        let _fork = repo(&home, "code/forks/web-shop", &[("origin", "git@gitlab.example.test:me/web-shop.git"), ("upstream", "https://gitlab.example.test/acme/web-shop.git")]);
        let api = repo(&home, "Documents/clients/x/api-gateway", &[("upstream", "https://gitlab.example.test/acme/api-gateway")]);
        repo(&home, "code/node_modules/acme/docs", &[("origin", "git@gitlab.example.test:acme/docs.git")]);
        repo(&home, "code/a/b/c/too-deep", &[("origin", "git@gitlab.example.test:acme/deep.git")]);
        repo(&home, "code/other-host", &[("origin", "git@gitlab.other.test:acme/web-shop.git")]);
        fs::write(home.join("code/acme/web-shop/secret.env"), "TOKEN=never-read").unwrap();
        assert_eq!(remotes_of(&shop), ["git@gitlab.example.test:acme/web-shop.git"]);
        let hosts = vec!["gitlab.example.test".to_string()];
        let wanted: HashSet<String> = ["acme/web-shop", "acme/api-gateway", "acme/docs", "acme/deep"].iter().map(|s| s.to_string()).collect();
        let r = roots(&home, &[]);
        assert_eq!(r, [home.join("code"), home.join("Documents")]);
        let found = scan(&r, &hosts, &wanted);
        let at = |m: &HashMap<String, String>, p: &str| m.get(p).map(PathBuf::from); // compared as paths: Windows mixes / and \
        assert_eq!(at(&found, "acme/web-shop"), Some(shop), "the shallowest of two (a fork's upstream counts too)");
        assert_eq!(at(&found, "acme/api-gateway"), Some(api), "an upstream remote, three levels down");
        assert!(!found.contains_key("acme/docs") && !found.contains_key("acme/deep"), "node_modules and too deep are left out: {found:?}");
        assert!(scan(&r, &hosts, &HashSet::new()).is_empty());
        // the parents of folders sessions ran in are looked in too, once
        let elsewhere = repo(&home, "elsewhere/acme-docs", &[("origin", "git@gitlab.example.test:acme/docs.git")]);
        let r2 = roots(&home, &[home.join("elsewhere/other").to_string_lossy().into_owned(), home.join("code/acme/x").to_string_lossy().into_owned()]);
        assert_eq!(r2, [home.join("code"), home.join("Documents"), home.join("elsewhere")]);
        assert_eq!(at(&scan(&r2, &hosts, &wanted), "acme/docs"), Some(elsewhere));
        assert_eq!(clone_dest(&home.join("code"), "acme/web-shop"), home.join("code/web-shop"));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn ssh_aliases_count_for_their_real_host() {
        let a = ssh_aliases("Host *\n  ServerAliveInterval 30\nHost gh-work gh-me\n  HostName github.com\n  HostName ignored.example\n  User git\n\
            Host=lab\n\thostname = GitLab.Example.Test\nMatch host x\n  HostName nope\nHost plain\n  Port 22\n");
        assert_eq!(a.get("gh-work").map(String::as_str), Some("github.com"));
        assert_eq!(a.get("gh-me").map(String::as_str), Some("github.com"), "every name of a Host line");
        assert_eq!(a.get("lab").map(String::as_str), Some("gitlab.example.test"), "key=value and spaces round =");
        assert_eq!(a.len(), 3, "the pattern, Match and an alias without HostName count for nothing: {a:?}");
        assert_eq!(unalias_with("git@gh-work:Acme/Shop.git", &a), "git@github.com:Acme/Shop.git");
        assert_eq!(unalias_with("ssh://git@lab:2222/g/p.git", &a), "ssh://git@gitlab.example.test:2222/g/p.git");
        assert_eq!(unalias_with("gh-me:acme/shop.git", &a), "github.com:acme/shop.git");
        for same in ["https://gh-work/acme/shop.git", "git@other:acme/shop.git", "/local/path/repo.git"] { assert_eq!(unalias_with(same, &a), same); }
        assert_eq!(model::project_path(&unalias_with("git@gh-work:Acme/Shop.git", &a), "github.com").as_deref(), Some("acme/shop"), "https, ssh and aliases all name it");
    }

    #[test]
    fn a_chosen_folder_is_checked_against_the_project() {
        let home = std::env::temp_dir().join(format!("bobix-check-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        let git = |d: &Path, a: &[&str]| { let mut v = vec!["-C", d.to_str().unwrap()]; v.extend_from_slice(a); assert!(procs::exec("git", &v, 15, &[]).ok, "{a:?}"); };
        let shop = home.join("web-shop");
        fs::create_dir_all(shop.join("src")).unwrap();
        git(&shop, &["init", "-q"]);
        git(&shop, &["remote", "add", "origin", "git@gitlab.example.test:acme/web-shop.git"]);
        let hosts = vec!["gitlab.example.test".to_string()];
        let top = check(&shop.join("src"), "acme/web-shop", &hosts).unwrap();
        assert!(Path::new(&top).ends_with("web-shop"), "a subfolder gives its repository's top: {top}");
        let e = check(&shop, "acme/api-gateway", &hosts).unwrap_err();
        assert_eq!((&e["reason"], &e["found"]), (&json!("other"), &json!("acme/web-shop")), "another project: said which");
        assert_eq!(check(&home, "acme/web-shop", &hosts).unwrap_err()["reason"], "notgit");
        assert!(clone("glab", true, "h", "acme/web-shop", &shop).is_err(), "never into a folder that is there");
        assert!(shop.join("src").is_dir(), "and that folder is left as it was");
        let _ = fs::remove_dir_all(&home);
    }

    // A clone that fails (no access, network down, disk full) leaves nothing: the folder it began is removed; one that
    // works gives the clone's folder.
    #[cfg(unix)]
    #[test]
    fn a_failed_clone_leaves_no_folder() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("bobix-clone-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let bin = |name: &str, body: &str| {
            let p = root.join(name);
            fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            p.to_string_lossy().into_owned()
        };
        let no = bin("glab-no", r#"mkdir -p "$4/.git"; echo partial > "$4/README.md"; echo "remote: You are not allowed to download code from this project." >&2; exit 1"#);
        let dest = root.join("web-shop");
        let e = clone(&no, true, "gitlab.example.test", "acme/web-shop", &dest).unwrap_err();
        assert!(e.contains("not allowed"), "the reason is shown: {e}");
        assert!(!dest.exists(), "the partial folder is gone");
        // one that runs past its time is stopped, and what it began is left alone (it may still be written to)
        let slow = bin("glab-slow", r#"mkdir -p "$4/.git"; sleep 10"#);
        let e = clone_within(&slow, true, "gitlab.example.test", "acme/web-shop", &dest, 3).unwrap_err();
        assert!(e.contains("left as it is") && dest.is_dir(), "{e}");
        fs::remove_dir_all(&dest).unwrap();
        let yes = bin("glab-yes", r#"mkdir -p "$4/.git""#);
        assert_eq!(clone(&yes, true, "gitlab.example.test", "acme/web-shop", &dest), Ok(dest.to_string_lossy().into_owned()));
        let _ = fs::remove_dir_all(&root);
    }
}
