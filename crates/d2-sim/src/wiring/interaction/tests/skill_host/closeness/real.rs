//! The closeness report on the rows of a real install: every row of the
//! install's skills table that the lifter knows, lifted from the row's own
//! numbers and formulas, then run both ways on the report's fixture.
//!
//! The fixture is the report's, so the ids a row names (its missile, its aura
//! state) stand for the fixture's one missile and one state; what is the
//! install's own is every number the lifted skill reads: the do function, the
//! aura filter, range and length, the missile's speed, the skill's
//! parameters and the formulas. A row with a formula the lifter's language
//! cannot say, or one the fixture cannot run, is counted and left out. Only
//! row numbers and counts are written, never names or table cells.
//! It reads the game files in `D2_GAME_DIR` and writes the markdown to the file
//! named by `D2_CLOSENESS_REAL_REPORT` when that is set, so it is ignored by
//! default.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::panic::{catch_unwind, AssertUnwindSafe};

use d2_data::tables::Skills;

use crate::rules::data_mod::lift::{lift, LiftError, Source};
use crate::skills::special::SKILLCALC_CODES;
use crate::skills::SkillTables;

use super::super::blocks::STATE;
use super::{flagged, metrics, run, tables, unfit, Outcome, CASES, NONE, OTHER};

mod player;

/// The install's skills tables, from the archives in `D2_GAME_DIR`.
#[allow(clippy::disallowed_methods)]
fn install() -> (SkillTables, BTreeMap<usize, u32>) {
    use d2_data::bin::{read_excel, BinTable, DataRules};
    use d2_data::tables::{decode_all, Itemstatcost, Missiles, Monstats, Record, Skilldesc};
    let dir = std::env::var("D2_GAME_DIR").expect("D2_GAME_DIR must be set");
    let set = d2_formats::mpq::ArchiveSet::open_dir(dir).expect("archives open");
    let file = |name: &str| {
        read_excel(&set, name)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap_or_else(|| panic!("{name}: in no archive"))
    };
    let table = |name: &str, size: usize| {
        let bin = format!("{name}.bin");
        let (source, bytes) = file(&bin);
        BinTable::parse(name, &source, &bin, &bytes, size).expect("table parses")
    };
    // How many monster rows name each skills row in one of their eight skill columns.
    let monsters: Vec<Monstats> = decode_all(&table("monstats", Monstats::SIZE)).unwrap();
    let mut used: BTreeMap<usize, u32> = BTreeMap::new();
    for m in &monsters {
        let mut rows: Vec<u16> = vec![
            m.skill1, m.skill2, m.skill3, m.skill4, m.skill5, m.skill6, m.skill7, m.skill8,
        ];
        rows.retain(|&r| r != 0xFFFF);
        rows.sort_unstable();
        rows.dedup();
        for r in rows {
            *used.entry(usize::from(r)).or_default() += 1;
        }
    }
    let tables = SkillTables {
        skills: decode_all(&table("skills", Skills::SIZE)).unwrap(),
        skilldesc: decode_all(&table("skilldesc", Skilldesc::SIZE)).unwrap(),
        missiles: decode_all(&table("missiles", Missiles::SIZE)).unwrap(),
        skills_code: file("skillscode.bin").1,
        miss_code: file("misscode.bin").1,
        level_cap: crate::skills::LEVEL_CAP_114D,
        stat_count: table("itemstatcost", Itemstatcost::SIZE).count as i32,
        data_rules: DataRules::Original,
    };
    (tables, used)
}

// Shared with authoring-time name resolution; the report reads the exact
// built-in operators, functions and parameters instead of zeroing them.
use crate::rules::data_mod::lift::formulas::decompile;

/// A row of the install, as the lifter reads a skills row. Ids stand for the
/// fixture's missile and state; the cells that are formulas are read back from
/// the install's code buffer, and what that cannot say is recorded in
/// `blocked`.
struct Real<'a> {
    t: &'a SkillTables,
    id: usize,
    blocked: RefCell<Vec<String>>,
    missile: RefCell<Option<usize>>,
}

impl Real<'_> {
    fn rec(&self) -> &Skills {
        &self.t.skills[self.id]
    }

    fn calc(&self, column: &str, offset: u32) -> String {
        decompile(&self.t.skills_code, offset).unwrap_or_else(|why| {
            self.blocked
                .borrow_mut()
                .push(format!("`{column}` uses {why}"));
            String::new()
        })
    }
}

fn row_or_empty(row: u16) -> String {
    if row == 0xFFFF {
        String::new()
    } else {
        row.to_string()
    }
}

impl Source for Real<'_> {
    fn monster(&self, _: &str) -> Option<u16> {
        Some(0)
    }
    fn pet_type(&self, _: &str) -> Option<u16> {
        Some(2)
    }
    fn skill(&self, column: &str) -> String {
        let r = self.rec();
        if matches!(
            r.srvdofunc,
            2 | 6 | 10 | 17 | 18 | 34 | 35 | 49 | 59 | 61 | 66 | 68 | 69 | 72 | 81 | 116
        ) || (r.srvdofunc == 0 && r.passivestate > 0 && r.passivestate < 0x8000)
        {
            let stats = [
                r.aurastat1,
                r.aurastat2,
                r.aurastat3,
                r.aurastat4,
                r.aurastat5,
                r.aurastat6,
            ];
            let calcs = [
                r.aurastatcalc1,
                r.aurastatcalc2,
                r.aurastatcalc3,
                r.aurastatcalc4,
                r.aurastatcalc5,
                r.aurastatcalc6,
            ];
            let passive = [
                r.passivestat1,
                r.passivestat2,
                r.passivestat3,
                r.passivestat4,
                r.passivestat5,
            ];
            let pcalcs = [
                r.passivecalc1,
                r.passivecalc2,
                r.passivecalc3,
                r.passivecalc4,
                r.passivecalc5,
            ];
            for (i, (&stat, &calc)) in stats.iter().zip(&calcs).enumerate() {
                if column == format!("aurastat{}", i + 1) {
                    return row_or_empty(stat);
                }
                if column == format!("aurastatcalc{}", i + 1) {
                    return self.calc(column, calc);
                }
            }
            for (i, (&stat, &calc)) in passive.iter().zip(&pcalcs).enumerate() {
                if column == format!("passivestat{}", i + 1) {
                    return row_or_empty(stat);
                }
                if column == format!("passivecalc{}", i + 1) {
                    return self.calc(column, calc);
                }
            }
            for (i, (&event, &callback)) in [r.auraevent1, r.auraevent2, r.auraevent3]
                .iter()
                .zip(&[r.auraeventfunc1, r.auraeventfunc2, r.auraeventfunc3])
                .enumerate()
            {
                if column == format!("auraevent{}", i + 1) {
                    return row_or_empty(event);
                }
                if column == format!("auraeventfunc{}", i + 1) {
                    return row_or_empty(callback);
                }
            }
        }
        match column {
            "skill" => format!("skill {}", self.id),
            "srvdofunc" => r.srvdofunc.to_string(),
            "passivestate" => row_or_empty(r.passivestate),
            "passiveitype" => row_or_empty(r.passiveitype),
            "summon" => row_or_empty(r.summon),
            "sumumod" => r.sumumod.to_string(),
            "summode" => r.summode.to_string(),
            "pettype" => r.pettype.to_string(),
            "petmax" => self.calc(column, r.petmax),
            "delay" => self.calc(column, r.delay),
            "perdelay" => self.calc(column, r.perdelay),
            "mana" => r.mana.to_string(),
            "lvlmana" => r.lvlmana.to_string(),
            "manashift" => r.manashift.to_string(),
            "hitclass" => r.hitclass.to_string(),
            "hitflags" => r.hitflags.to_string(),
            "resultflags" => r.resultflags.to_string(),
            "InTown" => u8::from(r.intown).to_string(),
            "srvmissile" => row_or_empty(r.srvmissile),
            "srvmissilea" => row_or_empty(r.srvmissilea),
            "srvmissileb" => row_or_empty(r.srvmissileb),
            "lob" => u8::from(r.lob).to_string(),
            "progressive" => u8::from(r.progressive).to_string(),
            "calc1" => self.calc(column, r.calc1),
            "calc2" => self.calc(column, r.calc2),
            "calc3" => self.calc(column, r.calc3),
            "srvstfunc" => r.srvstfunc.to_string(),
            "srvoverlay" => row_or_empty(r.srvoverlay),
            "param1" => r.param1.to_string(),
            "sumoverlay" => row_or_empty(r.sumoverlay),
            "tohitcalc" => self.calc(column, r.tohitcalc),
            "tohit" => r.tohit.to_string(),
            "levtohit" => r.levtohit.to_string(),
            "srcdam" => r.srcdam.to_string(),
            "aurastate" => row_or_empty(r.aurastate),
            "auratargetstate" => row_or_empty(r.auratargetstate),
            "aurafilter" => r.aurafilter.to_string(),
            "aurarangecalc" => self.calc(column, r.aurarangecalc),
            "auralencalc" => self.calc(column, r.auralencalc),
            // Family-specific stat columns are read above; unknown columns
            // stay empty rather than borrowing another fixture value.
            _ => String::new(),
        }
    }
    fn state(&self, name: &str) -> Option<u16> {
        // The fixture's one state stands for the aura's own; any other state
        // the row names (for the units around) is the fixture's second.
        let own = row_or_empty(if self.rec().srvdofunc == 0 {
            self.rec().passivestate
        } else {
            self.rec().aurastate
        });
        Some(if name == own { STATE } else { OTHER })
    }
    fn stat(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    fn missile(&self, name: &str) -> Option<u16> {
        let real: usize = name.parse().ok()?;
        *self.missile.borrow_mut() = Some(real);
        Some(0)
    }
    fn missile_cell(&self, _: u16, column: &str) -> String {
        let row = self.missile.borrow().and_then(|m| self.t.missiles.get(m));
        let cell = row.map(|m| match column {
            "vel" => m.vel,
            "vellev" => m.vellev,
            _ => 0,
        });
        cell.map(|c| c.to_string()).unwrap_or_default()
    }
}

/// The fixture's tables with the row's own numbers put in: the formulas and
/// their code buffer, the aura filter, the parameters, and the speed of the
/// missile the row names.
fn with_row(
    mut t: SkillTables,
    real: &SkillTables,
    id: usize,
    missile: Option<usize>,
    lifted_start: bool,
) -> SkillTables {
    let r = &real.skills[id];
    if matches!(
        r.srvdofunc,
        6 | 8
            | 10
            | 17
            | 18
            | 34
            | 35
            | 45
            | 49
            | 56
            | 59
            | 61
            | 66
            | 68
            | 69
            | 72
            | 81
            | 114
            | 115
            | 116
            | 118
            | 119
    ) || (r.srvdofunc == 0 && r.passivestate > 0 && r.passivestate < 0x8000)
    {
        let fixture = t.skills[0].clone();
        t.skills = real.skills.clone();
        t.skilldesc = real.skilldesc.clone();
        let lifted = &mut t.skills[0];
        *lifted = r.clone();
        lifted.srvdofunc = fixture.srvdofunc;
        lifted.srvmissile = fixture.srvmissile;
        lifted.srvmissilea = fixture.srvmissilea;
        lifted.aurastate = if r.aurastate == NONE {
            NONE
        } else {
            fixture.aurastate
        };
    }
    let s = &mut t.skills[0];
    if r.srvdofunc == 0 && r.passivestate > 0 && r.passivestate < 0x8000 {
        s.passivestate = STATE;
        s.aurastate = if r.aurastate == NONE || r.aurastate == 0 {
            NONE
        } else {
            OTHER
        };
    }
    if matches!(
        r.srvdofunc,
        6 | 8 | 10 | 17 | 34 | 35 | 59 | 61 | 66 | 69 | 72 | 81 | 118
    ) {
        if s.srvdofunc != crate::rules::BLOCK_SLOT {
            s.srvdofunc = r.srvdofunc;
        }
        s.auratargetstate = if r.auratargetstate == NONE {
            NONE
        } else if r.auratargetstate == r.aurastate {
            STATE
        } else {
            OTHER
        };
    }
    if r.srvdofunc == 18 {
        s.aurastat1 = r.aurastat1;
        s.aurastat2 = r.aurastat2;
        s.aurastat3 = r.aurastat3;
        s.aurastat4 = r.aurastat4;
        s.aurastat5 = r.aurastat5;
        s.aurastat6 = r.aurastat6;
        s.aurastatcalc1 = r.aurastatcalc1;
        s.aurastatcalc2 = r.aurastatcalc2;
        s.aurastatcalc3 = r.aurastatcalc3;
        s.aurastatcalc4 = r.aurastatcalc4;
        s.aurastatcalc5 = r.aurastatcalc5;
        s.aurastatcalc6 = r.aurastatcalc6;
        s.passivestat1 = r.passivestat1;
        s.passivestat2 = r.passivestat2;
        s.passivestat3 = r.passivestat3;
        s.passivestat4 = r.passivestat4;
        s.passivestat5 = r.passivestat5;
        s.passivecalc1 = r.passivecalc1;
        s.passivecalc2 = r.passivecalc2;
        s.passivecalc3 = r.passivecalc3;
        s.passivecalc4 = r.passivecalc4;
        s.passivecalc5 = r.passivecalc5;
        s.auraevent1 = r.auraevent1;
        s.auraevent2 = r.auraevent2;
        s.auraevent3 = r.auraevent3;
        s.auraeventfunc1 = r.auraeventfunc1;
        s.auraeventfunc2 = r.auraeventfunc2;
        s.auraeventfunc3 = r.auraeventfunc3;
    }
    if r.srvdofunc == 2 {
        s.srvoverlay = r.srvoverlay;
        s.resultflags = r.resultflags;
        s.hitflags = r.hitflags;
        s.hitclass = r.hitclass;
        s.aurastat1 = r.aurastat1;
        s.aurastat2 = r.aurastat2;
        s.aurastat3 = r.aurastat3;
        s.aurastat4 = r.aurastat4;
        s.aurastat5 = r.aurastat5;
        s.aurastat6 = r.aurastat6;
        s.aurastatcalc1 = r.aurastatcalc1;
        s.aurastatcalc2 = r.aurastatcalc2;
        s.aurastatcalc3 = r.aurastatcalc3;
        s.aurastatcalc4 = r.aurastatcalc4;
        s.aurastatcalc5 = r.aurastatcalc5;
        s.aurastatcalc6 = r.aurastatcalc6;
    }
    if matches!(r.srvdofunc, 45 | 49 | 56 | 114 | 115 | 119) {
        s.summon = 0;
        s.summode = r.summode;
        s.pettype = 2;
        s.petmax = r.petmax;
    }
    if matches!(r.srvdofunc, 8 | 10 | 17 | 118) {
        s.srvmissileb = if r.srvmissileb == NONE { NONE } else { 0 };
    }
    s.delay = r.delay;
    s.lob = r.lob;
    s.progressive = r.progressive;
    s.calc1 = r.calc1;
    s.calc2 = r.calc2;
    s.calc3 = r.calc3;
    // A lifted row that drops the start function keeps it dropped.
    if !lifted_start {
        s.srvstfunc = r.srvstfunc;
    }
    // The fixture has one missile row: the row's second missile is that one or none.
    if matches!(r.srvdofunc, 8 | 10 | 24 | 118) && r.srvmissileb == NONE {
        s.srvmissileb = NONE;
    }
    s.tohitcalc = r.tohitcalc;
    s.tohit = r.tohit;
    s.levtohit = r.levtohit;
    s.srcdam = r.srcdam;
    s.aurafilter = r.aurafilter;
    // The state for the units around: none, the caster's own, or another.
    s.auratargetstate = match r.auratargetstate {
        NONE => NONE,
        t if t == r.aurastate => STATE,
        _ => OTHER,
    };
    s.auralencalc = r.auralencalc;
    s.aurarangecalc = r.aurarangecalc;
    s.param1 = r.param1;
    s.param2 = r.param2;
    s.param3 = r.param3;
    s.param4 = r.param4;
    s.param5 = r.param5;
    s.param6 = r.param6;
    s.param7 = r.param7;
    s.param8 = r.param8;
    t.skills_code = real.skills_code.clone();
    if let Some(m) = missile.and_then(|m| real.missiles.get(m)) {
        t.missiles[0].vel = m.vel;
        t.missiles[0].vellev = m.vellev;
    }
    t
}

/// What became of the rows of one family.
#[derive(Default)]
struct Tally {
    compared: u32,
    same: u32,
    /// `(row, what differs)`.
    flagged: Vec<(usize, String)>,
    /// Why rows were left out, with how many.
    left_out: BTreeMap<String, u32>,
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    let text = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("it panicked");
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect()
}

/// The rows of `real` that `keep` accepts: the markdown tables of what became of
/// them, and for the rows the lifter does not know, how many skills use each do
/// function.
fn report_rows(
    real: &SkillTables,
    keep: &dyn Fn(usize) -> bool,
) -> (String, BTreeMap<&'static str, Tally>, BTreeMap<u16, u32>) {
    let mut by_family: BTreeMap<&'static str, Tally> = BTreeMap::new();
    let mut unlifted: BTreeMap<u16, u32> = BTreeMap::new();
    for id in (0..real.skills.len()).filter(|&id| keep(id)) {
        let src = Real {
            t: real,
            id,
            blocked: RefCell::default(),
            missile: RefCell::default(),
        };
        // The fixture's one skill is row 0, whatever row of the install it stands for.
        let lifted = match lift(0, &src) {
            Ok(l) => l,
            Err(LiftError::Unsupported(slot)) => {
                *unlifted.entry(slot).or_default() += 1;
                continue;
            }
            Err(e) => {
                let tally = by_family.entry("lifter").or_default();
                *tally.left_out.entry(e.to_string()).or_default() += 1;
                continue;
            }
        };
        let family = lifted.family.name();
        let tally = by_family.entry(family).or_default();
        if let Some(why) = src.blocked.borrow().first() {
            *tally.left_out.entry(why.clone()).or_default() += 1;
            continue;
        }
        let Some(case) = CASES.iter().find(|c| c.name == family) else {
            *tally
                .left_out
                .entry(format!("needs a generated {family} fixture"))
                .or_default() += 1;
            continue;
        };
        let missile = *src.missile.borrow();
        let ran = catch_unwind(AssertUnwindSafe(|| {
            let rust = run(
                case,
                None,
                with_row(tables(case, None), real, id, missile, false),
            );
            let data = run(
                case,
                Some(&lifted.toml),
                with_row(
                    tables(case, Some(&lifted.cells)),
                    real,
                    id,
                    missile,
                    lifted.cells.iter().any(|(c, _)| *c == "srvstfunc"),
                ),
            );
            (rust, data)
        }));
        let (rust, data): (Outcome, Outcome) = match ran {
            Ok(pair) if unfit(case.name, &pair.0).is_some() => {
                let why = unfit(case.name, &pair.0).unwrap_or_default();
                *tally.left_out.entry(why.to_owned()).or_default() += 1;
                continue;
            }
            Ok(pair) => pair,
            Err(p) => {
                let why = format!("the fixture could not run it: {}", panic_text(&*p));
                *tally.left_out.entry(why).or_default() += 1;
                continue;
            }
        };
        tally.compared += 1;
        let mut differs: Vec<String> = Vec::new();
        if rust.results != data.results {
            differs.push(format!("cast {:?} / {:?}", rust.results, data.results));
        }
        for (metric, a, b) in metrics(&rust, &data) {
            if flagged(a, b) {
                differs.push(format!("{metric} {a} / {b}"));
            }
        }
        if differs.is_empty() {
            tally.same += 1;
        } else {
            tally.flagged.push((id, differs.join(", ")));
        }
    }
    let mut text = String::from(
        "# Closeness report on the install's own skills rows\n\n\
         Each row of the install's skills table that the lifter knows is lifted from the \
         row's numbers and formulas and run both ways (the engine's body, then the data \
         copy) at the same level on the report's fixture, whose one missile and one \
         state stand for the row's own. Only row numbers and counts appear here. A row \
         is flagged when a metric differs by more than 25% (`Rust / data`) or when only \
         one of the two casts.\n\n\
         | family | compared | same | flagged | left out |\n| --- | ---: | ---: | ---: | ---: |\n",
    );
    for (family, t) in &by_family {
        let out: u32 = t.left_out.values().sum();
        let _ = writeln!(
            text,
            "| {family} | {} | {} | {} | {out} |",
            t.compared,
            t.same,
            t.flagged.len()
        );
    }
    for (family, t) in &by_family {
        if !t.left_out.is_empty() {
            let _ = writeln!(text, "\n## {family}: rows left out\n");
            for (why, n) in &t.left_out {
                let _ = writeln!(text, "- {n} × {why}");
            }
        }
        if !t.flagged.is_empty() {
            let _ = writeln!(text, "\n## {family}: flagged rows\n");
            for (row, what) in &t.flagged {
                let _ = writeln!(text, "- row {row}: {what}");
            }
        }
    }
    (text, by_family, unlifted)
}

/// The report over all rows, then over the skills some monster has, with the
/// do functions of those that are not lifted yet in the order of how many
/// monster rows use them.
fn report(
    real: &SkillTables,
    used: &BTreeMap<usize, u32>,
) -> (String, BTreeMap<&'static str, Tally>) {
    let (mut text, by_family, _) = report_rows(real, &|_| true);
    let (monster_text, _, unlifted) = report_rows(real, &|id| used.contains_key(&id));
    text.push_str(
        "
# The skills some monster has

",
    );
    // The same tables, cut to the rows a monstats row names.
    text.push_str(
        &monster_text
            .replacen(
                "# Closeness report on the install's own skills rows",
                "## Monster skills",
                1,
            )
            .replace(
                "
## ", "
### ",
            ),
    );
    let mut next: BTreeMap<u16, (u32, u32)> = BTreeMap::new();
    for (&id, &monsters) in used {
        let Some(r) = real.skills.get(id) else {
            continue;
        };
        if let Some(count) = unlifted.get(&r.srvdofunc) {
            let _ = count;
            let e = next.entry(r.srvdofunc).or_default();
            e.0 += 1;
            e.1 += monsters;
        }
    }
    let mut next: Vec<(u16, (u32, u32))> = next.into_iter().collect();
    next.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then(a.0.cmp(&b.0)));
    text.push_str(
        "
## Monster skills not lifted yet

By do function: how many skills, and how many monster rows name them.

",
    );
    for (slot, (skills, monsters)) in next.iter().take(12) {
        let _ = writeln!(
            text,
            "- do function {slot}: {skills} skills, {monsters} monster rows"
        );
    }
    (text, by_family)
}

#[test]
#[ignore = "needs the game tables in D2_GAME_DIR"]
fn the_closeness_report_runs_on_the_installs_own_rows() {
    let (real, used) = install();
    let (text, by_family) = report(&real, &used);
    if let Some(path) = std::env::var_os("D2_CLOSENESS_REAL_REPORT") {
        std::fs::write(path, &text).expect("the report file");
    }
    // The install has rows of the lifted families, and the fixture runs some.
    assert!(by_family.values().any(|t| t.compared > 0), "{text}");
    assert_eq!(text, report(&real, &used).0);
}

#[test]
fn a_formula_is_read_back_from_its_code() {
    let lvl = SKILLCALC_CODES.iter().position(|c| *c == "lvl").unwrap() as u8;
    let par1 = SKILLCALC_CODES.iter().position(|c| *c == "par1").unwrap() as u8;
    let ln12 = SKILLCALC_CODES.iter().position(|c| *c == "ln12").unwrap() as u8;
    let edmn = SKILLCALC_CODES.iter().position(|c| *c == "edmn").unwrap() as u8;
    // `lvl * 3 + par1`, then `10 - 25`, then `ln12`, then something it cannot say.
    let code = [
        0x04, lvl, 0x07, 3, 0x12, 0x04, par1, 0x10, 0x00, // 0
        0x07, 10, 0x07, 25, 0x11, 0x00, // 9
        0x04, ln12, 0x00, // 15
        0x07, 7, 0x07, 2, 0x14, 0x00, // 18
        0x04, edmn, 0x00, // 24
    ];
    assert_eq!(decompile(&code, 0).unwrap(), "((lvl * 3) + par1)");
    assert_eq!(decompile(&code, 9).unwrap(), "(10 - 25)");
    assert_eq!(decompile(&code, u32::MAX).unwrap(), "");
    assert_eq!(decompile(&code, 15).unwrap(), "ln12");
    assert_eq!(decompile(&code, 18).unwrap(), "(7 ^ 2)");
    assert_eq!(decompile(&code, 24).unwrap(), "edmn");
    assert!(decompile(&[0xff], 0).is_err());
    assert!(decompile(&[4, 127, 0], 0).is_err());
    // Every name it reads back is one the data formulas compile.
    for name in SKILLCALC_CODES.iter() {
        assert!(
            crate::rules::data_mod::Formula::compile(name).is_ok(),
            "{name}"
        );
    }
    assert_eq!(SKILLCALC_CODES.len(), 73);
    assert!(decompile(&code, 99).is_err());
}

#[test]
fn a_lifted_sequence_without_a_fixture_is_reported_as_needing_one() {
    let case = CASES.iter().find(|c| c.name == "jab").unwrap();
    let mut t = tables(case, None);
    t.skills.truncate(1);
    t.skills[0].srvdofunc = 13;
    t.skills[0].srvstfunc = 9;
    let (_, families, unsupported) = report_rows(&t, &|_| true);
    assert!(unsupported.is_empty());
    let sequence = &families["sequence"];
    assert_eq!(sequence.compared, 0);
    assert_eq!(sequence.left_out["needs a generated sequence fixture"], 1);
}

#[test]
fn pure_and_linked_function_calls_are_read_back_with_their_ids() {
    let min = [0x07, 3, 0x07, 7, 0x01, 0, 0];
    assert_eq!(decompile(&min, 0).unwrap(), "min(3, 7)");
    let max = [0x07, 3, 0x07, 7, 0x01, 1, 0];
    assert_eq!(decompile(&max, 0).unwrap(), "max(3, 7)");
    let linked = [0x07, 3, 0x07, 7, 0x01, 3, 0];
    assert_eq!(decompile(&linked, 0).unwrap(), "skill(3, 7)");
}

#[test]
fn warcry_fixture_preserves_absent_caster_state() {
    let case = CASES.iter().find(|case| case.name == "warcry").unwrap();
    let fixture = tables(case, None);
    let mut real = fixture.clone();
    real.skills[0].aurastate = NONE;
    let rust_tables = with_row(fixture.clone(), &real, 0, Some(0), false);
    assert_eq!(rust_tables.skills[0].aurastate, NONE);
    let src = Real {
        t: &real,
        id: 0,
        blocked: RefCell::default(),
        missile: RefCell::default(),
    };
    let lifted = lift(0, &src).unwrap();
    let rust = run(case, None, rust_tables);
    let data = run(
        case,
        Some(&lifted.toml),
        with_row(tables(case, Some(&lifted.cells)), &real, 0, Some(0), false),
    );
    assert_eq!(rust, data);
    assert_eq!(rust.affected, 0);
    assert!(rust.missiles >= 64);
}
