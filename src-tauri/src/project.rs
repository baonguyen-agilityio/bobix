// Which project a session works in: the git repository its folder belongs to, so that every session of a project
// (and every git worktree of it, which Claude desktop makes per session) gets the same pet. Same rule as
// the 0.1 (Electron) app's core/project.js.
use std::{collections::HashMap, fs, path::{Path, PathBuf}, sync::Mutex};

static CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn lexical_resolve(base: &Path, rel: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for c in base.join(rel).components() {
        match c {
            std::path::Component::ParentDir => { out.pop(); }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

// The main checkout of the repository that contains `cwd`, or `cwd` itself outside git.
pub fn project_of(cwd: &str) -> String {
    if !Path::new(cwd).is_absolute() { return cwd.to_string(); }
    if let Some(k) = CACHE.lock().unwrap().as_ref().and_then(|c| c.get(cwd)) { return k.clone(); }
    let mut key = PathBuf::from(cwd);
    let mut dir = PathBuf::from(cwd);
    for _ in 0..40 {
        let dot_git = dir.join(".git");
        if let Ok(st) = fs::metadata(&dot_git) {
            if st.is_dir() { key = dir; break; }
            // A worktree: ".git" is a file "gitdir: <main>/.git/worktrees/<name>", whose "commondir" leads to <main>/.git.
            key = fs::read_to_string(&dot_git).ok()
                .and_then(|t| t.lines().find_map(|l| l.strip_prefix("gitdir:").map(|g| g.trim().to_string())))
                .map(|g| {
                    let gitdir = lexical_resolve(&dir, &g);
                    let common = fs::read_to_string(gitdir.join("commondir")).map(|c| lexical_resolve(&gitdir, c.trim())).unwrap_or(gitdir);
                    if common.file_name().is_some_and(|n| n == ".git") { common.parent().unwrap_or(&dir).to_path_buf() } else { dir.clone() }
                })
                .unwrap_or(dir);
            break;
        }
        match dir.parent() { Some(p) => dir = p.to_path_buf(), None => break }
    }
    let key = key.to_string_lossy().into_owned();
    let mut c = CACHE.lock().unwrap();
    let c = c.get_or_insert_with(HashMap::new);
    if c.len() > 500 { c.clear(); }
    c.insert(cwd.to_string(), key.clone());
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktrees_belong_to_their_repository() {
        let root = fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("bobix-proj-{}", std::process::id()));
        let (repo, wt) = (root.join("app"), root.join("wt").join("feature-x"));
        fs::create_dir_all(repo.join(".git/worktrees/feature-x")).unwrap();
        fs::create_dir_all(repo.join("src/deep")).unwrap();
        fs::write(repo.join(".git/worktrees/feature-x/commondir"), "../..\n").unwrap();
        fs::create_dir_all(wt.join("src")).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", repo.join(".git/worktrees/feature-x").display())).unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        assert_eq!(project_of(&s(&repo)), s(&repo));
        assert_eq!(project_of(&s(&repo.join("src/deep"))), s(&repo));
        assert_eq!(project_of(&s(&wt.join("src"))), s(&repo));
        let plain = root.join("notes");
        fs::create_dir_all(&plain).unwrap();
        assert_eq!(project_of(&s(&plain)), s(&plain));
        let _ = fs::remove_dir_all(&root);
    }
}
