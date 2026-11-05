//! The owner's worked example, `templates/proc-charges`, end to end on the
//! real wiring: a melee attack sometimes opens a one-second reaction
//! window; using one of three skills inside it gives a charge (five at
//! most) and closes it; a window that runs out costs a charge; from three
//! charges on, skill 390 hits 25% harder.
//!
//! The mod's own files are read from the template folder so the example the
//! docs show is the one that is tested. Only three things change: the ids of
//! the three skills move onto the one skills row the test fixture has and
//! its neighbours, the passive that holds the triggers is stood in for by
//! the unit's level stat (the fixture has no passive rows), and the damage
//! is fixed so a bonus can be told from a roll.

use crate::rules::{DataMod, DataModFiles, HostDamage, HostStates, RuledState};
use crate::skills::use_::UseWorld;
use crate::stats::stat as sst;
use crate::units::UnitId;

use super::super::skill_use::Fx;
use super::blocks::{fixture, life, LIFE};

/// The monster's life: a hit of 125 must not be cut off at its 100.
const TOUGH: i32 = LIFE * 10;

const STATES: &str = include_str!("../../../../../../../templates/proc-charges/files/states.toml");
const SKILLS: &str = include_str!("../../../../../../../templates/proc-charges/files/skills.toml");
const TRIGGERS: &str =
    include_str!("../../../../../../../templates/proc-charges/files/triggers.toml");

const WINDOW: u16 = 190;
const CHARGES: u16 = 191;
/// The stat the charges give and Ember Lash reads.
const BONUS: u16 = 340;
/// The template's skills as the test numbers them: Ember Lash (the block skill
/// row), Frost Nova, Quick Strike; and the passive's id in the template.
const EMBER: i32 = 0;
const FROST: i32 = 1;
const STRIKE: i32 = 2;
const PASSIVE: i32 = 393;

const ATTACK: u8 = 7;

/// The template's files with its skill ids moved to rows 0 to 3 (0 is the passive) and Ember
/// Lash's damage fixed at 100.
fn files() -> (String, String) {
    let skills = SKILLS
        .replace("id = 390", "id = 0")
        .replace("id = 391", "id = 1")
        .replace("id = 392", "id = 2")
        .replace("min = \"10 + lvl * 4\"", "min = 100")
        .replace("max = \"20 + lvl * 6\"", "max = 100");
    let triggers = TRIGGERS
        .replace("skills = [390, 391, 392]", "skills = [0, 1, 2]")
        .replace("while_skill = 393", "while_stat = 12");
    (skills, triggers)
}

/// A player who is the passive's holder (stat 12) and has Ember Lash, playing the template mod.
fn game() -> (Fx, UnitId, UnitId) {
    let (mut fx, p, m, _) = fixture("");
    {
        let tables = std::sync::Arc::make_mut(&mut fx.sim.sys.hooks.tables);
        tables.combat.monstats2[0].isatt = true;
        let row = &mut tables.skills.skills[EMBER as usize];
        row.srvdofunc = crate::rules::BLOCK_SLOT;
        row.srvmissile = 0xFFFF;
    }
    let entry = |skill| crate::skills::SkillEntry {
        skill,
        base: 1,
        owner_guid: -1,
        ..crate::skills::SkillEntry::default()
    };
    fx.sim.sys.hooks.x.skills.insert(p, vec![entry(EMBER)]);
    let (skills, triggers) = files();
    let files = DataModFiles {
        states: Some(("states.toml", STATES)),
        skills: Some(("skills.toml", &skills)),
        triggers: Some(("triggers.toml", &triggers)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("proc-charges", &files)
        .unwrap()
        .into_ruleset();
    // The real server turns the unit event registry on.
    fx.sim.sys.hooks.enable_unit_events();
    fx.sim.sys.hooks.x.aim_at = (11, 10);
    fx.set(m, &[(sst::MAXHP, TOUGH), (sst::HITPOINTS, TOUGH)]);
    fx.sim.sys.hooks.x.targets.insert(p, m);
    (fx, p, m)
}

fn spec(state: u16) -> RuledState {
    RuledState {
        state,
        skill: PASSIVE,
        level: 1,
        base_ticks: 0,
    }
}

fn open_window(fx: &mut Fx, p: UnitId) {
    fx.sim
        .skill_use(&mut fx.game, |w| w.apply_ruled_state(p, p, &spec(WINDOW)));
}

fn window(fx: &Fx, p: UnitId) -> bool {
    fx.sim.sys.stats.has_state(p, u32::from(WINDOW))
}

fn charges(fx: &Fx, p: UnitId) -> u16 {
    let Some(r) = fx.sim.sys.stats.unit_list(p) else {
        return 0;
    };
    fx.sim
        .sys
        .stats
        .list_of_state(r, u32::from(CHARGES))
        .map_or(0, |l| fx.sim.sys.stats.stacks(l))
}

fn bonus(fx: &Fx, p: UnitId) -> i32 {
    fx.sim.sys.stats.unit_total(p, BONUS, 0)
}

/// The unit swings at `m` in melee.
fn swing(fx: &mut Fx, p: UnitId, m: UnitId) {
    fx.sim.skill_use(&mut fx.game, |w| {
        HostDamage::unit_event(w, ATTACK, p, Some(m), None);
    });
}

/// Uses skill `skill` as the game does when it did something.
fn use_skill(fx: &mut Fx, p: UnitId, skill: i32) {
    fx.sim.skill_use(&mut fx.game, |w| {
        UseWorld::skill_done(w, p, skill, 1);
    });
}

/// Casts Ember Lash through the use path; the monster's life lost, in
/// whole points. The monster is back at full life after.
fn ember(fx: &mut Fx, p: UnitId, m: UnitId) -> i32 {
    let before = life(fx, m);
    let table = fx.sim.sys.hooks.tables.clone();
    let r = fx.sim.skill_use(&mut fx.game, |w| {
        crate::skills::use_::do_skill(w, &table.skills, p, EMBER, 1)
    });
    assert_eq!(r, 1, "Ember Lash did something");
    let lost = before - life(fx, m);
    fx.set(m, &[(sst::HITPOINTS, TOUGH)]);
    lost / 256
}

fn frames(fx: &mut Fx, n: usize) {
    for _ in 0..n {
        fx.frame();
    }
}

#[test]
fn the_template_loads_and_names_the_hud_icon() {
    let (fx, _, _) = game();
    let rules = fx.sim.sys.hooks.rules.state_rules().unwrap();
    let c = rules.get(i32::from(CHARGES)).unwrap();
    assert_eq!((c.max_stacks, c.icon.as_deref()), (5, Some("charges")));
    assert_eq!(
        rules.get(i32::from(WINDOW)).unwrap().duration_seconds,
        Some(1)
    );
}

#[test]
fn about_eight_percent_of_attacks_open_the_window() {
    let (mut fx, p, m) = game();
    let mut opened = 0;
    for _ in 0..2000 {
        swing(&mut fx, p, m);
        if window(&fx, p) {
            opened += 1;
            fx.sim.skill_use(&mut fx.game, |w| w.clear_state(p, WINDOW));
        }
    }
    // 8% of 2000 is 160; the rolls come from the unit's seed, so this is
    // the same every run.
    assert!((100..=240).contains(&opened), "{opened} windows in 2000");
    fx.assert_clean();
}

#[test]
fn a_window_is_not_reopened_while_it_is_open() {
    let (mut fx, p, m) = game();
    open_window(&mut fx, p);
    frames(&mut fx, 10);
    for _ in 0..500 {
        swing(&mut fx, p, m);
    }
    // 10 frames in, the clock was not restarted by the attacks.
    frames(&mut fx, 15);
    assert!(!window(&fx, p), "the window ran out on its own clock");
    fx.assert_clean();
}

#[test]
fn a_skill_in_the_window_gives_a_charge_and_closes_it() {
    let (mut fx, p, _) = game();
    use_skill(&mut fx, p, FROST);
    assert_eq!(charges(&fx, p), 0, "no window, no charge");
    open_window(&mut fx, p);
    use_skill(&mut fx, p, FROST);
    assert_eq!(charges(&fx, p), 1);
    assert!(!window(&fx, p), "the window is used up");
    use_skill(&mut fx, p, STRIKE);
    assert_eq!(charges(&fx, p), 1, "one charge per window");
    // A skill outside the three gives nothing.
    open_window(&mut fx, p);
    use_skill(&mut fx, p, 9);
    assert_eq!(charges(&fx, p), 1);
    assert!(window(&fx, p));
    fx.assert_clean();
}

#[test]
fn three_charges_make_ember_lash_hit_a_quarter_harder() {
    let (mut fx, p, m) = game();
    open_window(&mut fx, p);
    let plain = ember(&mut fx, p, m);
    assert_eq!(plain, 100);
    // The cast inside the window earned the first charge, after its hit.
    assert_eq!(charges(&fx, p), 1);
    for skill in [FROST, STRIKE] {
        assert_eq!(bonus(&fx, p), 0, "under three charges there is no bonus");
        open_window(&mut fx, p);
        use_skill(&mut fx, p, skill);
    }
    assert_eq!(charges(&fx, p), 3);
    assert_eq!(bonus(&fx, p), 25);
    assert_eq!(ember(&mut fx, p, m), 125);
    // More charges do not add more; the bonus is flat.
    for _ in 0..2 {
        open_window(&mut fx, p);
        use_skill(&mut fx, p, FROST);
    }
    assert_eq!(charges(&fx, p), 5);
    assert_eq!(bonus(&fx, p), 25);
    open_window(&mut fx, p);
    use_skill(&mut fx, p, FROST);
    assert_eq!(charges(&fx, p), 5, "five at most");
    fx.assert_clean();
}

#[test]
fn missing_the_window_costs_a_charge_and_the_bonus_with_it() {
    let (mut fx, p, m) = game();
    for _ in 0..3 {
        open_window(&mut fx, p);
        use_skill(&mut fx, p, FROST);
    }
    assert_eq!((charges(&fx, p), bonus(&fx, p)), (3, 25));
    open_window(&mut fx, p);
    frames(&mut fx, 24);
    assert!(window(&fx, p));
    assert_eq!(charges(&fx, p), 3);
    frames(&mut fx, 1);
    assert!(!window(&fx, p));
    assert_eq!(charges(&fx, p), 2, "the missed window cost a charge");
    assert_eq!(bonus(&fx, p), 0);
    assert_eq!(ember(&mut fx, p, m), 100);
    // With none left a missed window costs nothing.
    for _ in 0..2 {
        open_window(&mut fx, p);
        frames(&mut fx, 26);
    }
    assert_eq!(charges(&fx, p), 0);
    open_window(&mut fx, p);
    frames(&mut fx, 26);
    assert_eq!(charges(&fx, p), 0);
    fx.assert_clean();
}

#[test]
fn the_template_does_nothing_without_the_passive() {
    let (mut fx, p, m) = game();
    fx.set(p, &[(12, 0)]);
    for _ in 0..300 {
        swing(&mut fx, p, m);
    }
    assert!(!window(&fx, p));
    open_window(&mut fx, p);
    use_skill(&mut fx, p, FROST);
    assert_eq!(charges(&fx, p), 0);
    fx.assert_clean();
}

#[test]
fn a_game_without_the_mod_has_none_of_it() {
    let (mut fx, p, m) = game();
    fx.sim.sys.hooks.rules = crate::rules::Ruleset::Original;
    for _ in 0..300 {
        swing(&mut fx, p, m);
    }
    use_skill(&mut fx, p, FROST);
    assert!(!window(&fx, p));
    assert_eq!(charges(&fx, p), 0);
    fx.assert_clean();
}
