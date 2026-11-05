//! Curated drop rules shared by every treasure-class walk. Counts change the
//! NoDrop exponent; TC picks and explicit output limits remain table/caller data.

use super::{Dropper, DropperKind, GameFacts, Recipient};
use serde::Deserialize;

/// Integer controls applied before the TC walk consumes randomness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DropRules {
    /// Honor the single-player /players floor.
    pub players_setting: bool,
    /// Count nearby living party members fully instead of as other players.
    pub party_bonus: bool,
    /// Divisor for players outside the nearby party (2 counts each half).
    pub other_player_divisor: u32,
    /// Limit the exponent to the monster's spawn-time player count.
    pub monster_spawn_cap: bool,
    /// Scale recipient MF before quality diminishing returns.
    pub magic_find_percent: u32,
    /// Optional upper limit on scaled MF; negative MF remains negative.
    pub magic_find_cap: Option<u32>,
    /// Scale raw TC NoDrop before the player exponent.
    pub nodrop_percent: u32,
}

impl Default for DropRules {
    fn default() -> Self {
        Self::defaults()
    }
}

impl DropRules {
    pub const fn defaults() -> Self {
        Self {
            players_setting: true,
            party_bonus: true,
            other_player_divisor: 2,
            monster_spawn_cap: true,
            magic_find_percent: 100,
            magic_find_cap: None,
            nodrop_percent: 100,
        }
    }

    pub fn player_factor(
        &self,
        game: &GameFacts,
        dropper: &Dropper,
        recipient: Option<&Recipient>,
    ) -> i32 {
        let p = if self.party_bonus {
            match recipient.and_then(|r| r.party) {
                Some(k) if k >= 2 => k.min(8),
                _ => 1,
            }
        } else {
            1
        };
        let mut players = game.living_players;
        if self.players_setting && matches!(game.game_type, 1..=3) {
            players = players.max(game.players_setting);
        }
        let mut n =
            p.wrapping_add(players.wrapping_sub(p) / self.other_player_divisor.clamp(1, 8) as i32);
        if self.monster_spawn_cap {
            if let DropperKind::Monster { playercount, .. } = dropper.kind {
                n = n.min(playercount.max(1));
            }
        }
        n
    }

    pub fn magic_find(&self, value: i32) -> i32 {
        let value = scale(value, self.magic_find_percent);
        self.magic_find_cap
            .map_or(value, |cap| value.min(cap.min(i32::MAX as u32) as i32))
    }
}

/// Signed percentage with saturation and an identity path for existing arithmetic.
pub(crate) fn scale(value: i32, percent: u32) -> i32 {
    if percent == 100 {
        return value;
    }
    (i64::from(value) * i64::from(percent) / 100).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
        as i32
}
