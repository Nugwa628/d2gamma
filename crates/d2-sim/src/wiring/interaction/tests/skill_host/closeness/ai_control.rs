//! Actual monster overrides and alignment changes for AI curses.
use super::*;
use crate::skills::use_::bodies::BodyWorld;
pub(super) struct DataOnly(
    pub(super) std::collections::BTreeMap<UnitId, crate::monsters::init::MonsterData>,
);

impl<X> crate::wiring::action::monsters::MonsterWorld<X> for DataOnly {
    fn type_init(
        &mut self,
        _: &mut crate::units::hooks::Sim<'_>,
        _: &mut ActionHooks<X>,
        _: UnitId,
    ) {
    }
    fn umods(
        &mut self,
        _: &mut crate::units::hooks::Sim<'_>,
        _: &mut ActionHooks<X>,
        _: UnitId,
        _: Option<UnitId>,
        _: u8,
    ) {
    }
    fn assign_umod(
        &mut self,
        _: &mut crate::units::hooks::Sim<'_>,
        _: &mut ActionHooks<X>,
        _: UnitId,
        _: u8,
    ) {
    }
    fn forget(&mut self, _: UnitId) {}
    fn monster(&self, unit: UnitId) -> Option<&crate::monsters::init::MonsterData> {
        self.0.get(&unit)
    }
    fn monster_mut(&mut self, unit: UnitId) -> Option<&mut crate::monsters::init::MonsterData> {
        self.0.get_mut(&unit)
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

pub(super) fn setup(fx: &mut Fx, units: &[UnitId]) {
    let data = units
        .iter()
        .map(|&u| (u, crate::monsters::init::MonsterData::default()))
        .collect();
    fx.sim.sys.hooks.monster_world = Some(Box::new(DataOnly(data)));
    Arc::make_mut(&mut fx.sim.sys.hooks.tables).combat.monstats[0].switchai = true;
    for &u in units {
        let monster = fx.sim.sys.units.get_mut(u).unwrap();
        monster.flags |= 4;
        monster.mode = 1;
    }
}
pub(super) fn observe(fx: &mut Fx, out: &mut Outcome, units: &[UnitId]) {
    let h = &fx.sim.sys.hooks;
    let world = h.monster_world.as_ref().unwrap();
    out.ai_overrides = units
        .iter()
        .map(|&u| i64::from(world.monster(u).unwrap().override_kind))
        .sum();
    out.ai_nodes = units
        .iter()
        .filter(|&&u| fx.sim.sys.units.get(u).unwrap().node_index == 9)
        .count() as i64;
    fx.sim.skill_use(&mut fx.game, |w| {
        out.ai_alignments = units
            .iter()
            .map(|&u| i64::from(w.cv.v.alignment_stat(u)))
            .sum();
        out.ai_values = units
            .iter()
            .map(|&u| {
                [STATE, OTHER]
                    .iter()
                    .filter_map(|&state| w.state_list(u, i32::from(state)))
                    .map(|l| i64::from(w.list_get(l, 31)))
                    .sum::<i64>()
            })
            .sum();
    });
}
pub(super) fn remaining(fx: &Fx, units: &[UnitId]) -> i64 {
    let world = fx.sim.sys.hooks.monster_world.as_ref().unwrap();
    units
        .iter()
        .map(|&u| {
            i64::from(world.monster(u).unwrap().override_kind)
                + i64::from(fx.sim.sys.stats.has_state(u, u32::from(STATE)))
                + i64::from(fx.sim.sys.stats.has_state(u, u32::from(OTHER)))
        })
        .sum()
}
#[test]
fn ai_curse_report_changes_actual_overrides_alignment_nodes_and_stats() {
    let (_, all) = report();
    for name in ["ai_curse", "attract"] {
        let (_, native, data) = all.iter().find(|(n, _, _)| *n == name).unwrap();
        assert_eq!(native, data);
        assert_eq!(native.results, [1]);
        assert!(native.ai_overrides > 0);
        assert!(native.ai_alignments > 0);
        assert!(native.ai_nodes > 0);
        if name == "ai_curse" {
            assert_eq!(native.ai_values, 24);
        }
    }
}
#[test]
fn ai_curse_state_and_override_expire_on_the_actual_callbacks() {
    for name in ["ai_curse", "attract"] {
        let original = CASES.iter().find(|c| c.name == name).unwrap();
        let case = Case {
            window: 270,
            casts: 1,
            ..*original
        };
        let lifted = lift(0, &Cells(case.cells.iter().copied().collect())).unwrap();
        let native = run(&case, None, tables(&case, None));
        let data = run(
            &case,
            Some(&lifted.toml),
            tables(&case, Some(&lifted.cells)),
        );
        assert_eq!(native, data);
        assert_eq!(native.ai_remaining, 0);
        assert!(native.ai_overrides > 0);
    }
}
