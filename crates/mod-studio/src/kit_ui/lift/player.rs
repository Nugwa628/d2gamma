//! Copy a class's supported skills into the editable kit, using the install
//! reference rather than bundled table cells. Existing authored skills and
//! unsupported rows stay untouched, with a reason for every skipped row.

use super::{copy_base, lift_row, table, Row};
use crate::{kit::Kit, tables::Tables};
use d2_sim::rules::data_mod::lift::{player::needs_part, player::PlayerClass, Source};

/// Copy packs validated class by class. The engine catalog also identifies
/// classes whose pack has not been enabled yet.
pub const CLASS_PACKS: &[&str] = &["ama", "sor", "nec", "pal", "bar", "dru", "ass"];

pub fn classes() -> impl Iterator<Item = PlayerClass> {
    d2_sim::rules::data_mod::lift::player::CLASSES
        .iter()
        .copied()
        .filter(|c| CLASS_PACKS.contains(&c.code))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillStatus {
    pub id: u16,
    pub name: String,
    /// None means that the row can be copied.
    pub reason: Option<String>,
}

pub fn skills(t: &Tables, class: PlayerClass) -> Vec<SkillStatus> {
    let Some(skills) = table(t.reference(), "skills") else {
        return Vec::new();
    };
    (0..skills.rows.len().min(usize::from(u16::MAX) + 1))
        .filter_map(|id| {
            let row = Row {
                data: t.reference(),
                row: id,
            };
            if !row.skill("charclass").eq_ignore_ascii_case(class.code) {
                return None;
            }
            let reason = lift_row(t, id as u16).err().map(|e| {
                let slot = row.skill("srvdofunc").parse().unwrap_or(0);
                format!("needs part {} (do {slot}): {e}", needs_part(slot))
            });
            Some(SkillStatus {
                id: id as u16,
                name: row.skill("skill"),
                reason,
            })
        })
        .collect()
}

#[derive(Default, Debug)]
pub struct Copied {
    pub ids: Vec<u16>,
    pub skipped: Vec<SkillStatus>,
    pub notes: Vec<String>,
}

pub fn copy(kit: &mut Kit, t: &mut Tables, class: PlayerClass) -> Copied {
    let mut result = Copied::default();
    for mut row in skills(t, class) {
        if row.reason.is_none() && kit.skills.iter().any(|s| s.id == row.id) {
            row.reason = Some("already authored; kept existing skill".to_owned());
        }
        if row.reason.is_none() {
            match copy_base(kit, t, row.id) {
                Ok(lifted) => {
                    result.ids.push(row.id);
                    result.notes.extend(lifted.notes);
                    continue;
                }
                Err(e) => row.reason = Some(e.to_string()),
            }
        }
        result.skipped.push(row);
    }
    result
}

pub fn panel(
    ui: &mut eframe::egui::Ui,
    kit: &mut Kit,
    t: &mut Tables,
    selected: &mut usize,
    message: &mut String,
) -> bool {
    let mut changed = false;
    eframe::egui::CollapsingHeader::new("Copy a player class")
        .id_salt("copy-player-class")
        .show(ui, |ui| {
            for class in classes() {
                let rows = skills(t, class);
                let available = rows
                    .iter()
                    .filter(|r| r.reason.is_none() && !kit.skills.iter().any(|s| s.id == r.id))
                    .count();
                if ui
                    .add_enabled(
                        available > 0,
                        eframe::egui::Button::new(format!("Copy {} ({available})", class.name)),
                    )
                    .clicked()
                {
                    let copied = copy(kit, t, class);
                    changed = !copied.ids.is_empty();
                    if changed {
                        *selected = kit.skills.len() - 1;
                    }
                    *message = format!(
                        "{}: copied {}, kept/skipped {}. {}",
                        class.name,
                        copied.ids.len(),
                        copied.skipped.len(),
                        copied.notes.join(" ")
                    );
                }
                ui.collapsing(format!("{}: skills and missing parts", class.name), |ui| {
                    for row in rows {
                        let status = row.reason.as_deref().unwrap_or("ready to copy as parts");
                        ui.label(format!("{} · {}: {status}", row.id, row.name));
                    }
                });
            }
        });
    changed
}
