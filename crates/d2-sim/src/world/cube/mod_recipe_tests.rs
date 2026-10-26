//! A mod game's recipe hooks (row-aware ops and outputs, the eligibility
//! veto, the commit step) on the fake world, and D2RUST's rules
//! unchanged by them.

use std::sync::{Arc, Mutex};

use super::tests::{data, gem_recipe, Fake, GCV, P};
use super::*;
use crate::rules::{ModRules, Ruleset};

/// A recipe op the mod tests itself, on the player.
const OP_ROW: u8 = 210;

/// Records what the cube hands it; refuses the rows in `refuse`.
#[derive(Debug, Default)]
struct RowCube {
    refuse: Vec<usize>,
    /// Rows the op test saw.
    tested: Mutex<Vec<usize>>,
    /// (row, slot) of each output call.
    outputs: Mutex<Vec<(usize, usize)>>,
    /// (row, outputs) of each commit.
    commits: Mutex<Vec<(usize, Vec<UnitId>)>>,
}

impl CubeProvider for RowCube {
    fn op_scope(&self, op: u8) -> Option<OpScope> {
        (op == OP_ROW).then_some(OpScope::Recipe)
    }
    fn op_test_at(&self, _: &dyn CubeWorld, row: usize, _: &Recipe, _: UnitId) -> bool {
        self.tested.lock().unwrap().push(row);
        true
    }
    fn eligible(&self, _: &dyn CubeWorld, row: usize, _: &Recipe, player: UnitId) -> bool {
        assert_eq!(player, P);
        !self.refuse.contains(&row)
    }
    fn output_at(
        &self,
        _: &mut dyn CubeWorld,
        _: UnitId,
        row: usize,
        _: &Recipe,
        slot: usize,
        _: &Capture,
        _: i32,
    ) -> Option<CubeOutput> {
        self.outputs.lock().unwrap().push((row, slot));
        None
    }
    fn committed(
        &self,
        _: &mut dyn CubeWorld,
        row: usize,
        _: &Recipe,
        player: UnitId,
        outputs: &[UnitId],
    ) {
        assert_eq!(player, P);
        self.commits.lock().unwrap().push((row, outputs.to_vec()));
    }
}

#[derive(Debug)]
struct Rules(Arc<RowCube>);

impl ModRules for Rules {
    fn name(&self) -> &str {
        "test-recipes"
    }
    fn cube(&self) -> Option<&dyn CubeProvider> {
        Some(&*self.0)
    }
}

/// Two copies of the gem recipe with the mod's op: rows 0 and 1.
fn two_rows() -> CubeData {
    let mut r = gem_recipe();
    r.op = OP_ROW;
    data(vec![r.clone(), r])
}

fn run(cube: Arc<RowCube>) -> Transmute {
    let mut f = Fake {
        rules: Ruleset::Mod(Arc::new(Rules(cube))),
        ..Fake::new()
    };
    for _ in 0..3 {
        f.add(GCV, 2);
    }
    two_rows().transmute(&mut f, P)
}

#[test]
fn hooks_get_the_recipe_row_and_the_commit() {
    let cube = Arc::new(RowCube::default());
    let t = run(cube.clone());
    assert!(t.committed);
    assert_eq!(t.record, Some(0));
    assert_eq!(*cube.tested.lock().unwrap(), vec![0]);
    assert_eq!(*cube.outputs.lock().unwrap(), vec![(0, 0), (0, 1), (0, 2)]);
    assert_eq!(*cube.commits.lock().unwrap(), vec![(0, t.outputs.clone())]);
    assert!(!t.outputs.is_empty());
}

#[test]
fn an_ineligible_row_falls_through_to_the_next() {
    let cube = Arc::new(RowCube {
        refuse: vec![0],
        ..RowCube::default()
    });
    let t = run(cube.clone());
    assert_eq!(t.record, Some(1));
    // Row 0 was refused before its op ran.
    assert_eq!(*cube.tested.lock().unwrap(), vec![1]);
    assert_eq!(cube.commits.lock().unwrap()[0].0, 1);
}

#[test]
fn no_commit_when_nothing_is_eligible() {
    let cube = Arc::new(RowCube {
        refuse: vec![0, 1],
        ..RowCube::default()
    });
    let t = run(cube.clone());
    assert_eq!(t.record, None);
    assert!(!t.committed);
    assert!(cube.commits.lock().unwrap().is_empty());
    assert!(cube.outputs.lock().unwrap().is_empty());
}

#[test]
fn d2rust_rules_ignore_the_hooks() {
    // Under D2RUST's rules the unknown op passes and the first row runs.
    let mut f = Fake::new();
    for _ in 0..3 {
        f.add(GCV, 2);
    }
    let t = two_rows().transmute(&mut f, P);
    assert_eq!(t.record, Some(0));
    assert!(t.committed);
}

#[test]
fn calendar_defaults_to_the_local_date() {
    let mut f = Fake::new();
    f.date = (26, 2);
    let c = CubeWorld::calendar(&f);
    assert_eq!((c.year, c.month, c.day, c.weekday), (0, 0, 26, 1));
    assert!(f.unit_query().is_none());
}
