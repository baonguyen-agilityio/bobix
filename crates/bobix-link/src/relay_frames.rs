//! Frames between a device and the relay: JSON text, tagged by `type`. The relay never parses a `body`: it is an
//! opaque string (a sealed packet, URL-safe base64). Push is part of the frames from the start.

use crate::envelope::VersionRange;
use crate::messages::{Who, MAX_FRAME};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    Oversize,
    Malformed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PushKind {
    Alert,
    Background,
    LiveActivity,
}

/// What a `liveActivity` push does to the phone's Live Activity. `start` begins one (the relay uses the phone's
/// push-to-start token, or updates a running activity when it has that token instead), `update` changes a running one,
/// `end` closes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityEvent {
    Start,
    Update,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Push {
    pub kind: PushKind,
    pub priority: u8,
    /// Only for `liveActivity`; absent reads as `update`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<ActivityEvent>,
    /// An opaque id the relay passes on as the collapse id, so a later notice can take the place of an earlier one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresenceState {
    Online,
    Offline,
    Asleep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Ios,
    Android,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApnsEnv {
    Sandbox,
    Production,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenKind {
    Device,
    Activity,
    PushToStart,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Revoked,
    Oversize,
    /// The default relay serves invited computers only.
    Invite,
    Update,
    /// A push was asked for a phone whose grant has ended; the frame's `peer` names that phone.
    Grant,
    Auth,
    Malformed,
    PairExpired,
    /// The `auth` signature is valid but made for another relay address ("signed for another relay URL"): the
    /// relay's configured public URL does not match what the device connected to.
    RelayUrl,
    NoPeer,
    #[serde(other)]
    Other,
}

/// A computer that got in by redeeming an invite code, as the relay lists it to an owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invitee {
    pub id: String,
    /// The label the owner put in the code (may be empty).
    #[serde(default)]
    pub label: String,
    /// The owner computer whose code it was.
    pub by: String,
    /// Unix ms of the redemption.
    pub at: u64,
}

/// Why the relay refused an invite code (or, for `already`, why it did not need it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InviteReason {
    Expired,
    /// Some computer redeemed this code before.
    Used,
    /// Made for another relay.
    OtherRelay,
    /// Signed by a computer that is not one of this relay's owners.
    NotOwner,
    /// Not a code, or its signature does not hold, or it expires too far ahead.
    Invalid,
    TooMany,
    /// The relay could not save it; nothing was spent.
    Server,
    /// This computer was invited already; the code was not spent.
    Already,
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Frame {
    /// relay → device, first frame of a connection.
    Challenge { nonce: String },
    /// device → relay. `relay` is the address the device is connected to; `sig` is Ed25519 over
    /// `command::auth_bytes(relay, nonce)`. The relay calls `command::verify_auth_frame`.
    Auth { id: String, key: String, relay: String, sig: String, versions: VersionRange },
    /// The devices allowed to send to this one.
    Peers { allow: Vec<String> },
    Revoke { id: String },
    Send {
        pid: String,
        to: String,
        /// Seconds the relay may hold it for an offline peer.
        ttl: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        collapse: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        push: Option<Push>,
        body: String,
    },
    Msg { pid: String, from: String, body: String },
    Ack { pid: String },
    Queued { pid: String },
    Delivered { pid: String },
    Dropped { pid: String },
    Withdraw { pid: String },
    Withdrawn { pid: String },
    TooLate { pid: String },
    Presence { peer: String, state: PresenceState },
    /// A desktop tells the relay it is going to sleep (or woke up).
    State { asleep: bool },
    /// `relay_token_hash` is `PairingCode::relay_token_hash`, URL-safe base64.
    PairOpen { relay_token_hash: String, ttl: u32 },
    PairSend { relay_token: String, body: String },
    PushToken {
        platform: Platform,
        env: ApnsEnv,
        kind: TokenKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r#ref: Option<String>,
        token: String,
        grant: String,
    },
    /// computer → relay, after login: who am I to this relay's invitations? Answered by `inviteState`.
    InviteQuery {},
    /// computer → relay: an invite code (`invite::InviteCode`, text form). Answered by `inviteResult`.
    InviteRedeem { code: String },
    /// owner → relay: no longer invite a computer that got in by a code. Answered by `inviteState`.
    InviteRemove { id: String },
    /// relay → computer. `relay_id` and `invitees` are sent to an owner only.
    InviteState {
        owner: bool,
        invited: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay_id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        invitees: Vec<Invitee>,
    },
    InviteResult {
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<InviteReason>,
    },
    Error {
        code: ErrorCode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<Who>,
        /// For `grant`: the id of the phone whose push grant ended (the desktop asks that phone for a new one).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        peer: Option<String>,
    },
}

impl Frame {
    /// Parses a text frame. The size is checked before anything is parsed.
    pub fn parse(text: &str) -> Result<Frame, FrameError> {
        if text.len() > MAX_FRAME {
            return Err(FrameError::Oversize);
        }
        serde_json::from_str(text).map_err(|_| FrameError::Malformed)
    }

    pub fn to_text(&self) -> Result<String, FrameError> {
        let t = serde_json::to_string(self).map_err(|_| FrameError::Malformed)?;
        if t.len() > MAX_FRAME {
            Err(FrameError::Oversize)
        } else {
            Ok(t)
        }
    }

    /// The packet id of the frames that have one.
    pub fn pid(&self) -> Option<&str> {
        match self {
            Frame::Send { pid, .. }
            | Frame::Msg { pid, .. }
            | Frame::Ack { pid }
            | Frame::Queued { pid }
            | Frame::Delivered { pid }
            | Frame::Dropped { pid }
            | Frame::Withdraw { pid }
            | Frame::Withdrawn { pid }
            | Frame::TooLate { pid } => Some(pid),
            _ => None,
        }
    }
}
