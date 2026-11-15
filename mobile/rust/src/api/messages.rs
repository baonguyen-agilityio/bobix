//! The protocol's messages for the phone app: parsing what the computer sends, building what the phone sends, the
//! `hello` of pairing, and the hashes. Everything forwards to `bobix-link`, which owns the shapes, the limits and the
//! bytes, so Dart never re-implements the protocol: it passes plain values in and gets bytes or JSON out.
//! Commands are built in two steps because the signature is made in secure hardware: `prepare_command` gives the bytes to
//! sign, the hardware signs them, `finish_command` puts the signature in.

use super::core::{CoreError, LinkStream};
use bobix_link::command;
use bobix_link::envelope::{self, Direction, Lane, VersionRange, CURRENT_VERSION};
use bobix_link::keys::{Identity, SignPublic};
use bobix_link::messages::{self as m, Choice, CommandBody, Decision, DesktopMsg, Mode, PhoneMsg};
use bobix_link::pairing::{self, Hello, PairingCode, Rights};
use crypto_box::{PublicKey, SecretKey};

fn key32(bytes: &[u8]) -> Result<[u8; 32], CoreError> {
    bytes.try_into().map_err(|_| CoreError::InvalidKey)
}

// ---- hashes and text rules ----

/// The hash a `start` carries: SHA-256 of the whole prompt the person was shown, in hex.
pub fn prompt_hash(prompt: String) -> String {
    m::prompt_hash(&prompt)
}

/// Whether a name or note is plain (no control or format characters that could hide or reorder text) and within `max` bytes.
pub fn is_plain_text(text: String, max: usize) -> bool {
    m::is_plain_text(&text, max)
}

// ---- pairing ----

/// What this phone asks for (the computer may grant fewer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhoneRights {
    pub answer: bool,
    pub plan: bool,
    pub question: bool,
    pub start: bool,
    pub stop: bool,
    pub activity: bool,
}

impl From<PhoneRights> for Rights {
    fn from(r: PhoneRights) -> Self {
        Rights { answer: r.answer, plan: r.plan, question: r.question, start: r.start, stop: r.stop, activity: r.activity }
    }
}

fn sign_public(bytes: &[u8]) -> Result<SignPublic, CoreError> {
    SignPublic::from_sec1(bytes).map_err(|_| CoreError::InvalidKey)
}

/// The bytes the `hello`'s MAC covers (exposed so the committed vector can be checked).
pub fn hello_mac_input(
    pairing_uri: String,
    phone_auth_public: Vec<u8>,
    phone_box_public: Vec<u8>,
    sign_public_key: Vec<u8>,
    name: String,
    rights: PhoneRights,
) -> Result<Vec<u8>, CoreError> {
    let code = PairingCode::parse(&pairing_uri)?;
    let phone = bobix_link::keys::PublicKeys { auth: key32(&phone_auth_public)?, boxk: key32(&phone_box_public)? };
    let sign = sign_public(&sign_public_key)?.to_sec1();
    Ok(pairing::hello_mac_input(&code.desktop, &phone, &sign, &name, rights.into()))
}

/// The body of the `hello` the phone sends after scanning a code: its box public key, then the `hello` sealed to the
/// computer. `sign_public_key` is the secure hardware's public key (any SEC1 form). The name must be plain text.
#[allow(clippy::too_many_arguments)]
pub fn build_hello_body(
    pairing_uri: String,
    auth_seed: Vec<u8>,
    box_secret: Vec<u8>,
    sign_public_key: Vec<u8>,
    name: String,
    rights: PhoneRights,
    version_min: u8,
    version_max: u8,
    counter: u64,
) -> Result<Vec<u8>, CoreError> {
    let code = PairingCode::parse(&pairing_uri)?;
    if name.is_empty() || !m::is_plain_text(&name, m::MAX_NAME) || name.contains('\n') {
        return Err(CoreError::Hello);
    }
    let box_secret = key32(&box_secret)?;
    let phone = Identity::from_secrets(key32(&auth_seed)?, box_secret).public();
    let hello = Hello::new(&code, &phone, &sign_public(&sign_public_key)?, &name, rights.into(), VersionRange { min: version_min, max: version_max });
    let lane = Lane { dir: Direction::ToDesktop, stream: envelope::Stream::Event };
    let packet = envelope::seal(
        CURRENT_VERSION,
        lane,
        counter,
        &SecretKey::from(box_secret),
        &PublicKey::from(code.desktop.boxk),
        &PhoneMsg::Hello(hello).to_json(),
    );
    Ok(pairing::hello_body(&phone.boxk, &packet))
}

// ---- what the computer sends ----

/// A message from the computer, checked by the shared crate (shape and limits). `kind` is its `kind` (`home`,
/// `detail`, `startPrompt`, `result`, `paired`, `revoked`, `relayMoved`, `notice`, `error`); `json` is the message as the
/// crate serialises it. Anything that does not parse, or breaks a limit, is an error.
pub struct DesktopMessage {
    pub kind: String,
    pub json: String,
    /// The stream it travels on: `home` on `state`, everything else on `event`.
    pub stream: LinkStream,
}

pub fn parse_desktop_message(bytes: Vec<u8>) -> Result<DesktopMessage, CoreError> {
    let msg = DesktopMsg::from_json(&bytes).map_err(|_| CoreError::Malformed)?;
    msg.validate().map_err(|_| CoreError::Limit)?;
    let json = serde_json::to_string(&msg).map_err(|_| CoreError::Malformed)?;
    let kind = serde_json::from_str::<serde_json::Value>(&json)
        .ok()
        .and_then(|v| v["kind"].as_str().map(str::to_string))
        .ok_or(CoreError::Malformed)?;
    let stream = if matches!(msg, DesktopMsg::Home(_)) { LinkStream::State } else { LinkStream::Event };
    Ok(DesktopMessage { kind, json, stream })
}

// ---- what the phone sends ----

fn phone_json(msg: PhoneMsg) -> Result<Vec<u8>, CoreError> {
    msg.validate().map_err(|_| CoreError::Limit)?;
    Ok(msg.to_json())
}

/// Ends pairing from the phone's side: sent when the computer's `paired` arrived.
pub fn paired_ack_message() -> Vec<u8> {
    PhoneMsg::PairedAck.to_json()
}

/// On connect: the commands the phone still waits for, and the versions it understands.
pub fn sync_message(waiting: Vec<String>, version_min: u8, version_max: u8) -> Result<Vec<u8>, CoreError> {
    phone_json(PhoneMsg::Sync { waiting, versions: VersionRange { min: version_min, max: version_max } })
}

/// Asks for the whole content of a request, plan or question.
pub fn detail_request_message(id: String) -> Vec<u8> {
    PhoneMsg::Detail { id }.to_json()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMode {
    Full,
    Quick,
}

impl From<StartMode> for Mode {
    fn from(s: StartMode) -> Self {
        match s {
            StartMode::Full => Mode::Full,
            StartMode::Quick => Mode::Quick,
        }
    }
}

/// Asks the computer for the prompt it would start a ticket with, to show it before the person confirms.
pub fn start_preview_message(project: String, iid: String, mode: StartMode) -> Vec<u8> {
    PhoneMsg::StartPreview { project, iid, mode: mode.into() }.to_json()
}

/// Goodbye to one computer: `signature` is the `auth` key's signature over `bye_bytes`.
pub fn bye_message(signature: Vec<u8>) -> Vec<u8> {
    PhoneMsg::Bye { sig: bobix_link::b64(&signature) }.to_json()
}

// ---- commands ----

/// What a phone asks for with a signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandInput {
    Answer { id: String, allow: bool, shown: String },
    Plan { id: String, approve: bool, note: Option<String>, shown: String },
    Question { id: String, answers: Vec<String>, shown: String },
    Start { project: String, iid: String, mode: StartMode, prompt_hash: String },
    Stop { session: String, request: String },
}

impl From<CommandInput> for CommandBody {
    fn from(c: CommandInput) -> Self {
        match c {
            CommandInput::Answer { id, allow, shown } => CommandBody::Answer { id, choice: if allow { Choice::Allow } else { Choice::Deny }, shown },
            CommandInput::Plan { id, approve, note, shown } => {
                CommandBody::Plan { id, decision: if approve { Decision::Approve } else { Decision::Back }, note, shown }
            }
            CommandInput::Question { id, answers, shown } => CommandBody::Question { id, answers, shown },
            CommandInput::Start { project, iid, mode, prompt_hash } => CommandBody::Start { project, iid, mode: mode.into(), prompt_hash },
            CommandInput::Stop { session, request } => CommandBody::Stop { session, request },
        }
    }
}

/// The first step: a fresh command id and the bytes the secure hardware signs.
pub struct PreparedCommand {
    pub cid: String,
    pub kind: String,
    pub target: String,
    pub bytes_to_sign: Vec<u8>,
}

fn limit(body: &CommandBody) -> Result<(), CoreError> {
    body.validate().map_err(|_| CoreError::Limit)
}

/// `counter` is the packet counter the command will travel with; `issued_at` the phone's clock in Unix ms.
pub fn prepare_command(input: CommandInput, phone_id: String, computer_id: String, counter: u64, issued_at: u64) -> Result<PreparedCommand, CoreError> {
    let body: CommandBody = input.into();
    limit(&body)?;
    let cid = command::new_cid();
    let text = serde_json::to_string(&body).map_err(|_| CoreError::Malformed)?;
    let bytes_to_sign = command::command_bytes(CURRENT_VERSION, &phone_id, &computer_id, body.kind(), &body.target(), &cid, counter, issued_at, &text);
    Ok(PreparedCommand { cid, kind: body.kind().to_string(), target: body.target(), bytes_to_sign })
}

/// The second step: the `cmd` message, with the signature (DER) the hardware made over `prepare_command`'s bytes. Give it
/// the same inputs and the same `cid`.
pub fn finish_command(
    input: CommandInput,
    cid: String,
    phone_id: String,
    computer_id: String,
    counter: u64,
    issued_at: u64,
    signature_der: Vec<u8>,
) -> Result<Vec<u8>, CoreError> {
    let body: CommandBody = input.into();
    limit(&body)?;
    let cmd = command::build_with_cid(cid, CURRENT_VERSION, &phone_id, &computer_id, counter, issued_at, &body, |_| signature_der);
    phone_json(PhoneMsg::Cmd(cmd))
}

/// Whether the granted rights allow this command (Stop needs the Answer right too).
pub fn rights_allow(rights: PhoneRights, input: CommandInput) -> bool {
    Rights::from(rights).allows(&input.into())
}
