//! The format of the records a realm stores (`records`): who reads a
//! character's record and makes its summary.
//!
//! - [`RecordFormat::D2s`] (the default): every record is a `.d2s`. The
//!   realm reads its summary from the record's header and runs its own
//!   `.d2s` checks on it (the save checks, the item registry, the ladder's
//!   experience); a record that is no `.d2s` is refused.
//! - [`RecordFormat::Game`]: the records are in the format of the game the
//!   realm runs (a mod game with a save format of its own). The realm
//!   never parses or edits them: a save's summary is the one its game
//!   server made from the played character, and the `.d2s`-only steps are
//!   skipped. The record's name must still be the leased character's.
//!
//! The record format is the realm's, not each save's: a D2Gamma realm
//! never trusts a game server's summary, whatever bytes it sends.

use d2_net::msg::account::CharSummary;

/// The format of a realm's character records (see the module notes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RecordFormat {
    /// D2Gamma's own `.d2s` records, read and checked by the realm.
    #[default]
    D2s,
    /// The game's own format: opaque to the realm.
    Game,
}

impl RecordFormat {
    /// The config value: `d2s` or `game`.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "d2s" => Ok(Self::D2s),
            "game" => Ok(Self::Game),
            _ => Err("must be `d2s` or `game`".into()),
        }
    }

    /// `true` when the realm reads and edits its records as `.d2s`.
    pub fn is_d2s(self) -> bool {
        self == Self::D2s
    }

    /// The summary a stored `record` gets: read from the record (`.d2s`),
    /// or the game server's `reported` one (the game's format), stamped
    /// `last_played`. `None` reported in the game's format: refused, as
    /// the realm can't read the record.
    pub fn summary(
        self,
        record: &[u8],
        reported: Option<&CharSummary>,
        last_played: u64,
    ) -> Result<CharSummary, &'static str> {
        match self {
            Self::D2s => crate::summary::of_record(record, last_played),
            Self::Game => {
                let reported = reported.ok_or("the game server sent no summary")?;
                if record.is_empty() {
                    return Err("the record is empty");
                }
                if reported.name.is_empty() {
                    return Err("the record has no name");
                }
                Ok(CharSummary {
                    last_played,
                    season: 0,
                    ..reported.clone()
                })
            }
        }
    }
}
