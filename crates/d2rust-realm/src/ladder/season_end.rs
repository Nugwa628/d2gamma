//! Season end (lobby contract section 6, "Seasons"; plan `lobby-ui.md`
//! L6b): what closing a season does to its characters and its ladder.
//!
//! - **The final ranks stay:** an ended season's rows are its record. A
//!   save of one of its characters (one still in a game when the season
//!   ended) no longer writes them ([`Ladder::frozen`]), so the ended
//!   season ranks as it stood when it closed, browsable like any other
//!   (a deleted character still leaves it, as every season).
//! - **Its characters become non-ladder when next loaded:** the seat path
//!   loads a character's record for its game ([`SaveDesk::seat`]); for a
//!   character of an ended season the hook ([`Ladder::retire`]) clears
//!   the record's ladder bit, writes the record back, puts the character
//!   in season 0 and writes its new non-ladder row (level and experience
//!   read from the record). The game gets the changed record, so its next
//!   save keeps the bit cleared and goes to season 0.
//! - **The admin's lines:** `season-end` (console) and `--season-end`
//!   (offline) end the open season and say how many ranked characters its
//!   record keeps ([`Ladder::season_end`]).
//!
//! [`SaveDesk::seat`]: crate::realm::saves::SaveDesk::seat

use d2_accounts::ladder::{Split, NON_LADDER};
use d2_accounts::social::SocialResult;
use d2_accounts::{characters, summary, AccountId, CharSeat};
use d2_formats::d2s::status;
use d2_net::msg::account::CharSummary;
use d2_net::msg::ErrorCode;

use super::{now_ms, save_hook, Ladder};
use crate::denied::Denied;
use crate::realm::saves::{SeatLoad, SeatLoading};

/// How many characters an ended season ranks, over its four ladders.
pub fn ranked(db: &d2_accounts::db::Db, game: &str, season: u16) -> SocialResult<usize> {
    let mut n = 0;
    for bits in 0..4 {
        n += db
            .ladder()
            .split(game, season, Split::from_bits(bits))?
            .len();
    }
    Ok(n)
}

/// The summary of a character of an ended season once it is non-ladder:
/// season 0, the ladder bit cleared.
pub fn non_ladder(s: &CharSummary) -> CharSummary {
    CharSummary {
        status: s.status & !status::LADDER,
        season: NON_LADDER,
        ..s.clone()
    }
}

/// `record` with its ladder bit cleared; `None` when the bit isn't set
/// (nothing to write) or the record has no status word.
pub fn cleared_record(record: &[u8]) -> Option<Vec<u8>> {
    let st = summary::of_record(record, 0).ok()?.status;
    if st & status::LADDER == 0 {
        return None;
    }
    let mut r = record.to_vec();
    summary::set_status(&mut r, st & !status::LADDER).then_some(r)
}

impl Ladder {
    /// Whether `season` has ended: its rows are its final ranks, which
    /// saves no longer change.
    pub async fn frozen(&self, season: u16) -> Result<bool, Denied> {
        if season == NON_LADDER {
            return Ok(false);
        }
        let game = self.game.clone();
        Ok(self
            .accounts
            .store(move |db| Ok(db.ladder().season(&game, season)?))
            .await?
            .is_some_and(|s| s.ended_ms.is_some()))
    }

    /// The load hook: `seat` is the account's character as loaded for a
    /// game. A character of an ended season becomes non-ladder (see the
    /// module notes); any other passes unchanged. A record that can't be
    /// written back refuses the seat (logged), so a character never plays
    /// on with a record and a row that disagree.
    pub async fn retire(&self, account: AccountId, seat: &mut CharSeat) -> Result<(), Denied> {
        let season = seat.summary.season;
        if !self.frozen(season).await? {
            return Ok(());
        }
        let name = seat.summary.name.clone();
        // The record first: with the row changed and the record not, the
        // next load would see season 0 and leave the bit set for good.
        if let Some(cleared) = seat.record.as_deref().and_then(cleared_record) {
            let (path, record) = (seat.file.clone(), cleared.clone());
            tokio::task::spawn_blocking(move || characters::write_record(&path, &record))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()))
                .map_err(|e| {
                    self.log.line(&format!(
                        "ladder: `{name}` of ended season {season}: writing its record: {e}"
                    ));
                    Denied::new(ErrorCode::OTHER)
                })?;
            seat.record = Some(cleared);
        }
        let after = non_ladder(&seat.summary);
        let experience = self.experience_of(seat, &after).await;
        let entry = experience.map(|xp| save_hook::entry_of(account.0, &after, xp, now_ms()));
        let (game, row) = (self.game.clone(), after.clone());
        self.accounts
            .store(move |db| {
                db.ladder().set_character_season(&row.name, NON_LADDER)?;
                db.save_summary(account.0, &row)?;
                if let Some(e) = &entry {
                    db.ladder().upsert(&game, e)?;
                }
                Ok(())
            })
            .await?;
        seat.summary = after;
        self.forget_ranks();
        self.log.line(&format!(
            "ladder: `{name}` of ended season {season} is non-ladder now{}",
            match experience {
                Some(xp) => format!(" (season 0 row: experience {xp})"),
                None => String::new(),
            }
        ));
        Ok(())
    }

    /// The experience in the seat's record, read with the realm's game
    /// data; `None` without a record or the tables (then the next save
    /// writes the row), or when it doesn't read (logged).
    async fn experience_of(&self, seat: &CharSeat, after: &CharSummary) -> Option<u64> {
        let tables = self.tables.clone()?;
        let record = seat.record.clone()?;
        let expansion = after.status & status::EXPANSION != 0;
        let read = tokio::task::spawn_blocking(move || {
            summary::experience(&record, &tables.reading(expansion))
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        match read {
            Ok(xp) => Some(xp),
            Err(e) => {
                self.log
                    .line(&format!("ladder: `{}`'s experience: {e}", after.name));
                None
            }
        }
    }
}

impl SeatLoad for Ladder {
    fn loaded<'a>(&'a self, account: AccountId, seat: &'a mut CharSeat) -> SeatLoading<'a> {
        Box::pin(self.retire(account, seat))
    }
}

#[cfg(test)]
#[path = "season_end_tests.rs"]
mod tests;
