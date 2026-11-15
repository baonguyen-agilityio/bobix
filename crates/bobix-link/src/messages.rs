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

/// True for control characters and for format characters that can hide or reorder text (bidirectional controls,
/// zero-width characters, tags, the byte order mark).
pub fn is_odd_char(c: char) -> bool {
    c.is_control()
        || matches!(c as u32,
            0x00AD | 0x061C | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F
            | 0xFEFF | 0xFFF9..=0xFFFB | 0xE0000..=0xE007F)
}

/// `text` with every odd character replaced by a visible escape (`\u{202e}`), and whether any was found.
/// Newline and tab stay: a command or a plan has lines.
pub fn visible_escapes(text: &str) -> (String, bool) {
    let mut odd = false;
    let out = text
        .chars()
        .map(|c| {
            if is_odd_char(c) && c != '\n' && c != '\t' {
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
    let mut f = Fields::new("bobix-shown-v1").field(text.as_bytes()).num(cut as u64).num(questions.len() as u64);
    for q in questions {
        f = f.field(q.text.as_bytes()).num(q.multi_select as u64).num(q.options.len() as u64);
        for o in &q.options {
            f = f.field(o.as_bytes());
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
    s.len() <= max && !s.chars().any(|c| is_odd_char(c) && c != '\n')
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
    Error { code: String, who: Option<Who> },
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
}

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
    /// See `shown_hash`: covers `text`, `cut` and `questions`.
    pub shown: String,
    pub odd: bool,
    /// Anything was dropped from `text`: a remote Allow is refused.
    pub cut: bool,
    /// For a question: the questions with their options, in the order `question { answers }` answers them.
    #[serde(default)]
    pub questions: Vec<Question>,
}

impl Detail {
    /// Builds the reply for an item the desktop holds: escapes odd characters, sets `odd`, and computes `shown` over
    /// the text as displayed, `cut` and `questions`.
    pub fn build(id: &str, what: DetailKind, raw_text: &str, cut: bool, questions: Vec<Question>) -> Detail {
        let (text, odd) = visible_escapes(raw_text);
        Detail { id: id.into(), what, shown: shown_hash(&text, cut, &questions), text, odd, cut, questions }
    }

    /// Recomputes `shown` from the content (not from the stored field).
    pub fn shown_now(&self) -> String {
        shown_hash(&self.text, self.cut, &self.questions)
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
    pub prompt: String,
    /// See `prompt_hash`; `start` sends it back.
    pub prompt_hash: String,
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
            DesktopMsg::Detail(d) => within(
                "detail",
                &d.text,
                if d.what == DetailKind::Plan { MAX_PLAN } else { MAX_TOOL_INPUT },
            ),
            DesktopMsg::StartPrompt(p) => within("prompt", &p.prompt, MAX_PLAN),
            DesktopMsg::Notice(n) => within("notice", &n.body, MAX_PUSH_BODY),
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
