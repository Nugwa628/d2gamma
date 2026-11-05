//! Named telegraph visuals; no simulation writes or client combat decisions.
use super::{missiles::Missiles, mod_effects::ModEffects};
use crate::bridge::world::{ClientWorld, UnitKey};
use d2_proto::mod_ui::VisualEvent;
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Telegraphs {
    live: BTreeMap<(UnitKey, String), (u64, u64, u16)>,
    epoch: Option<u64>,
}
impl Telegraphs {
    pub fn update(
        &mut self,
        world: &ClientWorld,
        epoch: u64,
        events: Vec<VisualEvent>,
        effects: &mut ModEffects,
        missiles: &mut Missiles,
    ) {
        let reset = self.epoch.is_some_and(|e| e != epoch);
        self.epoch = Some(epoch);
        if reset {
            effects.forget_world(missiles);
            self.live.clear();
        }
        let expired: Vec<_> = self
            .live
            .iter()
            .filter(|((key, _), (act, born, duration))| {
                reset
                    || *act != world.act_loads
                    || world.server_ticks < *born
                    || world.server_ticks.saturating_sub(*born) >= u64::from(*duration)
                    || world.units.get(key).is_none_or(|u| u.is_dead())
            })
            .map(|(k, _)| k.clone())
            .collect();
        for (key, name) in expired {
            effects.trigger(&name, key, (0, 0), false, 0, world, missiles);
            self.live.remove(&(key, name));
        }
        for event in events {
            let key = UnitKey::new(event.source_type, event.source_guid);
            let identity = (key, event.name.clone());
            if event.duration == 0 {
                effects.trigger(&event.name, key, (0, 0), false, 0, world, missiles);
                self.live.remove(&identity);
                continue;
            }
            if world.units.get(&key).is_none_or(|u| u.is_dead()) {
                continue;
            }
            if self.live.len() >= 128 && !self.live.contains_key(&identity) {
                continue;
            }
            if effects.trigger(
                &event.name,
                key,
                crate::bridge::predict::cell_centre((event.x, event.y)),
                event.follow,
                event.duration,
                world,
                missiles,
            ) {
                self.live.insert(
                    identity,
                    (world.act_loads, world.server_ticks, event.duration),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::mod_effects::Recipe;
    use super::*;
    use crate::{
        bridge::world::ClientUnit,
        rules::lighting::records::{LightKind, LightList},
    };
    fn setup() -> (Telegraphs, ModEffects, Missiles, ClientWorld, LightList) {
        let mut e = ModEffects::default();
        e.recipes.insert(
            "windup".into(),
            Recipe {
                definition: d2_modkit::effects::parse(
                    "version=1\n[[effects]]\nname='windup'\n[effects.light]\nradius=8",
                )
                .unwrap()
                .remove(0),
                missile: None,
                overlay: None,
            },
        );
        let mut w = ClientWorld::default();
        for guid in [1, 2] {
            let key = UnitKey::new(1, guid);
            let mut u = ClientUnit::new(key);
            u.mode = 1;
            u.position = Some((100, 100));
            w.units.insert(key, u);
        }
        (
            Telegraphs::default(),
            e,
            Missiles::default(),
            w,
            LightList::new(),
        )
    }
    fn event(guid: u32) -> VisualEvent {
        VisualEvent {
            name: "windup".into(),
            source_type: 1,
            source_guid: guid,
            x: 100,
            y: 100,
            duration: 25,
            follow: true,
        }
    }
    #[test]
    fn telegraphs_are_per_caster_and_cancel_on_death_expiry_and_removal() {
        let (mut t, mut e, mut m, mut w, mut l) = setup();
        t.update(&w, 0, vec![event(1), event(2)], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert_eq!(l.len(), 2);
        w.units.get_mut(&UnitKey::new(1, 1)).unwrap().mode = 0;
        t.update(&w, 0, vec![], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert_eq!(l.len(), 1);
        w.server_ticks = 25;
        t.update(&w, 0, vec![], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert!(l.is_empty());
        assert!(t.live.is_empty());
        t.update(&w, 0, vec![event(2)], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert_eq!(l.len(), 1);
        w.units.remove(&UnitKey::new(1, 2));
        t.update(&w, 0, vec![], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert!(l.is_empty());
    }
    #[test]
    fn telegraphs_cancel_replace_and_ignore_unknown_or_dead_sources() {
        let (mut t, mut e, mut m, w, mut l) = setup();
        let mut unknown = event(1);
        unknown.name = "missing".into();
        t.update(&w, 0, vec![unknown, event(99)], &mut e, &mut m);
        assert!(t.live.is_empty());
        t.update(&w, 0, vec![event(1), event(1)], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert_eq!(l.len(), 1);
        let mut cancel = event(1);
        cancel.duration = 0;
        t.update(&w, 0, vec![cancel], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert!(l.is_empty());
    }
    #[test]
    fn telegraphs_act_change_and_resume_forget_owned_handles_safely() {
        let (mut t, mut e, mut m, mut w, mut l) = setup();
        t.update(&w, 0, vec![event(1)], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        w.act_loads += 1;
        t.update(&w, 0, vec![event(2)], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert_eq!(l.len(), 1);
        let old_id = l.iter().next().unwrap().0;
        l = LightList::new();
        for _ in 0..old_id.0 {
            l.create(None, (0, 0), LightKind::Plain, 8, 255, 1, 2, 3)
                .unwrap();
        }
        let unrelated = l
            .create(None, (0, 0), LightKind::Plain, 8, 255, 1, 2, 3)
            .unwrap();
        t.update(&w, 1, vec![], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert!(
            l.get(unrelated).is_some(),
            "resume must not remove reused light ids from the new world"
        );
        assert!(t.live.is_empty());
    }

    #[test]
    fn telegraphs_bound_instances_and_clear_on_clock_rewind() {
        let (mut t, mut e, mut m, mut w, mut l) = setup();
        for guid in 3..=129 {
            let key = UnitKey::new(1, guid);
            let mut u = ClientUnit::new(key);
            u.mode = 1;
            u.position = Some((100, 100));
            w.units.insert(key, u);
        }
        w.server_ticks = 10;
        t.update(&w, 0, (1..=129).map(event).collect(), &mut e, &mut m);
        assert_eq!(t.live.len(), 128);
        assert!(!t
            .live
            .contains_key(&(UnitKey::new(1, 129), "windup".into())));
        let mut cancel = event(1);
        cancel.duration = 0;
        t.update(&w, 0, vec![cancel, event(129)], &mut e, &mut m);
        assert_eq!(t.live.len(), 128);
        assert!(t
            .live
            .contains_key(&(UnitKey::new(1, 129), "windup".into())));
        e.prepare(&w, &mut l, &mut m);
        w.server_ticks = 9;
        t.update(&w, 0, vec![], &mut e, &mut m);
        e.prepare(&w, &mut l, &mut m);
        assert!(t.live.is_empty());
        assert!(l.is_empty());
    }
}
