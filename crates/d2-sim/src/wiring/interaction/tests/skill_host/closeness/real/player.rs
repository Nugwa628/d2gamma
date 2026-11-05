//! Per-class install reports use the same seeded SK2 comparison as the
//! all-row report. Only row identifiers, counts and missing mechanisms are
//! written; no install table or generated game assets are checked in.

use super::*;
use crate::rules::data_mod::lift::player::{needs_part, PlayerClass, CLASSES};

fn class_report(real: &SkillTables, class: PlayerClass) -> String {
    let keep = |id: usize| real.skills[id].charclass == class.id;
    let (summary, by_family, _) = report_rows(real, &keep);
    let rows = real
        .skills
        .iter()
        .filter(|r| r.charclass == class.id)
        .count();
    let compared: u32 = by_family.values().map(|t| t.compared).sum();
    let same: u32 = by_family.values().map(|t| t.same).sum();
    let mut text = format!(
        "# SK2: {}\n\n| class | rows | lifted/compared | same | flagged | needs part/fixture |\n| --- | ---: | ---: | ---: | ---: | ---: |\n| {} | {rows} | {compared} | {same} | {} | {} |\n\n\
         Same means no flagged metric in the SK2 fixture, not full game parity. \
         The fixture substitutes one missile/state for the install identifiers; \
         damage and weapon fidelity are limited. Rust bodies remain the default. \
         Unsupported rows remain untouched.\n\n\
         | skill row | do function | result |\n| ---: | ---: | --- |\n",
        class.name, class.name, compared - same, rows as u32 - compared,
    );
    for id in (0..real.skills.len()).filter(|&id| keep(id)) {
        let (_, families, unsupported) = report_rows(real, &|row| row == id);
        let slot = real.skills[id].srvdofunc;
        let result = if unsupported.contains_key(&slot) {
            format!("needs part {}", needs_part(slot))
        } else if let Some(tally) = families.values().find(|t| t.compared > 0) {
            if tally.same > 0 {
                "lifted: same in fixture".to_owned()
            } else {
                format!("lifted: flagged {}", tally.flagged[0].1)
            }
        } else {
            let reasons: Vec<_> = families
                .values()
                .flat_map(|t| t.left_out.keys())
                .cloned()
                .collect();
            format!("needs formula/fixture support: {}", reasons.join("; "))
        };
        let _ = writeln!(text, "| {id} | {slot} | {} |", result.replace('|', "/"));
    }
    text.push('\n');
    text.push_str(&summary);
    text
}

#[test]
#[ignore = "needs the game tables in D2_GAME_DIR"]
fn the_player_class_closeness_report_runs_on_install_rows() {
    let code = std::env::var("D2_PLAYER_CLASS").expect("D2_PLAYER_CLASS class code");
    let class = CLASSES
        .iter()
        .find(|c| c.code == code)
        .copied()
        .expect("enabled class pack");
    let (real, _) = install();
    let text = class_report(&real, class);
    assert!(real.skills.iter().any(|r| r.charclass == class.id));
    assert_eq!(
        text,
        class_report(&real, class),
        "seeded report is deterministic"
    );
    if let Some(path) = std::env::var_os("D2_PLAYER_REPORT") {
        std::fs::write(path, &text).expect("class report file");
    }
}
