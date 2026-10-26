//! [`apply_live`] laid over a save: a mod game's sections, the saved
//! stats' params and the skills section of a class with more skills.

use super::*;

fn live_with(skills: usize) -> Live {
    let mut live = Live {
        stats: SNAPSHOT_STATS.iter().map(|&id| (id, 0)).collect(),
        ..Live::default()
    };
    live.stats[usize::from(LEVEL_STAT)].1 = 3;
    live.extra.skills = Some((0..skills).map(|i| (i % 5) as u8).collect());
    live
}

fn base() -> D2s {
    fresh(1, b"Tester", 0)
}

/// A class with more skills than a new character's 30 slots saves all of
/// them, and +0x2A says how many; a class with 30 or fewer keeps the 30.
#[test]
fn the_skills_section_holds_every_class_skill() {
    let save = apply_live(&base(), &live_with(95), 1);
    let b = save.body.as_ref().unwrap();
    assert_eq!(b.skills.len(), 95);
    assert_eq!(save.header.skill_count, 95);
    assert_eq!(b.skills[94], (94 % 5) as u8);
    for n in [30, 4] {
        let save = apply_live(&base(), &live_with(n), 1);
        assert_eq!(save.body.as_ref().unwrap().skills.len(), 30);
        assert_eq!(save.header.skill_count, 30);
    }
    // A loaded 95-skill save stays 95.
    let again = apply_live(&apply_live(&base(), &live_with(95), 1), &live_with(95), 2);
    assert_eq!(again.header.skill_count, 95);
}

/// A saved stat's params are the game's: they replace the loaded ones; the
/// layers of a stat the game does not save pass through.
#[test]
fn a_saved_stats_params_replace_the_loaded_ones() {
    let e = |id, layer, value| StatEntry { id, layer, value };
    let mut base = base();
    base.body.as_mut().unwrap().stats =
        Stats::Bits(vec![e(12, 0, 2), e(40, 0, 1), e(40, 2, 20), e(41, 4, 44)]);
    let mut live = live_with(30);
    live.stats.push((40, 6));
    live.stat_params = vec![e(40, 5, 50)];
    let save = apply_live(&base, &live, 1);
    let got = save.body.unwrap().stats.entries();
    assert_eq!(got, [e(12, 0, 3), e(40, 0, 6), e(40, 5, 50), e(41, 4, 44)]);
}

/// The sections of a game with save hooks replace the loaded ones; without
/// hooks (`None`) the loaded ones stay.
#[test]
fn mod_sections_come_from_the_hooks_or_stay() {
    let mut base = base();
    let old = vec![ModSection::new(*b"OLD ", vec![1])];
    base.body.as_mut().unwrap().mod_sections = old.clone();
    let kept = apply_live(&base, &live_with(30), 1);
    assert_eq!(kept.body.unwrap().mod_sections, old);
    let new = vec![ModSection::new(*b"NEW ", vec![2])];
    let live = Live {
        mod_sections: Some(new.clone()),
        ..live_with(30)
    };
    assert_eq!(apply_live(&base, &live, 1).body.unwrap().mod_sections, new);
}
