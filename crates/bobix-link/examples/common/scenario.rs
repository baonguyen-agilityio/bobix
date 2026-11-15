//! The scenarios `fake-desktop --scenario <name>` plays. Each is a short script of what the desktop sends after a
//! pairing and how it answers commands; the phone app's tests and `tests/fakes.rs` check what each one ends in.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    Normal,
    Asleep,
    Offline,
    AnsweredOnComputer,
    TwoPhones,
    Expired,
    LongCommand,
    CutCommand,
    Destructive,
    LongPlan,
    Question,
    FolderMissing,
    LowDisk,
    TwoComputers,
    Revoked,
    LatePush,
    OddInput,
    RelayMoved,
    UnknownCommand,
    Released,
    RightsChanged,
}

pub const ALL: [(&str, Scenario); 21] = [
    ("normal", Scenario::Normal),
    ("asleep", Scenario::Asleep),
    ("offline", Scenario::Offline),
    ("answered-on-computer", Scenario::AnsweredOnComputer),
    ("two-phones", Scenario::TwoPhones),
    ("expired", Scenario::Expired),
    ("long-command", Scenario::LongCommand),
    ("cut-command", Scenario::CutCommand),
    ("destructive", Scenario::Destructive),
    ("long-plan", Scenario::LongPlan),
    ("question", Scenario::Question),
    ("folder-missing", Scenario::FolderMissing),
    ("low-disk", Scenario::LowDisk),
    ("two-computers", Scenario::TwoComputers),
    ("revoked", Scenario::Revoked),
    ("late-push", Scenario::LatePush),
    ("odd-input", Scenario::OddInput),
    ("relay-moved", Scenario::RelayMoved),
    ("unknown-command", Scenario::UnknownCommand),
    ("released", Scenario::Released),
    ("rights-changed", Scenario::RightsChanged),
];

impl Scenario {
    pub fn parse(name: &str) -> Result<Scenario, String> {
        ALL.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).ok_or_else(|| {
            format!("unknown scenario '{name}'; valid names: {}", ALL.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", "))
        })
    }
}
