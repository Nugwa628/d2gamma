//! Authoritative AI producers can publish a named visual to each eligible viewer.
use d2_proto::{
    envelope::{self, Direction},
    mod_ui::{self, VisualEvent},
};
use d2_sim::{
    rules::EventHost,
    units::{UnitId, UnitType},
};
/// Wind-up duration is in server ticks; zero explicitly cancels this source/name.
pub struct Cue<'a> {
    pub name: &'a str,
    pub point: (u16, u16),
    pub duration: u16,
    pub follow: bool,
}
/// The producer selects viewers using its normal visibility rules. Identity comes
/// from the live server unit list, never a client claim. Call only on the mod wire.
pub fn emit(host: &mut dyn EventHost, viewer: UnitId, source: UnitId, cue: Cue<'_>) -> bool {
    if host.unit_type(viewer) != Some(UnitType::Player)
        || host.unit_type(source) != Some(UnitType::Monster)
    {
        return false;
    }
    let event = {
        let game = host.game();
        let Some(unit) = game.lists.unit(source) else {
            return false;
        };
        if unit.ty != UnitType::Monster {
            return false;
        }
        VisualEvent {
            name: cue.name.into(),
            source_type: unit.ty as u8,
            source_guid: unit.guid,
            x: cue.point.0,
            y: cue.point.1,
            duration: cue.duration,
            follow: cue.follow,
        }
    };
    let Some(payload) = mod_ui::visual(&event) else {
        return false;
    };
    let Ok(bytes) = envelope::encode(Direction::ToClient, &payload) else {
        return false;
    };
    host.send(viewer, &bytes);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Host {
        game: d2_sim::game::Game,
        sent: Vec<(UnitId, Vec<u8>)>,
    }
    impl EventHost for Host {
        fn game(&mut self) -> &mut d2_sim::game::Game {
            &mut self.game
        }
        fn unit_type(&self, u: UnitId) -> Option<UnitType> {
            self.game.lists.unit(u).map(|u| u.ty)
        }
        fn class_id(&self, _: UnitId) -> i32 {
            0
        }
        fn stat(&self, _: UnitId, _: u16, _: u16) -> i32 {
            0
        }
        fn set_stat(&mut self, _: UnitId, _: u16, _: i32) {
            panic!("visual may not write stats");
        }
        fn has_state(&self, _: UnitId, _: u16) -> bool {
            false
        }
        fn seed(&mut self, _: UnitId) -> Option<&mut d2_sim::rng::Seed> {
            panic!("visual may not draw game RNG");
        }
        fn level(&self, _: UnitId) -> Option<u32> {
            None
        }
        fn send(&mut self, u: UnitId, b: &[u8]) {
            self.sent.push((u, b.to_vec()));
        }
    }
    #[test]
    fn telegraph_visual_emission_uses_live_guid_and_addressed_viewer() {
        let mut h = Host::default();
        let player = h
            .game
            .lists
            .add_unit(UnitType::Player, 100, None, false)
            .unwrap();
        let monster = h
            .game
            .lists
            .add_unit(UnitType::Monster, 999, None, false)
            .unwrap();
        let cue = || Cue {
            name: "ember/windup",
            point: (123, 456),
            duration: 25,
            follow: true,
        };
        assert!(emit(&mut h, player, monster, cue()));
        assert_eq!(h.sent.len(), 1);
        assert_eq!(h.sent[0].0, player);
        let payload = envelope::payload(Direction::ToClient, &h.sent[0].1).unwrap();
        let event = mod_ui::read_visual(payload).unwrap();
        assert_eq!(event.source_guid, 999);
        assert_ne!(event.source_guid, monster.0);
        assert_eq!((event.x, event.y), (123, 456));
        assert!(!emit(&mut h, monster, monster, cue()));
        assert!(!emit(&mut h, player, player, cue()));
        assert!(!emit(
            &mut h,
            player,
            monster,
            Cue {
                name: "bad name",
                ..cue()
            }
        ));
        assert!(!emit(
            &mut h,
            player,
            monster,
            Cue {
                duration: 3601,
                ..cue()
            }
        ));
        assert_eq!(h.sent.len(), 1);
        h.game.lists.remove_unit(monster).unwrap();
        assert!(!emit(&mut h, player, monster, cue()));
        assert_eq!(h.sent.len(), 1);
    }
}
