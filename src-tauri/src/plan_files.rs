// The Markdown files a held plan names, read once when the plan is held, so that a phone with the plan right can read what
// it approves. What may leave the computer is decided here and nowhere else:
//  - only a file the plan text itself names (a token ending in ".md"; no directory is listed, nothing else is searched);
//  - the name is first judged as TEXT, with no call to the system: `\` counts as `/`, `.` and `..` are folded away, and a name
//    that starts with `//` (a network share, `\\?\`, `\\.\`), is a drive-relative or device path (`C:x`, `CON`) or does not lead
//    under the session's folder is refused before anything touches the disk (the system would otherwise open a network share
//    or wait on a dead mount just to answer "does this exist"). An absolute name counts as inside when it leads under the
//    folder as Claude reported it or under its canonical form;
//  - only then is what that text leads to resolved with every symlink followed (canonicalised), and the result must again be
//    INSIDE the session's folder, be a regular file (never a directory, device or pipe, which is not even opened) and still end
//    in ".md"; so a symlink inside the folder to a file outside it is refused, and one inside to a file inside is read (as that file);
//  - at most MAX_PLAN_FILES files, at most MAX_PLAN_FILE_TEXT bytes of text each and MAX_PLAN_FILES_TEXT together, after the
//    same secret redaction as the previews and with hidden characters as visible escapes; a larger file is shortened (`cut`),
//    one over MAX_WHOLE bytes, unreadable or not text (not UTF-8, or with a NUL) is only listed (`sent` false): never read whole.
//  - the file is opened without following a symlink and without waiting on a pipe, and must be the file that was checked
//    (same device and inode).
//  - Nothing is left out unseen: a file past the eighth, a name that cannot be read and names past the first 64 tokens are
//    counted in `omitted`, and a name or folder too long to send is listed (cut, `sent` false); any of them stops an Approve.
// A name that does not resolve to such a file is not a candidate at all and is not listed.
// What stays open, accepted: a folder on the way swapped between the check and the open; and a symlink INSIDE the folder that
// points to a network path or a dead mount, which the system follows when the name is resolved (the folder is the person's own
// work folder, so such a link is theirs). The plan text cannot make one: it only names files.
use crate::{answers::escape_hidden, phone_view::cut_bytes, room};
use bobix_link::messages::{PlanFile, MAX_PLAN_FILES, MAX_PLAN_FILES_TEXT, MAX_PLAN_FILE_NAME, MAX_PLAN_FILE_PATH, MAX_PLAN_FILE_TEXT};
use std::{fs, io::Read, path::{Component, Path, PathBuf, Prefix}};

/// A file larger than this is listed by name and size, never read.
const MAX_WHOLE: u64 = MAX_PLAN_FILES_TEXT as u64;
/// How many names of the plan are tried.
const MAX_TOKENS: usize = 64;

/// What a plan's files are, as the phone is shown them: the files, and how many the plan names that could not be shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanFiles { pub files: Vec<PlanFile>, pub omitted: u32 }

/// The files `plan` names that lie inside `cwd`, in the order the plan names them.
pub fn read(plan: &str, cwd: &str) -> PlanFiles {
    let Ok(root) = fs::canonicalize(cwd) else { return PlanFiles::default() };
    if root.parent().is_none() { return PlanFiles::default(); } // the top of a disk has everything inside it
    let cwd = fold(Path::new(cwd));
    let (mut out, mut seen, mut budget) = (Vec::<PlanFile>::new(), Vec::<PathBuf>::new(), MAX_PLAN_FILES_TEXT);
    let (names, unread) = tokens(plan);
    let mut omitted = unread as u32; // names past the cap were never looked at: one may be a file
    for token in names {
        let Some(file) = resolve(&token, &root, cwd.as_deref()) else { continue };
        if seen.contains(&file) { continue; }
        seen.push(file.clone());
        if out.len() >= MAX_PLAN_FILES { omitted += 1; continue; }
        match load(&file, &root, &mut budget) { Some(f) => out.push(f), None => omitted += 1 }
    }
    PlanFiles { files: out, omitted }
}

// The words of the plan that end in ".md" (a link's anchor and a sentence's end are left off), each once; and whether the
// plan has more of them than are tried.
fn tokens(plan: &str) -> (Vec<String>, bool) {
    let mut out: Vec<String> = vec![];
    for w in plan.split(|c: char| c.is_whitespace() || "`'\"()[]<>{},;|*".contains(c)) {
        let w = w.split(['#', '?']).next().unwrap_or("").trim_end_matches(['.', ':', '!']);
        if w.len() < 4 || w.contains("://") || w.contains('\0') || !w.to_ascii_lowercase().ends_with(".md") { continue; }
        if out.iter().any(|o| o == w) { continue; }
        if out.len() >= MAX_TOKENS { return (out, true); }
        out.push(w.into());
    }
    (out, false)
}

// The file a name stands for, when it is a regular ".md" file inside `root` (already canonical). The name is judged as text
// first (`lexical`, no system call); only a name that leads inside is handed to the system, which then follows its symlinks,
// so what is tested at the end is where the name really leads.
fn resolve(token: &str, root: &Path, cwd: Option<&Path>) -> Option<PathBuf> {
    let real = fs::canonicalize(lexical(token, root, cwd)?).ok()?;
    let md = real.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
    (real.starts_with(root) && md && fs::symlink_metadata(&real).ok()?.is_file()).then_some(real)
}

// Where a name leads inside `root`, from its text alone (no system call): `\` is a separator, `.` and `..` are folded, and
// the name is refused when it starts with `//` (a network share, `\\?\`, `\\.\`), has a drive without a root (`C:x`), a
// device name or a stream (Windows), climbs above where it started, or is an absolute path that is neither under `root` nor
// under `cwd` (the folder as Claude reported it, which may be a symlinked form of `root`). The result is `root` plus the rest.
fn lexical(token: &str, root: &Path, cwd: Option<&Path>) -> Option<PathBuf> {
    let text = token.replace('\\', "/");
    if text.starts_with("//") { return None; }
    let name = fold(Path::new(&text))?;
    let first = name.components().next();
    let rel = match first {
        Some(Component::Prefix(_) | Component::RootDir) => {
            if !name.has_root() { return None; } // `C:x`: relative to the current folder of a drive
            if cfg!(windows) && !matches!(first, Some(Component::Prefix(_))) { return None; } // `/x`: no drive named
            let under = |base: &Path| name.strip_prefix(base).ok().map(Path::to_path_buf);
            under(&plain(root)).or_else(|| cwd.and_then(under))?
        }
        _ => name.clone(),
    };
    if rel.as_os_str().is_empty() { return None; }
    if cfg!(windows) && rel.components().any(|c| matches!(c, Component::Normal(n) if odd_on_windows(&n.to_string_lossy()))) { return None; }
    Some(root.join(rel))
}

// A path with `.` and `..` folded away; None when `..` would climb above its start, or the prefix is not a plain drive.
fn fold(p: &Path) -> Option<PathBuf> {
    let mut out: Vec<Component> = vec![];
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => { if matches!(out.last(), Some(Component::Normal(_))) { out.pop(); } else { return None; } }
            Component::Prefix(x) if !matches!(x.kind(), Prefix::Disk(_)) => return None,
            c => out.push(c),
        }
    }
    Some(out.iter().collect())
}

// `root` as a plain path: the system's canonical form on Windows carries `\\?\` before a drive.
fn plain(root: &Path) -> PathBuf {
    match root.to_str().and_then(|r| r.strip_prefix("\\\\?\\")) { Some(r) if !r.starts_with("UNC\\") => PathBuf::from(r), _ => root.to_path_buf() }
}

// A name Windows treats as a device (with or without an extension) or as a stream (`name:stream`).
fn odd_on_windows(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let numbered = |p: &str| stem.strip_prefix(p).is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit() && n != "0");
    name.contains(':') || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str()) || numbered("COM") || numbered("LPT")
}

// What a phone is sent of one file; None only for a file that vanished or has no name. A name or folder too long to send is
// sent cut, and the file only listed (never read).
fn load(file: &Path, root: &Path, budget: &mut usize) -> Option<PlanFile> {
    let name = escape_hidden(&file.file_name()?.to_string_lossy());
    let path = escape_hidden(&file.strip_prefix(root).ok()?.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
    let long = name.len() > MAX_PLAN_FILE_NAME || path.len() > MAX_PLAN_FILE_PATH;
    let before = fs::symlink_metadata(file).ok()?;
    let bytes = before.len();
    let listed = PlanFile { name: cut_bytes(&name, MAX_PLAN_FILE_NAME), path: cut_bytes(&path, MAX_PLAN_FILE_PATH), bytes, text: String::new(), cut: false, sent: false };
    let cap = MAX_PLAN_FILE_TEXT.min(*budget);
    if long || bytes > MAX_WHOLE || cap == 0 { return Some(listed); }
    let mut buf = vec![];
    let opened = open_same(file, &before);
    // one byte more than a file may send: reaching it means the file goes on (also when it grew since it was measured)
    if opened.and_then(|f| f.take(MAX_PLAN_FILE_TEXT as u64 + 1).read_to_end(&mut buf).ok()).is_none() { return Some(listed); }
    let whole = buf.len() <= MAX_PLAN_FILE_TEXT;
    let Some(mut raw) = utf8(&buf, whole) else { return Some(listed) };
    if raw.contains('\0') { return Some(listed); }
    // a shortened file ends at a line end, so no half of a secret is sent
    if !whole { if let Some(i) = raw.rfind('\n') { raw = &raw[..i]; } }
    let text = escape_hidden(&room::redact_to(raw, usize::MAX).0);
    let cut = !whole || text.len() > cap;
    let text = cut_bytes(&text, cap);
    *budget -= text.len();
    Some(PlanFile { text, cut, sent: true, ..listed })
}

// Opens `file` only if it is still the very file `resolve` looked at: not through a symlink put in its place, never waiting
// on a pipe put in its place (the core is locked while this runs), and with the same device and inode where there are such.
fn open_same(file: &Path, before: &fs::Metadata) -> Option<fs::File> {
    let mut o = fs::OpenOptions::new();
    o.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let f = o.open(file).ok()?;
    let now = f.metadata().ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (now.dev(), now.ino()) != (before.dev(), before.ino()) { return None; }
    }
    now.is_file().then_some(f)
}

// The bytes as text: all of a whole file must be UTF-8; a shortened file may end inside a character.
fn utf8(buf: &[u8], whole: bool) -> Option<&str> {
    match std::str::from_utf8(buf) {
        Ok(s) => Some(s),
        Err(e) if !whole && e.error_len().is_none() => std::str::from_utf8(&buf[..e.valid_up_to()]).ok(),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a scratch folder tree: `<tmp>/<case>/project` is the session's folder, `<tmp>/<case>/outside` is next to it
    struct Tree(PathBuf);
    impl Tree {
        fn new(case: &str) -> Tree {
            let t = std::env::temp_dir().join(format!("bobix-plan-files-{case}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&t);
            fs::create_dir_all(t.join("project/plans")).unwrap();
            fs::create_dir_all(t.join("outside")).unwrap();
            Tree(t)
        }
        fn project(&self) -> String { self.0.join("project").to_string_lossy().into_owned() }
        fn write(&self, rel: &str, text: &[u8]) -> PathBuf { let p = self.0.join(rel); fs::write(&p, text).unwrap(); p }
    }
    impl Drop for Tree { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

    fn files_of(plan: &str, cwd: &str) -> Vec<PlanFile> { read(plan, cwd).files }
    fn names(files: &[PlanFile]) -> Vec<&str> { files.iter().map(|f| f.name.as_str()).collect() }

    #[test]
    fn a_named_md_file_inside_the_folder_is_read_and_one_naming_it_twice_is_listed_once() {
        let t = Tree::new("inside");
        t.write("project/plans/a.md", b"## A\nstep one");
        t.write("project/plans/b.md", b"B");
        let abs = t.0.join("project/plans/b.md");
        let plan = format!("See `plans/a.md`, [b]({}#top), (plans/a.md) and ./plans/a.md.", abs.display());
        let files =files_of(&plan, &t.project());
        assert_eq!(names(&files), ["a.md", "b.md"]);
        assert_eq!((files[0].path.as_str(), files[0].text.as_str(), files[0].bytes, files[0].cut, files[0].sent), ("plans/a.md", "## A\nstep one", 13, false, true));
        assert_eq!(files[1].path, "plans/b.md", "an absolute path inside the folder is shown relative to it");
    }

    #[test]
    fn nothing_outside_the_folder_is_read() {
        let t = Tree::new("escape");
        let secret = t.write("outside/secret.md", b"outside text");
        t.write("project/plans/ok.md", b"ok");
        let project = t.project();
        // `..` out of the folder, an absolute path outside it, and the folder's own sibling that shares its name as a prefix
        fs::create_dir_all(t.0.join("project-2")).unwrap();
        fs::write(t.0.join("project-2/x.md"), "sibling").unwrap();
        let plan = format!("../outside/secret.md plans/../../outside/secret.md {} ../project-2/x.md {}/../project-2/x.md plans/ok.md", secret.display(), project);
        assert_eq!(names(&files_of(&plan, &project)), ["ok.md"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_judged_by_where_it_leads() {
        use std::os::unix::fs::symlink;
        let t = Tree::new("symlink");
        let outside = t.write("outside/secret.md", b"outside text");
        let inside = t.write("project/plans/real.md", b"inside text");
        t.write("outside/secret.txt", b"not markdown");
        symlink(&outside, t.0.join("project/plans/to-outside.md")).unwrap();
        symlink(&inside, t.0.join("project/plans/to-inside.md")).unwrap();
        symlink(t.0.join("outside/secret.txt"), t.0.join("project/plans/disguised.md")).unwrap();
        symlink(&t.0.join("outside"), t.0.join("project/plans/dir")).unwrap();
        symlink(t.0.join("project/plans/loop.md"), t.0.join("project/plans/loop.md")).unwrap();
        let project = t.project();
        assert!(files_of("plans/to-outside.md", &project).is_empty(), "a symlink to a file outside");
        assert!(files_of("plans/dir/secret.md", &project).is_empty(), "a symlink to a folder outside, then a file in it");
        assert!(files_of("plans/disguised.md", &project).is_empty(), "named .md but leads to a file that is not");
        assert!(files_of("plans/loop.md", &project).is_empty(), "a loop");
        let f =files_of("plans/to-inside.md", &project);
        assert_eq!((names(&f), f[0].text.as_str()), (vec!["real.md"], "inside text"), "a symlink to a file inside is read, as that file");
        // the folder itself reached through a symlink is the same folder
        let via = t.0.join("via");
        symlink(&project, &via).unwrap();
        assert_eq!(names(&files_of("plans/real.md", &via.to_string_lossy())), ["real.md"]);
    }

    #[test]
    fn only_regular_text_files_are_read() {
        let t = Tree::new("kinds");
        fs::create_dir_all(t.0.join("project/plans/folder.md")).unwrap();
        t.write("project/plans/notes.txt", b"not named md");
        t.write("project/plans/blob.md", b"binary\0data");
        t.write("project/plans/latin1.md", &[b'c', b'a', b'f', 0xE9]);
        t.write("project/plans/ok.MD", b"upper case extension");
        #[cfg(unix)]
        {
            let fifo = std::ffi::CString::new(t.0.join("project/plans/pipe.md").to_string_lossy().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        }
        let files =files_of("plans/folder.md plans/notes.txt plans/pipe.md plans/missing.md plans/blob.md plans/latin1.md plans/ok.MD", &t.project());
        assert_eq!(names(&files), ["blob.md", "latin1.md", "ok.MD"], "a folder, a pipe and a name that is not there are not candidates");
        assert_eq!((files[0].sent, files[0].text.as_str(), files[0].bytes), (false, "", 11), "binary: listed with its size");
        assert!(!files[1].sent, "not UTF-8: listed");
        assert!(files[2].sent);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_swapped_after_the_check_is_not_opened() {
        use std::os::unix::fs::symlink;
        let t = Tree::new("swap");
        let a = t.write("project/plans/a.md", b"a");
        let b = t.write("project/plans/b.md", b"b");
        let checked = fs::symlink_metadata(&a).unwrap();
        assert!(open_same(&a, &checked).is_some());
        assert!(open_same(&b, &checked).is_none(), "another file where the checked one was");
        fs::remove_file(&b).unwrap();
        symlink(&a, &b).unwrap();
        let checked_b = fs::symlink_metadata(&b).unwrap();
        assert!(open_same(&b, &checked_b).is_none(), "a symlink is never followed at the open");
        let fifo = t.0.join("project/plans/p.md");
        assert_eq!(unsafe { libc::mkfifo(std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap().as_ptr(), 0o600) }, 0);
        assert!(open_same(&fifo, &checked).is_none(), "a pipe is neither waited on nor read");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_opened_is_listed_not_sent() {
        use std::os::unix::fs::PermissionsExt;
        let t = Tree::new("unreadable");
        let p = t.write("project/plans/locked.md", b"locked text");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::File::open(&p).is_ok() { return; } // a superuser opens it anyway
        let f =files_of("plans/locked.md", &t.project());
        assert_eq!((names(&f), f[0].sent, f[0].text.as_str(), f[0].bytes), (vec!["locked.md"], false, "", 11));
    }

    #[test]
    fn the_caps_hold_and_a_cut_is_said() {
        let t = Tree::new("caps");
        let line = "word word word word word word word word word\n"; // 45 bytes
        t.write("project/plans/mid.md", line.repeat(300).as_bytes()); // 13 500 bytes: shortened
        t.write("project/plans/huge.md", "x".repeat(MAX_WHOLE as usize + 1).as_bytes()); // listed only
        t.write("project/plans/exact.md", "y".repeat(MAX_PLAN_FILE_TEXT).as_bytes()); // whole
        let files =files_of("plans/mid.md plans/huge.md plans/exact.md", &t.project());
        assert_eq!(names(&files), ["mid.md", "huge.md", "exact.md"]);
        assert!(files[0].sent && files[0].cut && files[0].text.len() <= MAX_PLAN_FILE_TEXT && files[0].text.ends_with("word"), "shortened at a line end");
        assert_eq!((files[0].bytes, files[1].sent, files[1].text.len(), files[1].bytes), (13_500, false, 0, MAX_WHOLE + 1));
        assert!(files[2].sent && !files[2].cut && files[2].text.len() == MAX_PLAN_FILE_TEXT);

        // at most 8 files, and all texts together within the total: the later ones are listed only
        let many: Vec<String> = (0..12).map(|i| { t.write(&format!("project/plans/p{i}.md"), "z".repeat(MAX_PLAN_FILE_TEXT).as_bytes()); format!("plans/p{i}.md") }).collect();
        let files =files_of(&many.join(" "), &t.project());
        assert_eq!(files.len(), MAX_PLAN_FILES);
        assert!(files.iter().map(|f| f.text.len()).sum::<usize>() <= MAX_PLAN_FILES_TEXT);
        assert!(files[..6].iter().all(|f| f.sent && !f.cut) && files[6..].iter().all(|f| !f.sent), "6 x 8 KB fill the 48 KB");
    }

    #[test]
    fn secrets_and_hidden_characters_never_leave_as_they_are() {
        let t = Tree::new("redact");
        let long_line = format!("api_key = sk-ant-{}\n", "a".repeat(40));
        t.write("project/plans/a.md", format!("{long_line}plain \u{202e}evil \u{200b}x\nnormal").as_bytes());
        let f = &files_of("plans/a.md", &t.project())[0];
        assert!(!f.text.contains("sk-ant-") && f.text.contains("[redacted]"), "{}", f.text);
        assert!(f.text.contains("\\u202E") && f.text.contains("\\u200B") && !f.text.contains('\u{202e}') && !f.text.contains('\u{200b}'), "{}", f.text);
        // a hidden character in a name is escaped too, and a name cannot make the message too long
        t.write("project/plans/b\u{202e}.md", b"x");
        let f =files_of("plans/b\u{202e}.md", &t.project());
        assert_eq!(f[0].name, "b\\u202E.md");
        let long = format!("{}.md", "n".repeat(MAX_PLAN_FILE_NAME));
        t.write(&format!("project/plans/{long}"), b"x");
        let f = read(&format!("plans/{long}"), &t.project());
        assert_eq!((f.files.len(), f.omitted), (1, 0), "a name too long to send is listed, not dropped");
        assert!(f.files[0].name.len() <= MAX_PLAN_FILE_NAME && !f.files[0].sent && f.files[0].text.is_empty());
    }

    #[test]
    fn no_folder_or_the_top_of_a_disk_reads_nothing() {
        let t = Tree::new("roots");
        t.write("project/plans/a.md", b"a");
        assert!(files_of("plans/a.md", "").is_empty());
        assert!(files_of("plans/a.md", &t.0.join("missing").to_string_lossy()).is_empty());
        assert!(files_of(&format!("{}", t.0.join("project/plans/a.md").display()), "/").is_empty());
        assert!(files_of("no files are named here", &t.project()).is_empty());
    }

    #[test]
    fn a_file_the_plan_names_is_never_left_out_unseen() {
        let t = Tree::new("omitted");
        t.write("project/plans/a.md", b"a");
        // names that lead nowhere do not count, whatever their number
        let junk: Vec<String> = (0..MAX_TOKENS).map(|i| format!("plans/none{i}.md")).collect();
        let r = read(&format!("{} plans/a.md", junk.join(" ")), &t.project());
        assert_eq!((r.files.len(), r.omitted), (0, 1), "a name past the first 64 was never looked at: it may be a file");
        let r = read("plans/a.md plans/gone.md", &t.project());
        assert_eq!((names(&r.files), r.omitted), (vec!["a.md"], 0));
    }

    // What a name leads to is judged from its text before the system is asked anything: a share, a device path or a mount
    // outside the folder is refused with no call that could open or wait on it.
    #[cfg(unix)]
    #[test]
    fn a_name_is_judged_as_text_before_the_system_is_asked() {
        let root = Path::new("/work/proj");
        let lex = |t: &str| lexical(t, root, Some(Path::new("/work/proj")));
        for outside in ["//host/share/x.md", "\\\\host\\share\\x.md", "\\\\?\\C:\\x.md", "\\\\.\\pipe\\x.md", "//?/C:/x.md", "///x.md", "/net/host/x.md", "/Volumes/share/x.md", "/work/proj-2/x.md", "/work/x.md",
            "../x.md", "plans/../../x.md", "..\\..\\x.md", "/work/proj/../x.md", "/.."] {
            assert_eq!(lex(outside), None, "{outside}");
        }
        if cfg!(windows) { assert_eq!(lex("C:x.md"), None, "a drive without a root"); }
        assert_eq!(lex("plans/./a.md"), Some(PathBuf::from("/work/proj/plans/a.md")));
        assert_eq!(lex("plans/sub/../a.md"), Some(PathBuf::from("/work/proj/plans/a.md")));
        assert_eq!(lex("plans\\a.md"), Some(PathBuf::from("/work/proj/plans/a.md")), "a Windows separator is a separator");
        assert_eq!(lex("/work/proj/plans/a.md"), Some(PathBuf::from("/work/proj/plans/a.md")));
        // the folder as Claude reported it may be another spelling of the canonical one
        assert_eq!(lexical("/var/p/plans/a.md", Path::new("/private/var/p"), Some(Path::new("/var/p"))), Some(PathBuf::from("/private/var/p/plans/a.md")));
        assert_eq!(lexical("/var/p/plans/a.md", Path::new("/private/var/p"), None), None);
        assert_eq!(lexical("/var/p-2/a.md", Path::new("/private/var/p"), Some(Path::new("/var/p"))), None);
    }

    #[test]
    fn the_forms_windows_reads_differently_are_known() {
        // a verbatim root (Windows' canonical form) is compared without its prefix
        assert_eq!(plain(Path::new("\\\\?\\C:\\work")), PathBuf::from("C:\\work"));
        assert_eq!(plain(Path::new("\\\\?\\UNC\\host\\share")), PathBuf::from("\\\\?\\UNC\\host\\share"));
        for dev in ["CON", "con.md", "NUL.md", "Com1.md", "lpt9.md", "a.md:stream"] { assert!(odd_on_windows(dev), "{dev}"); }
        for ok in ["a.md", "console.md", "COM0.md", "COM10.md", "plan.md"] { assert!(!odd_on_windows(ok), "{ok}"); }
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_name_that_is_a_share_a_device_or_a_drive_without_a_root_is_refused() {
        let root = Path::new(r"\\?\C:\work\proj");
        let lex = |t: &str| lexical(t, root, Some(Path::new(r"C:\work\proj")));
        for bad in [r"\\host\share\x.md", "//host/share/x.md", r"\\?\C:\x.md", r"\\.\pipe\x.md", "C:x.md", "/x.md", r"C:\other\x.md", r"plans\CON.md", r"plans\a.md:s", r"..\x.md"] {
            assert_eq!(lex(bad), None, "{bad}");
        }
        assert_eq!(lex(r"C:\work\proj\plans\a.md"), Some(root.join(r"plans\a.md")));
        assert_eq!(lex(r"plans\a.md"), Some(root.join(r"plans\a.md")));
    }

    // `..` is folded in the text, not by the system after a symlink on the way: a link inside the folder does not make a `..`
    // lead somewhere the text does not, and a link kept outside the folder is not reachable by naming it.
    #[cfg(unix)]
    #[test]
    fn dot_dot_is_folded_in_the_text_before_any_symlink_is_followed() {
        use std::os::unix::fs::symlink;
        let t = Tree::new("dotdot");
        let inside = t.write("project/plans/a.md", b"inside text");
        fs::create_dir_all(t.0.join("project/deep/sub")).unwrap();
        t.write("project/deep/b.md", b"deep text");
        symlink(t.0.join("project/deep/sub"), t.0.join("project/plans/hop")).unwrap();
        symlink(&inside, t.0.join("outside/link.md")).unwrap();
        let project = t.project();
        // by the system, hop/../b.md is deep/b.md; as text it is plans/b.md, which is not there
        assert!(files_of("plans/hop/../b.md", &project).is_empty());
        assert_eq!(names(&files_of("plans/hop/../a.md", &project)), ["a.md"], "as text it is plans/a.md");
        // a link outside the folder that leads back in is not a name inside it
        assert!(files_of("../outside/link.md", &project).is_empty());
        assert!(files_of(&t.0.join("outside/link.md").to_string_lossy(), &project).is_empty());
    }
}
