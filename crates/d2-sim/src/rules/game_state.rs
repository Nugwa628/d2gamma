//! Per-game mod rules (`docs/plans/mods/seams.md` MS11).
//!
//! A server runs many games on one [`Ruleset`]. A mod game that keeps
//! state per game (counters, locks, balances its hooks change) makes a
//! fresh rules object for each game in [`super::ModRules::for_game`]: the game
//! builder asks once, before anything of the game is built, and every
//! seam of that game then asks the per-game object. A mod game whose
//! rules hold no game state keeps the default, and all its games share
//! the one object.

use super::Ruleset;

/// The game being built, as [`super::ModRules::for_game`] sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NewGame {
    /// The game's seed: its map seeds and its random seed come from it.
    pub seed: u32,
    /// The game's difficulty (0 normal, 1 nightmare, 2 hell).
    pub difficulty: u8,
    /// The game runs the expansion rules.
    pub expansion: bool,
}

impl Ruleset {
    /// The ruleset the game `game` is built with: the mod game's own
    /// rules for that game ([`super::ModRules::for_game`]) when it makes them,
    /// else this ruleset. [`Ruleset::Original`] stays itself.
    pub fn for_game(&self, game: &NewGame) -> Ruleset {
        match self {
            Ruleset::Original => Ruleset::Original,
            Ruleset::Mod(rules) => match rules.for_game(game) {
                Some(own) => Ruleset::Mod(own),
                None => self.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::rules::ModRules;

    /// A mod game counting kills per game: each game gets its own counter.
    #[derive(Debug, Default)]
    struct Counting {
        kills: AtomicU32,
        made: Mutex<Vec<(NewGame, Arc<Counting>)>>,
    }

    impl ModRules for Counting {
        fn name(&self) -> &str {
            "counting"
        }
        fn for_game(&self, game: &NewGame) -> Option<Arc<dyn ModRules>> {
            let own = Arc::new(Counting::default());
            self.made.lock().unwrap().push((*game, own.clone()));
            Some(own)
        }
    }

    /// A mod game with no game state: its games share it.
    #[derive(Debug)]
    struct Shared;

    impl ModRules for Shared {
        fn name(&self) -> &str {
            "shared"
        }
    }

    /// The ruleset's mod rules are `want`.
    fn is(r: &Ruleset, want: &Arc<Counting>) -> bool {
        r.mod_rules().is_some_and(|m| {
            std::ptr::eq(
                m as *const dyn ModRules as *const (),
                Arc::as_ptr(want) as *const (),
            )
        })
    }

    #[test]
    fn each_game_gets_its_own_rules() {
        let base = Arc::new(Counting::default());
        let r = Ruleset::Mod(base.clone());
        let g1 = NewGame {
            seed: 7,
            difficulty: 2,
            expansion: true,
        };
        let g2 = NewGame { seed: 9, ..g1 };
        let (a, b) = (r.for_game(&g1), r.for_game(&g2));
        let made = base.made.lock().unwrap().clone();
        // The mod was asked once per game, with the game's facts, and each
        // game runs the object made for it.
        assert_eq!(made.iter().map(|m| m.0).collect::<Vec<_>>(), [g1, g2]);
        assert!(is(&a, &made[0].1) && is(&b, &made[1].1));
        assert!(!is(&a, &base) && !Arc::ptr_eq(&made[0].1, &made[1].1));
        // State changed in one game stays in that game.
        made[0].1.kills.fetch_add(3, Ordering::Relaxed);
        assert_eq!(made[1].1.kills.load(Ordering::Relaxed), 0);
        assert_eq!(base.kills.load(Ordering::Relaxed), 0);
        // A clone of a game's ruleset is the same game's rules.
        assert!(is(&a.clone(), &made[0].1));
    }

    #[test]
    fn rules_without_game_state_are_shared() {
        let shared: Arc<dyn ModRules> = Arc::new(Shared);
        let r = Ruleset::Mod(shared.clone());
        let Ruleset::Mod(got) = r.for_game(&NewGame::default()) else {
            panic!("a mod ruleset stays a mod ruleset");
        };
        assert!(Arc::ptr_eq(&got, &shared));
    }

    #[test]
    fn original_stays_original() {
        assert!(Ruleset::Original
            .for_game(&NewGame::default())
            .is_original());
    }
}
