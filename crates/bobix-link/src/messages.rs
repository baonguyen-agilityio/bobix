//! The messages sealed inside packets. JSON, camelCase, tagged by `kind`.
//!
//! Spike results that shaped the format are noted where they apply; guesses nobody has confirmed are marked
//! `// assumption: ...`.

use crate::envelope::VersionRange;
use crate::pairing::Rights;
use crate::{hex, sha256, Fields};
use serde::{Deserialize, Serialize};

// Limits, all in UTF-8 bytes.
pub const MAX_FRAME: usize = 256 * 1024;
pub const MAX_TOOL_INPUT: usize = 20_000;
pub const MAX_PLAN: usize = 20_000;
pub const MAX_PREVIEW: usize = 300;
/// A push body above this (after encoding) is replaced by a short sealed "open the app" notice.
pub const MAX_PUSH_BODY: usize = 3_500;
pub const MAX_NAME: usize = 64;
/// A note on a plan and each answer to a question.
pub const MAX_NOTE: usize = 2_000;
/// The issue list (`issues`): repositories, issues over all of them, and the longest title, repository path and host.
/// Far below `MAX_FRAME`: 100 issues of 120 bytes and 40 repositories of 300 bytes are about 30 KB.
pub const MAX_ISSUE_REPOS: usize = 40;
pub const MAX_ISSUES: usize = 100;
pub const MAX_ISSUE_TITLE: usize = 120;
pub const MAX_ISSUE_PATH: usize = 200;
pub const MAX_ISSUE_HOST: usize = 100;
/// The files a plan comes with (`Detail.files`): how many, the longest text of one, all texts together, and the longest
/// name and folder path. Together with a plan's `MAX_PLAN` they stay far below `MAX_FRAME` even when JSON doubles them.
pub const MAX_PLAN_FILES: usize = 8;
pub const MAX_PLAN_FILE_TEXT: usize = 8 * 1024;
pub const MAX_PLAN_FILES_TEXT: usize = 48 * 1024;
pub const MAX_PLAN_FILE_NAME: usize = 200;
pub const MAX_PLAN_FILE_PATH: usize = 400;

/// A command is accepted from this long before to this long after the receiver's clock.
pub const COMMAND_MAX_AGE_MS: u64 = 10 * 60 * 1000;
pub const COMMAND_MAX_AHEAD_MS: u64 = 2 * 60 * 1000;

#[derive(Debug, PartialEq, Eq)]
pub enum LimitError {
    /// Which field, and the limit in bytes.
    TooLong(&'static str, usize),
}

fn within(field: &'static str, s: &str, max: usize) -> Result<(), LimitError> {
    if s.len() > max {
        Err(LimitError::TooLong(field, max))
    } else {
        Ok(())
    }
}

/// True for control characters, for format characters that can hide or reorder text (bidirectional controls, zero-width
/// characters, tags, the byte order mark, the other `Cf` characters), and for the characters that are not format characters
/// but draw nothing or change nothing visible: the combining grapheme joiner, the Hangul fillers (letters that render
/// blank, usable as an invisible name) and the variation selectors. The one list: the desktop's escaper and the previews
/// use it too, so a request, a plan and a name are judged by the same rule. It judges one character alone, so it leaves out
/// the two presentation selectors, which depend on what precedes them: use [`odd_flags`] on text.
pub fn is_odd_char(c: char) -> bool {
    c.is_control()
        || matches!(c as u32,
            0x00AD | 0x034F | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x08E2 | 0x115F | 0x1160
            | 0x180B..=0x180F | 0x200B..=0x200F | 0x2028..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F
            | 0x3164 | 0xFE00..=0xFE0D | 0xFEFF | 0xFFA0 | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0000..=0xE007F | 0xE0100..=0xE01EF)
}

/// U+FE0E and U+FE0F, the text and emoji presentation selectors (`⚠️`, `✅`): not odd by themselves, see [`odd_flags`].
pub fn is_presentation_selector(c: char) -> bool {
    matches!(c as u32, 0xFE0E | 0xFE0F)
}

/// For each character of `text`, whether it is hidden. The same as [`is_odd_char`], except that a presentation selector
/// (U+FE0E, U+FE0F) is allowed directly after a visible character, which is how emoji are written, and is hidden when it
/// stands first, follows white space, a control or another hidden character, or is stacked on another selector: there it
/// can only carry something, not select how a character looks.
pub fn odd_flags(text: &str) -> Vec<bool> {
    let mut prev_visible = false;
    text.chars()
        .map(|c| {
            let odd = if is_presentation_selector(c) { !prev_visible } else { is_odd_char(c) };
            prev_visible = !odd && !c.is_whitespace() && !is_presentation_selector(c);
            odd
        })
        .collect()
}

/// Whether `text` holds a hidden character (see [`odd_flags`]); newline and tab count, as they do for [`is_odd_char`].
pub fn has_odd(text: &str) -> bool {
    odd_flags(text).contains(&true)
}

/// `text` with every odd character replaced by a visible escape (`\u{202e}`), and whether any was found.
/// Newline and tab stay: a command or a plan has lines.
pub fn visible_escapes(text: &str) -> (String, bool) {
    let mut odd = false;
    let out = text
        .chars()
        .zip(odd_flags(text))
        .map(|(c, hidden)| {
            if hidden && c != '\n' && c != '\t' {
                odd = true;
                format!("\\u{{{:x}}}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect();
    (out, odd)
}

/// The `shown` hash: SHA-256, in hex, of a canonical encoding of everything the phone renders for an item: the text
/// as displayed (with visible escapes), whether it was cut, and the questions with their options in order. `detail`
/// carries it; `answer`, `plan` and `question` send it back; the desktop recomputes it from what it holds.
pub fn shown_hash(text: &str, cut: bool, questions: &[Question]) -> String {
    shown_hash_with_files(text, cut, questions, &[])
}

/// [`shown_hash`] for a plan that comes with files: the files, in order, with everything the phone shows of each (name,
/// folder, size, text, `cut`, `sent`), are appended after the questions. With no files the bytes are exactly those of
/// `shown_hash` before files existed, so an older desktop and an older phone still agree.
pub fn shown_hash_with_files(text: &str, cut: bool, questions: &[Question], files: &[PlanFile]) -> String {
    let mut f = Fields::new("bobix-shown-v1").field(text.as_bytes()).num(cut as u64).num(questions.len() as u64);
    for q in questions {
        f = f.field(q.text.as_bytes()).num(q.multi_select as u64).num(q.options.len() as u64);
        for o in &q.options {
            f = f.field(o.as_bytes());
        }
    }
    if !files.is_empty() {
        f = f.field(b"files").num(files.len() as u64);
        for p in files {
            f = f.field(p.name.as_bytes()).field(p.path.as_bytes()).num(p.bytes).field(p.text.as_bytes()).num(p.cut as u64).num(p.sent as u64);
        }
    }
    hex(&sha256(&[&f.done()]))
}

/// The hash a `start` carries: SHA-256 of the whole prompt, in hex.
pub fn prompt_hash(prompt: &str) -> String {
    hex(&sha256(&[prompt.as_bytes()]))
}

/// Text a phone sends (a name, a note) must be plain: no control or format characters.
pub fn is_plain_text(s: &str, max: usize) -> bool {
    s.len() <= max && !s.chars().zip(odd_flags(s)).any(|(c, odd)| odd && c != '\n')
}

// ---------------------------------------------------------------------------------------------------------------
// Desktop → phone

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DesktopMsg {
    /// Travels on the `state` stream. Every other desktop message travels on `event`.
    Home(Home),
    Detail(Detail),
    StartPrompt(StartPrompt),
    Result(CmdResult),
    Paired { computer: String, rights: Rights },
    Revoked { why: String },
    /// Sent through the old relay before the desktop switches to another one.
    RelayMoved { url: String },
    Notice(Notice),
    /// The reply to `issues`: the repositories ticked on the computer with their open issues.
    Issues(IssueList),
    Error { code: String, who: Option<Who> },
}

/// The issues a phone with the Start right could start, grouped by the repositories ticked on the computer (a repository
/// with no open issue is listed too). Titles are redacted and cut by the computer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueList {
    pub repos: Vec<IssueRepo>,
    /// Why the list may be incomplete: the computer could not read the issues (`gh` / `glab` missing, signed out or
    /// offline), or has not finished its first read. Absent when the list is whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<IssuesError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssuesError {
    /// The last read failed.
    Read,
    /// The first read has not finished.
    Reading,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRepo {
    /// The repository's path on its host (`owner/name`), the `project` of a `startPreview` and a `start`.
    pub path: String,
    pub host: String,
    /// A clone of it is on the computer; without one an issue cannot be started.
    pub cloned: bool,
    #[serde(default)]
    pub issues: Vec<IssueInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueInfo {
    pub iid: u64,
    pub title: String,
    /// When the tracker says it was last changed (as the tracker writes it, ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    /// A session on the computer already works on it: the computer refuses a start (`exists`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_session: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Who {
    Relay,
    Desktop,
    Phone,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Home {
    /// Unix milliseconds on the sender's clock when it sent this.
    pub sent_at: u64,
    pub computer: Computer,
    pub sessions: Vec<SessionInfo>,
    pub waiting: Vec<Waiting>,
    pub groups: Vec<Group>,
    /// What this phone may do right now. The desktop sets it in every `home` so a change made on the computer reaches the
    /// phone; absent (an older desktop) means keep the rights the phone already has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rights: Option<Rights>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComputerState {
    Online,
    /// "asleep, N sessions unfinished" (a closed lid is asleep too).
    Asleep,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Computer {
    pub name: String,
    pub state: ComputerState,
    pub unfinished: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pet {
    pub design: String,
    pub color: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub project: String,
    /// A short label the desktop chooses (working, waiting, idle, closed...).
    pub state: String,
    pub pet: Pet,
    /// The desktop sets it: Stop is possible for this session. Stop exists only for a session that waits on a held
    /// request, and the phone shows its button from this flag. An older desktop omits it, which reads as `false`.
    #[serde(default)]
    pub can_stop: bool,
    /// How many lines of activity `detail` would return for this session (at most 40). The desktop sets it only for a
    /// phone that may see activity, so the phone can show the count without asking; absent reads as 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lines: u8,
    /// For a session that waits with nothing held for phones (`state` is `atComputer`): why, when the desktop knows.
    /// Absent from an older desktop and whenever the reason is unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_computer_why: Option<AtComputerWhy>,
}

/// Why a waiting session is not held for phones: Claude asks in its own prompt, so it is answered at the computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AtComputerWhy {
    /// Someone is using the computer.
    Present,
    /// The question could not be held whole (too many questions or options, text too long, or cut by the hook).
    TooLarge,
    /// A kind of prompt Bobix does not hold.
    Unsupported,
    /// Bobix is set to show or hold nothing on this computer for now.
    NotShown,
}

fn is_zero(n: &u8) -> bool { *n == 0 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WaitingKind {
    Request,
    Plan,
    Question,
}

/// Something that waits for an answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Waiting {
    pub id: String,
    pub session: String,
    pub what: WaitingKind,
    /// At most 300 bytes, passed through the redactor by the desktop.
    pub preview: String,
    /// The input was cut or too long to show whole: the desktop refuses a remote Allow.
    pub cut: bool,
    pub risky: bool,
    /// Released (see `Outcome::Released`): someone is at the computer. Still answerable; the first answer wins.
    #[serde(default)]
    pub at_computer: bool,
    /// "Seconds left", counted by the receiver from its own receive time. None when the desktop waits forever.
    pub seconds_left: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupKind {
    NeedsYou,
    InProgress,
    WaitingForReview,
    Stuck,
    ToDo,
    Done,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub group: GroupKind,
    /// Ids of `sessions` or `waiting` entries, in display order.
    pub ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DetailKind {
    ToolInput,
    Plan,
    Question,
    LastLines,
}

/// The reply to a `detail` request: the whole thing, never cut silently.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub id: String,
    pub what: DetailKind,
    /// With `odd`, every control or format character is already a visible escape.
    pub text: String,
    /// See `shown_hash`: covers `text`, `cut`, `questions` and `files`.
    pub shown: String,
    pub odd: bool,
    /// Anything was dropped from `text`: a remote Allow is refused.
    pub cut: bool,
    /// For a question: the questions with their options, in the order `question { answers }` answers them.
    #[serde(default)]
    pub questions: Vec<Question>,
    /// For `lastLines`: when each line of `text` happened (Unix ms), one per line in order. Absent from an older desktop,
    /// and empty when unknown; a phone uses it only when it counts the lines. Not in `shown_hash`: nothing is approved on it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub times: Vec<u64>,
    /// For a plan: the Markdown files the plan names inside the session's folder, read when the plan was held. Absent from
    /// an older desktop and when the plan names none. In `shown_hash`: approving a plan covers exactly these files as shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<PlanFile>,
}

/// One file of a plan. `text` is what the computer chose to send: redacted, with hidden characters as visible escapes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanFile {
    /// The file's name (its last path component).
    pub name: String,
    /// Where it is, relative to the session's folder: the folder and the name, with `/` as the separator.
    pub path: String,
    /// The file's size on the computer, in bytes (not of `text`).
    pub bytes: u64,
    pub text: String,
    /// `text` is the start of the file only: the rest was left out. A remote Approve is refused.
    #[serde(default)]
    pub cut: bool,
    /// `text` was sent. When false the file is only listed (too large, unreadable, not text): a remote Approve is refused.
    pub sent: bool,
}

impl Detail {
    /// Builds the reply for an item the desktop holds: escapes odd characters, sets `odd`, and computes `shown` over
    /// the text as displayed, `cut` and `questions`.
    pub fn build(id: &str, what: DetailKind, raw_text: &str, cut: bool, questions: Vec<Question>) -> Detail {
        let (text, odd) = visible_escapes(raw_text);
        Detail { id: id.into(), what, shown: shown_hash(&text, cut, &questions), text, odd, cut, questions, times: vec![], files: vec![] }
    }

    /// The same reply for a plan that comes with `files`: `shown` now covers them.
    pub fn with_files(mut self, files: Vec<PlanFile>) -> Detail {
        self.files = files;
        self.shown = self.shown_now();
        self
    }

    /// Recomputes `shown` from the content (not from the stored field).
    pub fn shown_now(&self) -> String {
        shown_hash_with_files(&self.text, self.cut, &self.questions, &self.files)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub text: String,
    pub options: Vec<String>,
    pub multi_select: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Full,
    Quick,
}

/// The reply to `startPreview`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPrompt {
    pub project: String,
    pub iid: String,
    pub mode: Mode,
    /// The prompt as the person must read it: every hidden character written as a visible escape (the text that runs is
    /// this one, hidden characters and all).
    pub prompt: String,
    /// See `prompt_hash`; `start` sends it back. Over [prompt] as shown.
    pub prompt_hash: String,
    /// The prompt held a hidden character (the phone says so, next to the escapes).
    #[serde(default)]
    pub odd: bool,
    pub folder_known: bool,
    /// Free disk space on that repository's volume, in bytes.
    pub free_disk: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Ok,
    LatePc,
    LatePhone,
    Expired,
    /// Someone is at the computer: the request was released from the phones. The phone gets no more pushes for it and
    /// shows it as "being handled at the computer". It is NOT withdrawn: the phone can still answer, the first answer
    /// wins, and an answer to a released request is applied (`ok`), not `stale`.
    Released,
    Stale,
    Refused,
    /// The desktop never received that command; it may still arrive within 10 minutes, or be withdrawn.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmdResult {
    pub cid: String,
    pub outcome: Outcome,
    /// Who answered first, for `latePc` and `latePhone`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// For `refused`: `clock`, `computer`, `signature`, `encoding`, `cut`, `right`, and the like. For a refused `start`:
    /// `folder` (the repository's folder is missing) or `disk` (not enough free space).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

impl CmdResult {
    pub fn new(cid: &str, outcome: Outcome) -> Self {
        CmdResult { cid: cid.into(), outcome, by: None, why: None }
    }
    pub fn refused(cid: &str, why: &str) -> Self {
        CmdResult { why: Some(why.into()), ..Self::new(cid, Outcome::Refused) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    Waiting,
    Closed,
    Done,
}

/// A notice. Its sub-kind is `what`: `kind` is the tag of the message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub what: NoticeKind,
    #[serde(rename = "ref")]
    pub reference: String,
    pub session: String,
    pub title: String,
    pub body: String,
    /// For `closed`: who closed the item. Absent from an older desktop and when the desktop does not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<ClosedBy>,
}

/// Who closed a held item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClosedBy {
    /// Answered at the computer (a page, a pet, or Claude's own prompt).
    Computer,
    /// Answered from the phone with this id.
    Phone { phone: String },
    /// Nobody answered in time, or Claude moved on.
    Expired,
    /// Handed back to Claude's own prompt.
    Released,
}

impl DesktopMsg {
    pub fn validate(&self) -> Result<(), LimitError> {
        match self {
            DesktopMsg::Home(h) => {
                for w in &h.waiting {
                    within("preview", &w.preview, MAX_PREVIEW)?;
                }
                Ok(())
            }
            DesktopMsg::Detail(d) => {
                within("detail", &d.text, if d.what == DetailKind::Plan { MAX_PLAN } else { MAX_TOOL_INPUT })?;
                if d.files.len() > MAX_PLAN_FILES { return Err(LimitError::TooLong("files", MAX_PLAN_FILES)); }
                if d.files.iter().map(|f| f.text.len()).sum::<usize>() > MAX_PLAN_FILES_TEXT { return Err(LimitError::TooLong("files", MAX_PLAN_FILES_TEXT)); }
                for f in &d.files {
                    within("file", &f.text, MAX_PLAN_FILE_TEXT)?;
                    within("fileName", &f.name, MAX_PLAN_FILE_NAME)?;
                    within("filePath", &f.path, MAX_PLAN_FILE_PATH)?;
                }
                Ok(())
            }
            DesktopMsg::StartPrompt(p) => within("prompt", &p.prompt, MAX_PLAN),
            DesktopMsg::Notice(n) => within("notice", &n.body, MAX_PUSH_BODY),
            DesktopMsg::Issues(l) => {
                if l.repos.len() > MAX_ISSUE_REPOS { return Err(LimitError::TooLong("repos", MAX_ISSUE_REPOS)); }
                if l.repos.iter().map(|r| r.issues.len()).sum::<usize>() > MAX_ISSUES { return Err(LimitError::TooLong("issues", MAX_ISSUES)); }
                for r in &l.repos {
                    within("path", &r.path, MAX_ISSUE_PATH)?;
                    within("host", &r.host, MAX_ISSUE_HOST)?;
                    for i in &r.issues { within("title", &i.title, MAX_ISSUE_TITLE)?; }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a message always serializes")
    }

    pub fn from_json(b: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(b)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Phone → desktop

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PhoneMsg {
    Hello(crate::pairing::Hello),
    PairedAck,
    /// The commands the phone still waits for; the desktop answers each with its `result`.
    Sync { waiting: Vec<String>, versions: VersionRange },
    Detail { id: String },
    StartPreview { project: String, iid: String, mode: Mode },
    /// Asks for the issues the phone could start; the desktop answers with `issues` (or `error` `right`).
    Issues,
    /// A signed command: answer, plan, question, start or stop.
    Cmd(crate::command::Command),
    /// Signed with the `auth` key (see `command::bye_bytes`).
    Bye { sig: String },
    Error { code: String, who: Option<Who> },
}

impl PhoneMsg {
    pub fn validate(&self) -> Result<(), LimitError> {
        if let PhoneMsg::Sync { waiting, .. } = self {
            if waiting.len() > 1_000 {
                return Err(LimitError::TooLong("sync", 1_000));
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a message always serializes")
    }

    pub fn from_json(b: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(b)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Choice {
    Allow,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Approve,
    Back,
}

/// What a phone asks for with a signature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CommandBody {
    Answer { id: String, choice: Choice, shown: String },
    Plan { id: String, decision: Decision, note: Option<String>, shown: String },
    /// One string per question, in the order of the `detail`'s `questions`: an option label, several labels joined
    /// by ", " for a multi-select, or free text. The desktop turns it into the hook's `answers` object keyed by
    /// question text (confirmed by the hook spike).
    Question { id: String, answers: Vec<String>, shown: String },
    Start { project: String, iid: String, mode: Mode, prompt_hash: String },
    /// Stops a session that waits on a held request: the desktop denies that request with `interrupt` (the only
    /// clean stop the CLI offers; stopping a running turn is not part of v1). `request` is the held request's id.
    Stop { session: String, request: String },
}

impl CommandBody {
    pub fn kind(&self) -> &'static str {
        match self {
            CommandBody::Answer { .. } => "answer",
            CommandBody::Plan { .. } => "plan",
            CommandBody::Question { .. } => "question",
            CommandBody::Start { .. } => "start",
            CommandBody::Stop { .. } => "stop",
        }
    }

    /// What the command acts on: the open item, the ticket, or the session.
    pub fn target(&self) -> String {
        match self {
            CommandBody::Answer { id, .. } | CommandBody::Plan { id, .. } | CommandBody::Question { id, .. } => id.clone(),
            CommandBody::Start { project, iid, .. } => format!("{project}#{iid}"),
            CommandBody::Stop { session, .. } => session.clone(),
        }
    }

    /// The `shown` hash this command sends back, for the kinds that carry one.
    pub fn shown(&self) -> Option<&str> {
        match self {
            CommandBody::Answer { shown, .. } | CommandBody::Plan { shown, .. } | CommandBody::Question { shown, .. } => {
                Some(shown)
            }
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<(), LimitError> {
        match self {
            CommandBody::Plan { note: Some(n), .. } => within("note", n, MAX_NOTE),
            CommandBody::Question { answers, .. } => answers.iter().try_for_each(|a| within("answer", a, MAX_NOTE)),
            _ => Ok(()),
        }
    }
}
