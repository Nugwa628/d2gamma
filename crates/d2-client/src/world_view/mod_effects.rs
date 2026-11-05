//! Mod visual recipes composed from the existing missile, overlay, light and pixel paths.
use super::missiles::Missiles;
use crate::assets::path::FileSource;
use crate::bridge::world::{ClientWorld, UnitKey};
use crate::rules::lighting::records::{LightId, LightKind, LightList, Owner};
use d2_modkit::effects::Effect;
use std::collections::BTreeMap;
#[derive(Clone)]
pub struct Recipe {
    pub definition: Effect,
    pub missile: Option<u16>,
    pub overlay: Option<u16>,
}
#[derive(Default)]
pub struct ModEffects {
    pub recipes: BTreeMap<String, Recipe>,
    live: BTreeMap<(UnitKey, String), Live>,
    removed: Vec<Live>,
    maps: BTreeMap<u8, crate::scene::MapId>,
}
struct Live {
    act_loads: u64,
    recipe: Recipe,
    point: (u32, u32),
    follow: bool,
    born: u64,
    duration: u16,
    fx: Vec<u32>,
    light: Option<LightId>,
}
impl ModEffects {
    pub fn load(source: &dyn FileSource) -> Result<Self, String> {
        let mut result = Self::default();
        let (mut missiles, mut overlays) = (None, None);
        for (path, bytes) in source.effect_documents()? {
            for definition in d2_modkit::effects::parse(
                std::str::from_utf8(&bytes).map_err(|e| format!("{path}: {e}"))?,
            )
            .map_err(|e| format!("{path}: {e}"))?
            {
                let resolve = |name: &Option<String>,
                               table: &str,
                               column: &str,
                               cache: &mut Option<BTreeMap<String, u16>>|
                 -> Result<Option<u16>, String> {
                    name.as_ref()
                        .map(|name| {
                            if cache.is_none() {
                                *cache =
                                    Some(crate::ui::data_bindings::names(source, table, column)?);
                            }
                            let key = name
                                .to_ascii_lowercase()
                                .chars()
                                .take(31)
                                .collect::<String>();
                            cache
                                .as_ref()
                                .unwrap()
                                .get(&key)
                                .copied()
                                .filter(|id| *id != 0)
                                .ok_or_else(|| format!("{path}: unknown/nonvisual {table} {name}"))
                        })
                        .transpose()
                };
                let missile = resolve(&definition.missile, "missiles", "Missile", &mut missiles)?;
                let overlay = resolve(&definition.overlay, "overlay", "overlay", &mut overlays)?;
                let key = definition.name.clone();
                if result
                    .recipes
                    .insert(
                        key.clone(),
                        Recipe {
                            definition,
                            missile,
                            overlay,
                        },
                    )
                    .is_some()
                {
                    return Err(format!("{path}: duplicate effect {key}"));
                }
                if result.recipes.len() > 512 {
                    return Err("effects: at most 512 recipes".into());
                }
            }
        }
        Ok(result)
    }
    /// Duration zero cancels the matching visual. Nothing here can mutate simulation state.
    #[allow(clippy::too_many_arguments)]
    pub fn trigger(
        &mut self,
        name: &str,
        source: UnitKey,
        point: (u32, u32),
        follow: bool,
        duration: u16,
        world: &ClientWorld,
        missiles: &mut Missiles,
    ) -> bool {
        let tick = world.server_ticks;
        let key = (source, name.to_owned());
        if duration == 0 {
            if let Some(old) = self.live.remove(&key) {
                self.removed.push(old);
            }
            return true;
        }
        if self.removed.len() >= 256 {
            return false;
        }
        let Some(recipe) = self.recipes.get(name).cloned() else {
            return false;
        };
        if duration > 3600 || (!self.live.contains_key(&key) && self.live.len() >= 128) {
            return false;
        }
        if let Some(old) = self.live.remove(&key) {
            self.removed.push(old);
        }
        let mut fx = Vec::new();
        for (id, overlay) in recipe
            .missile
            .map(|id| (id, false))
            .into_iter()
            .chain(recipe.overlay.map(|id| (id, true)))
        {
            if let Some(id) =
                missiles.spawn_visual(id, overlay, point, follow.then_some(source), tick, duration)
            {
                fx.push(id);
            }
        }
        self.live.insert(
            key,
            Live {
                act_loads: world.act_loads,
                recipe,
                point,
                follow,
                born: tick,
                duration,
                fx,
                light: None,
            },
        );
        true
    }
    /// The bridge replaced its entire world/light list on reconnect. Old
    /// light handles may now name unrelated records and must be forgotten.
    pub(crate) fn forget_world(&mut self, missiles: &mut Missiles) {
        for v in std::mem::take(&mut self.live)
            .into_values()
            .chain(self.removed.drain(..))
        {
            for id in v.fx {
                missiles.remove_visual(id);
            }
        }
    }
    pub fn prepare(
        &mut self,
        world: &ClientWorld,
        lights: &mut LightList,
        missiles: &mut Missiles,
    ) {
        let expired: Vec<_> = self
            .live
            .iter()
            .filter(|((source, _), v)| {
                v.act_loads != world.act_loads
                    || world.server_ticks < v.born
                    || world.server_ticks - v.born >= u64::from(v.duration)
                    || !world.units.contains_key(source)
            })
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            if let Some(v) = self.live.remove(&k) {
                self.removed.push(v);
            }
        }
        for v in self.removed.drain(..) {
            if let Some(id) = v.light {
                let _ = lights.remove(id);
            }
            for id in v.fx {
                missiles.remove_visual(id);
            }
        }
        for ((source, _), v) in &mut self.live {
            if v.light.is_none() {
                if let Some(l) = &v.recipe.definition.light {
                    v.light = lights.create(
                        v.follow.then_some(Owner {
                            unit_type: u32::from(source.unit_type),
                            guid: source.guid,
                            client_only: false,
                        }),
                        (
                            crate::rules::lighting::records::unit_light_pos(v.point.0 as i32),
                            crate::rules::lighting::records::unit_light_pos(v.point.1 as i32),
                        ),
                        LightKind::Plain,
                        i32::from(l.radius),
                        255,
                        l.red,
                        l.green,
                        l.blue,
                    );
                }
            }
        }
    }
    pub fn add_particles(
        &mut self,
        world: &ClientWorld,
        assets: &mut super::ViewAssets,
        frame: &mut super::WorldFrame,
    ) -> Result<(), String> {
        use crate::frames::{FramePart, FrameSet, FrameSetKey, IndexFrame};
        use crate::scene::{DrawItem, DrawKey, Rect, ShadeChain};
        let Some(camera) = frame.camera else {
            return Ok(());
        };
        let key = FrameSetKey::new("modkit/effects/pixel", FramePart::Tile(0))
            .map_err(|e| e.to_string())?;
        let mut ordinal = 0u32;
        for ((source, _), v) in &self.live {
            let Some(p) = &v.recipe.definition.particles else {
                continue;
            };
            if !assets.frames.contains(&key) {
                assets
                    .frames
                    .insert(
                        key.clone(),
                        FrameSet {
                            frames: vec![
                                IndexFrame::new(1, 1, 0, 0, vec![1]).map_err(|e| e.to_string())?
                            ],
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
            let map = *self
                .maps
                .entry(p.color)
                .or_insert_with(|| assets.maps.push(crate::rules::blend::color_row(p.color)));
            let pos = if v.follow {
                world
                    .units
                    .get(source)
                    .and_then(|u| u.position)
                    .map(crate::bridge::predict::cell_centre)
                    .unwrap_or(v.point)
            } else {
                v.point
            };
            let at = camera.unit_draw(crate::rules::camera::moving_to_client(pos.0, pos.1), (0, 0));
            let age = world
                .tick_rate
                .progress(1, world.server_ticks.saturating_sub(v.born))
                .min(3600) as u16;
            for i in 0..p.count {
                if ordinal >= 4096 {
                    break;
                }
                let (x, y) = d2_modkit::effects::particle_position(p, i, age);
                let (x, y) = (at.0 + x, at.1 + y);
                if !Rect::world_clip().contains(i64::from(x), i64::from(y)) {
                    continue;
                }
                let mut item = DrawItem::new(assets.id(&key, 0).map_err(|e| e.to_string())?, x, y);
                item.clip = Rect::world_clip();
                item.shade = ShadeChain::new(&[map]).map_err(|e| e.to_string())?;
                item.key = DrawKey::new(crate::scene::order::pass::UNIDENTIFIED_9, 0, ordinal, 0)
                    .map_err(|e| e.to_string())?;
                frame.items.push(item);
                ordinal += 1;
            }
        }
        if ordinal > 0 {
            crate::scene::order(&mut frame.items);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::world::{ClientUnit, KindData, PlayerData};
    use crate::world_view::{ModelFeed, NoFeed, ViewAssets, WorldFrame};
    const KEY: UnitKey = UnitKey {
        unit_type: 0,
        guid: 1,
    };
    fn setup() -> (ModEffects, ClientWorld, Missiles, LightList, ViewAssets) {
        let definition=d2_modkit::effects::parse("version=1\n[[effects]]\nname='ember'\n[effects.light]\nradius=8\n[effects.particles]\ncount=16\nspread=0").unwrap().remove(0);
        let mut e = ModEffects::default();
        e.recipes.insert(
            "ember".into(),
            Recipe {
                definition,
                missile: None,
                overlay: None,
            },
        );
        let mut w = ClientWorld::default();
        let mut u = ClientUnit::new(KEY);
        u.position = Some((100, 100));
        u.kind = KindData::Player(PlayerData::default());
        w.units.insert(KEY, u);
        w.local_player = Some(KEY);
        (
            e,
            w,
            Missiles::default(),
            LightList::new(),
            ViewAssets::new(d2_formats::palette::Palette {
                colors: [Default::default(); 256],
            }),
        )
    }
    fn frame(w: &ClientWorld) -> WorldFrame {
        WorldFrame {
            camera: crate::world_view::frame_camera(w, &mut ModelFeed::<NoFeed>::default())
                .unwrap(),
            ..Default::default()
        }
    }
    #[test]
    fn mod_effects_lights_particles_expire_and_cancel_without_world_changes() {
        let (mut e, mut w, mut m, mut lights, mut assets) = setup();
        let before = w.units[&KEY].position;
        let mut empty = frame(&w);
        ModEffects::default()
            .add_particles(&w, &mut assets, &mut empty)
            .unwrap();
        assert!(empty.items.is_empty());
        assert!(!e.trigger("missing", KEY, (100 << 16, 100 << 16), true, 10, &w, &mut m));
        assert!(e.trigger("ember", KEY, (100 << 16, 100 << 16), true, 10, &w, &mut m));
        e.prepare(&w, &mut lights, &mut m);
        assert_eq!(lights.len(), 1);
        let mut f = frame(&w);
        e.add_particles(&w, &mut assets, &mut f).unwrap();
        assert_eq!(f.items.len(), 16);
        assert_eq!(w.units[&KEY].position, before);
        w.server_ticks = 9;
        e.prepare(&w, &mut lights, &mut m);
        assert_eq!(lights.len(), 1);
        w.server_ticks = 10;
        e.prepare(&w, &mut lights, &mut m);
        assert!(lights.is_empty());
        assert!(e.live.is_empty());
        assert!(e.trigger("ember", KEY, (0, 0), false, 10, &w, &mut m));
        e.prepare(&w, &mut lights, &mut m);
        assert!(e.trigger("ember", KEY, (0, 0), false, 0, &w, &mut m));
        e.prepare(&w, &mut lights, &mut m);
        assert!(lights.is_empty());
    }
    #[test]
    fn mod_effects_replace_cap_and_act_changes_clean_old_instances() {
        let (mut e, mut w, mut m, mut lights, _) = setup();
        assert!(e.trigger("ember", KEY, (0, 0), false, 10, &w, &mut m));
        e.prepare(&w, &mut lights, &mut m);
        w.act_loads += 1;
        assert!(e.trigger("ember", KEY, (0, 0), false, 10, &w, &mut m));
        e.prepare(&w, &mut lights, &mut m);
        assert_eq!(lights.len(), 1, "new-act event survives first prepare");
        w.act_loads += 1;
        e.prepare(&w, &mut lights, &mut m);
        assert!(lights.is_empty());
        for guid in 0..128 {
            assert!(e.trigger(
                "ember",
                UnitKey::new(1, guid),
                (0, 0),
                false,
                10,
                &w,
                &mut m
            ));
        }
        assert!(!e.trigger("ember", UnitKey::new(1, 129), (0, 0), false, 10, &w, &mut m));
        e.prepare(&w, &mut lights, &mut m);
        assert!(e.live.is_empty(), "removed sources leave no effects");
    }
    struct Files {
        text: String,
    }
    impl FileSource for Files {
        fn read_file(&self, name: &str) -> Option<Result<Vec<u8>, String>> {
            Some(Ok(if name.ends_with("missiles.txt") {
                b"Missile\tId\r\nnone\t0\r\nEMBER\t999\r\n".to_vec()
            } else {
                b"overlay\tId\r\nnone\t0\r\nglow\t888\r\n".to_vec()
            }))
        }
        fn effect_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
            Ok(vec![(
                "effects/test.toml".into(),
                self.text.as_bytes().to_vec(),
            )])
        }
    }
    #[test]
    fn mod_effects_resolve_engine_names_and_reject_typos() {
        let text = "version=1\n[[effects]]\nname='e'\nmissile='ember'\noverlay='glow'";
        let e = ModEffects::load(&Files { text: text.into() }).unwrap();
        assert_eq!(e.recipes["e"].missile, Some(1));
        assert_eq!(e.recipes["e"].overlay, Some(1));
        assert!(ModEffects::load(&Files {
            text: text.replace("ember", "missing")
        })
        .is_err());
        assert!(
            ModEffects::load(&crate::assets::path::MemorySource::default())
                .unwrap()
                .recipes
                .is_empty()
        );
    }
}
