//! A mod game's monster overlays ([`super::super::monster_overlays`]) at
//! the 0xAC monster assign; synthetic.

use std::sync::{Arc, Mutex};

use super::super::hit_overlays::HitOverlay;
use super::super::monster_overlays::{MonsterOverlayHook, MonsterOverlays};
use super::super::world::{MonsterClass, UnitKey, MONSTER};
use super::support::{hex, Model};

/// A mod's hook: overlay 40 for a unique, then 100 + each umod; it keeps
/// what it was asked.
#[derive(Default)]
struct ModOverlays {
    asked: Mutex<Vec<(u16, u8, Vec<u8>)>>,
}

impl MonsterOverlays for ModOverlays {
    fn overlays(&self, class: u16, flags: u8, umods: &[u8]) -> Vec<u16> {
        self.asked
            .lock()
            .unwrap()
            .push((class, flags, umods.to_vec()));
        let unique = (flags & 8 != 0).then_some(40);
        unique
            .into_iter()
            .chain(umods.iter().map(|&u| 100 + u16::from(u)))
            .collect()
    }
}

/// Packs `(value, bits)` fields low bits first.
fn pack(fields: &[(u32, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (pos, bit) in fields
        .iter()
        .flat_map(|&(v, n)| (0..n).map(move |i| (v >> i) & 1))
        .enumerate()
    {
        if pos / 8 == out.len() {
            out.push(0);
        }
        out[pos / 8] |= (bit as u8) << (pos % 8);
    }
    out
}

/// 0xAC of monster 9, class 1, mode 1: a unique with umods 5 and 7.
fn unique_assign() -> Vec<u8> {
    let stream = pack(&[
        (1, 4), // mode
        (0, 1), // no components
        (1, 1), // has type flags
        (0, 1), // champion
        (1, 1), // unique → 8
        (0, 1), // superunique
        (0, 1), // minion
        (0, 1), // ghostly (no hcIdx: not a superunique)
        (5, 8),
        (7, 8),
        (0, 8), // umods end
        (0, 16),
        (0, 1), // no value
        (0, 1), // no 31-bit value
        (0, 1), // no stat list
    ]);
    let mut b = hex("ac 09 00 00 00 01 00 0a 00 0b 00 10");
    b.push((0xD + stream.len()) as u8);
    b.extend(stream);
    b
}

fn assigned(hook: Option<Arc<ModOverlays>>) -> Model {
    let mut m = Model::default();
    m.inputs.tables.monsters = vec![Some(MonsterClass::default()); 2];
    m.inputs.monster_overlays = hook.map(|h| MonsterOverlayHook(h as Arc<dyn MonsterOverlays>));
    m.recv(&unique_assign());
    assert!(m.log.rejected.is_empty(), "{:?}", m.rejected());
    m
}

#[test]
fn the_mod_hook_makes_its_overlays_on_the_new_monster() {
    let hook = Arc::new(ModOverlays::default());
    let m = assigned(Some(hook.clone()));
    assert_eq!(*hook.asked.lock().unwrap(), [(1, 8, vec![5, 7])]);
    let key = UnitKey::new(MONSTER, 9);
    let made: Vec<_> =
        m.w.hit_overlays
            .iter()
            .filter(|o| o.unit == key && o.kind == 3)
            .map(|o| (o.id, o.args, o.remove))
            .collect();
    assert_eq!(
        made,
        [
            (40, [0; 3], false),
            (105, [0; 3], false),
            (107, [0; 3], false)
        ]
    );
}

#[test]
fn without_a_hook_the_assign_makes_no_monster_overlays() {
    let m = assigned(None);
    let with = assigned(Some(Arc::new(ModOverlays::default())));
    let base: Vec<HitOverlay> = m.w.hit_overlays.clone();
    // The hook only adds its own records after D2RUST's.
    assert_eq!(with.w.hit_overlays[..base.len()], base[..]);
    assert_eq!(with.w.hit_overlays.len(), base.len() + 3);
    assert!(base.iter().all(|o| o.kind != 3));
}

#[test]
fn client_mods_carry_the_hook() {
    let mods = crate::mods::ClientMods::new();
    assert!(mods.monster_overlays().is_none());
    let mods = mods.with_monster_overlays(ModOverlays::default());
    assert!(!mods.is_empty());
    let hook = mods.monster_overlays().expect("set");
    assert_eq!(hook.0.overlays(0, 8, &[]), [40]);
}
