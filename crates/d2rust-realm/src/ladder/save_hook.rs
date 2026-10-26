//! The ladder's save hook (lobby contract section 6, "Updating"): after
//! every stored save the character's ladder row is written, its level and
//! experience read from the stored record with the realm's game data.
//!
//! The network side hands each stored save over ([`super::Ladder::saved`]);
//! one worker ([`super::Ladder::run_saves`]) writes the rows in the order
//! the saves were stored, so an older save never overwrites a newer one.
//! A non-ladder character's row is in season 0, a ladder character's in its
//! season.

use d2_accounts::ladder::{LadderEntry, Split};
use d2_formats::d2s::status;
use d2_net::msg::account::CharSummary;

/// A stored save waiting for its ladder row: the account and the
/// character's name.
pub type Saved = (i64, String);

/// The ladder row of a character whose stored summary is `summary` and
/// whose record holds `experience`, written at `now_ms`.
pub fn entry_of(account: i64, summary: &CharSummary, experience: u64, now_ms: i64) -> LadderEntry {
    let hardcore = summary.status & status::HARDCORE != 0;
    LadderEntry {
        season: summary.season,
        name: summary.name.clone(),
        account,
        class: summary.class,
        level: summary.level,
        experience,
        split: Split {
            hardcore,
            expansion: summary.status & status::EXPANSION != 0,
        },
        dead: hardcore && summary.status & status::DEAD != 0,
        reached_ms: now_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(status: u16, season: u16) -> CharSummary {
        CharSummary {
            name: "Kashya".into(),
            class: 1,
            level: 40,
            status,
            season,
            ..CharSummary::default()
        }
    }

    #[test]
    fn the_row_follows_the_summary() {
        let e = entry_of(
            7,
            &summary(status::EXPANSION | status::LADDER, 2),
            1_000,
            55,
        );
        assert_eq!(
            (
                e.season,
                e.account,
                e.class,
                e.level,
                e.experience,
                e.reached_ms
            ),
            (2, 7, 1, 40, 1_000, 55)
        );
        assert_eq!(
            e.split,
            Split {
                hardcore: false,
                expansion: true
            }
        );
        assert!(!e.dead);
        let e = entry_of(7, &summary(status::HARDCORE | status::DEAD, 0), 0, 0);
        assert!(e.dead && e.split.hardcore && !e.split.expansion);
        // A softcore character that died plays on: not dead on the ladder.
        assert!(!entry_of(7, &summary(status::DEAD, 0), 0, 0).dead);
    }
}
