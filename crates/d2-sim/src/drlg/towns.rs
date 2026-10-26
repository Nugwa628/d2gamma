//! The town levels of a game: the levels the town rules apply to (no
//! population in the rooms next to them, their rooms kept while any is in
//! use, no town portal cast in them, the waypoint arrival tile, the town
//! test of units in their rooms).
//!
//! D2RUST's are the five act towns ([`super::TOWN_LEVELS`]). A mod game
//! hands out its own ([`crate::rules::ModRules::town_levels`]); the host
//! gives them to the game's [`super::Dungeon`] when it creates the game
//! ([`super::Dungeon::set_towns`]).

/// A set of town level ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TownLevels(Vec<u32>);

impl TownLevels {
    /// The town levels `levels` (any order; duplicates count once).
    pub fn new(levels: impl IntoIterator<Item = u32>) -> Self {
        let mut v: Vec<u32> = levels.into_iter().collect();
        v.sort_unstable();
        v.dedup();
        Self(v)
    }

    /// Whether `level` is a town.
    pub fn is_town(&self, level: u32) -> bool {
        self.0.binary_search(&level).is_ok()
    }

    /// The town levels, ascending.
    pub fn levels(&self) -> &[u32] {
        &self.0
    }
}

impl Default for TownLevels {
    /// D2RUST's: the five act towns.
    fn default() -> Self {
        Self::new(super::TOWN_LEVELS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_act_towns() {
        let t = TownLevels::default();
        assert_eq!(t.levels(), super::super::TOWN_LEVELS);
        for l in 0..1024 {
            assert_eq!(t.is_town(l), super::super::is_town(l), "level {l}");
        }
    }

    #[test]
    fn a_set_is_sorted_once() {
        let t = TownLevels::new([300, 1, 300, 40]);
        assert_eq!(t.levels(), [1, 40, 300]);
        assert!(t.is_town(300) && !t.is_town(75));
    }
}
