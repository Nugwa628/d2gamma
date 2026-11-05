//! The grown part vocabulary (missile patterns, chains, bounces, teleports)
//! on the real wiring: the parts reach the engine's own missile, damage and
//! placement code. The parts' exact requests are checked on a recording host
//! in `rules::data_mod::skills::tests_parts`.

use crate::units::{UnitId, UnitType};

use super::blocks::{cast, fixture, install, life, LIFE, STATES};
use super::{fx_with, skills, Fx};

fn missiles(fx: &Fx) -> usize {
    fx.game.lists.units_of_type(UnitType::Missile).len()
}

#[test]
fn a_ring_an_arc_and_a_volley_fire_that_many_missiles() {
    for (part, count) in [
        ("kind = \"missile_ring\"\nmissile = 0\ncount = 12\n", 12),
        ("kind = \"missile_ring\"\nmissile = 0\n", 16),
        ("kind = \"missile_arc\"\nmissile = 0\ncount = 5\n", 5),
        (
            "kind = \"missile_volley\"\nmissile = 0\ncount = \"lvl\"\n",
            3,
        ),
    ] {
        let toml = format!("[[skill]]\nid = 0\n[[skill.part]]\n{part}");
        let (mut fx, p, _, _) = fixture(&toml);
        fx.sim.sys.hooks.x.aim_at = (16, 10);
        let before = missiles(&fx);
        assert_eq!(cast(&mut fx, p, 3), 1, "{part}");
        assert_eq!(missiles(&fx), before + count, "{part}");
        fx.assert_clean();
    }
}

#[test]
fn a_ring_of_a_missile_row_that_does_not_exist_does_nothing() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"missile_ring\"\nmissile = 60000\n";
    let (mut fx, p, _, _) = fixture(toml);
    let before = missiles(&fx);
    assert_eq!(cast(&mut fx, p, 1), 0);
    assert_eq!(missiles(&fx), before);
}

/// A second monster beside the first, in reach of a chain.
fn second(fx: &mut Fx) -> UnitId {
    let n = fx.spawn(UnitType::Monster, 13, 10);
    fx.set(
        n,
        &[
            (31, 1),
            (12, 1),
            (crate::stats::stat::MAXHP, LIFE),
            (crate::stats::stat::HITPOINTS, LIFE),
        ],
    );
    fx.sim.sys.units.get_mut(n).unwrap().flags |=
        crate::missiles::unit_flag::IS_VALID_TARGET | crate::missiles::unit_flag::CAN_BE_ATTACKED;
    n
}

#[test]
fn a_chain_hits_the_target_then_the_next_unit_in_reach_for_less() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"chain\"\nradius = 8\njumps = 4\n\
                falloff = 50\n[skill.part.damage]\nelement = \"fire\"\nmin = 40\nmax = 40\n";
    let (mut fx, p, m, far) = fixture(toml);
    let n = second(&mut fx);
    fx.sim.sys.hooks.x.targets.insert(p, m);
    assert_eq!(cast(&mut fx, p, 1), 1);
    assert_eq!(life(&mut fx, m), LIFE - 40 * 256);
    // Half as much for the next one; the monster 19 sub-tiles away is out of reach.
    assert_eq!(life(&mut fx, n), LIFE - 20 * 256);
    assert_eq!(life(&mut fx, far), LIFE);
    fx.assert_clean();
}

#[test]
fn a_bounce_comes_back_to_the_first_unit() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"bounce\"\nradius = 8\njumps = 2\n\
                [skill.part.damage]\nelement = \"fire\"\nmin = 10\nmax = 10\n";
    let (mut fx, p, m, _) = fixture(toml);
    let n = second(&mut fx);
    fx.sim.sys.hooks.x.targets.insert(p, m);
    cast(&mut fx, p, 1);
    // m, n, m again.
    assert_eq!(life(&mut fx, m), LIFE - 20 * 256);
    assert_eq!(life(&mut fx, n), LIFE - 10 * 256);
    fx.assert_clean();
}

#[test]
fn a_chain_needs_a_target() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"chain\"\nradius = 8\njumps = 4\n\
                [skill.part.damage]\nmin = 40\nmax = 40\n";
    let (mut fx, p, m, _) = fixture(toml);
    assert_eq!(cast(&mut fx, p, 1), 0);
    assert_eq!(life(&mut fx, m), LIFE);
}

#[test]
fn a_charge_part_raises_damage_with_the_stacks_and_spends_them() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"area_hit\"\nradius = 6\n\
                around = \"caster\"\n[skill.part.damage]\nelement = \"fire\"\nmin = 20\nmax = 20\n\
                charge = { state = 30, percent = 50, consume = true }\n";
    let (mut fx, p, m, _) = fixture(toml);
    // Two stacks first, from a skill of the same kit.
    let charge = "[[skill]]
id = 0
[[skill.part]]
kind = \"state\"
op = \"add_stack\"
state = 30
count = 2
";
    install(&mut fx, charge, STATES);
    cast(&mut fx, p, 1);
    install(&mut fx, toml, STATES);
    assert_eq!(super::blocks::stacks(&fx, p), 2);
    cast(&mut fx, p, 1);
    // 20 plus two stacks of 50%.
    assert_eq!(life(&mut fx, m), LIFE - 40 * 256);
    assert_eq!(super::blocks::stacks(&fx, p), 0);
    fx.assert_clean();
}

#[test]
fn a_teleport_moves_the_caster_to_the_aim_point() {
    let mut fx = fx_with(skills());
    fx.sim.hooks().enable_paths().expect("embedded tables");
    let p = fx.spawn(UnitType::Player, 10, 10);
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"teleport\"\n";
    install(&mut fx, toml, STATES);
    fx.sim.sys.hooks.x.aim_at = (20, 12);
    assert_eq!(cast(&mut fx, p, 1), 1);
    assert_eq!(fx.sim.sys.hooks.path_position(p), (20, 12));
    // Out of every room: it stays.
    fx.sim.sys.hooks.x.aim_at = (5000, 5000);
    assert_eq!(cast(&mut fx, p, 1), 0);
    assert_eq!(fx.sim.sys.hooks.path_position(p), (20, 12));
}

#[test]
fn an_engine_body_part_runs_the_engines_own_slot() {
    // Slot 22 is the engine's nova: 64 missiles of the skill's missile row.
    let nova = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"engine_body\"\nslot = 22\n";
    let (mut fx, p, _, _) = fixture(nova);
    let before = missiles(&fx);
    assert_eq!(cast(&mut fx, p, 3), 1);
    assert_eq!(missiles(&fx), before + 64);
    fx.assert_clean();

    // A slot with no engine body goes to the host, as the core sends it.
    let other = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"engine_body\"\nslot = 53\n";
    let (mut fx, p, _, _) = fixture(other);
    cast(&mut fx, p, 1);
    assert_eq!(fx.sim.sys.hooks.x.log, ["srvdo 53"]);
}
