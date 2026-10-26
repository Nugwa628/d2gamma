//! The action wiring's side of a mod game's save hooks
//! ([`crate::rules::SaveHooks`]): the hooks act on the game through the
//! same [`CombatView`] host as the event hooks. Without save hooks (every
//! D2RUST game) nothing is asked: one check of the ruleset.

use crate::game::Game;
use crate::rules::ModSection;
use crate::units::UnitId;

use super::combat::CombatView;
use super::{ActionSim, Pending};

impl<X: Pending> ActionSim<X> {
    /// The mod sections of `player`'s save ([`SaveHooks::save_sections`]);
    /// `None` when the game's rules have no save hooks, so the save keeps
    /// the sections it was loaded with.
    ///
    /// [`SaveHooks::save_sections`]: crate::rules::SaveHooks::save_sections
    pub fn mod_save_sections(
        &mut self,
        game: &mut Game,
        player: UnitId,
    ) -> Option<Vec<ModSection>> {
        self.sys.hooks.rules.saves()?;
        self.combat(game, |cv, _| save_sections(cv, player))
    }

    /// Hands `player`'s loaded `sections` to the game's save hooks
    /// ([`SaveHooks::load_sections`]); nothing without save hooks.
    ///
    /// [`SaveHooks::load_sections`]: crate::rules::SaveHooks::load_sections
    pub fn mod_load_sections(&mut self, game: &mut Game, player: UnitId, sections: &[ModSection]) {
        if self.sys.hooks.rules.saves().is_none() {
            return;
        }
        self.combat(game, |cv, _| load_sections(cv, player, sections));
    }
}

fn save_sections<X: Pending>(
    cv: &mut CombatView<'_, X>,
    player: UnitId,
) -> Option<Vec<ModSection>> {
    // The ruleset is cloned (one `Arc`) so the hook can borrow the view.
    let rules = cv.v.h.rules.clone();
    Some(rules.saves()?.save_sections(cv, player))
}

fn load_sections<X: Pending>(cv: &mut CombatView<'_, X>, player: UnitId, sections: &[ModSection]) {
    let rules = cv.v.h.rules.clone();
    if let Some(hooks) = rules.saves() {
        hooks.load_sections(cv, player, sections);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::rules::{EventHost, ModRules, Ruleset, SaveHooks};
    use crate::units::UnitType;
    use crate::wiring::action::tests::Fx;

    /// Test save hooks: the player's stat 0 is saved as a section, and a
    /// loaded section is written back into stat 0 and kept.
    #[derive(Debug, Default)]
    struct Hooks {
        loaded: Mutex<Vec<(u32, Vec<ModSection>)>>,
    }

    impl SaveHooks for Hooks {
        fn save_sections(&self, h: &mut dyn EventHost, player: UnitId) -> Vec<ModSection> {
            let v = h.stat(player, 0, 0);
            vec![ModSection::new(*b"TEST", v.to_le_bytes().to_vec())]
        }
        fn load_sections(&self, h: &mut dyn EventHost, player: UnitId, sections: &[ModSection]) {
            if let Some(s) = d2_formats::d2s::mod_sections::find(sections, b"TEST") {
                let v = i32::from_le_bytes(s.data[..4].try_into().unwrap());
                h.set_stat(player, 0, v);
            }
            self.loaded
                .lock()
                .unwrap()
                .push((player.0, sections.to_vec()));
        }
    }

    #[derive(Debug)]
    struct TestGame(Arc<Hooks>);

    impl ModRules for TestGame {
        fn name(&self) -> &str {
            "test-saves"
        }
        fn saves(&self) -> Option<&dyn SaveHooks> {
            Some(self.0.as_ref())
        }
    }

    /// The mod's hooks build the save's sections from the game and take
    /// them back at load.
    #[test]
    fn the_hooks_save_and_load_their_sections() {
        let mut fx = Fx::new();
        let hooks = Arc::new(Hooks::default());
        fx.sim.sys.hooks.rules = Ruleset::Mod(Arc::new(TestGame(hooks.clone())));
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        fx.sim.with(&mut fx.game, |_, v| v.set_base(p, 0, 42));
        let saved = fx.sim.mod_save_sections(&mut fx.game, p);
        let want = vec![ModSection::new(*b"TEST", 42i32.to_le_bytes().to_vec())];
        assert_eq!(saved.as_deref(), Some(&want[..]));
        fx.sim.with(&mut fx.game, |_, v| v.set_base(p, 0, 0));
        fx.sim.mod_load_sections(&mut fx.game, p, &want);
        assert_eq!(fx.sim.sys.stats.unit_base(p, 0, 0), 42);
        assert_eq!(*hooks.loaded.lock().unwrap(), [(p.0, want)]);
    }

    /// D2RUST's own rules (and a mod without save hooks) ask nothing: no
    /// sections, so a save keeps the ones it was loaded with.
    #[test]
    fn without_save_hooks_nothing_is_asked() {
        #[derive(Debug)]
        struct NoSaves;
        impl ModRules for NoSaves {
            fn name(&self) -> &str {
                "no-saves"
            }
        }
        for rules in [Ruleset::Original, Ruleset::Mod(Arc::new(NoSaves))] {
            let mut fx = Fx::new();
            fx.sim.sys.hooks.rules = rules;
            let a = fx.a;
            let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
            fx.sim.with(&mut fx.game, |_, v| v.set_base(p, 0, 42));
            assert_eq!(fx.sim.mod_save_sections(&mut fx.game, p), None);
            let s = [ModSection::new(*b"TEST", 7i32.to_le_bytes().to_vec())];
            fx.sim.mod_load_sections(&mut fx.game, p, &s);
            assert_eq!(fx.sim.sys.stats.unit_base(p, 0, 0), 42);
        }
    }
}
