//! The quest editor: a list of quests and a form of objective and reward
//! blocks, written to `files/quests.toml`.
use super::{drag, labelled, list_header, move_buttons, pick_field, RED};
use crate::kit::*;
use crate::quest_panel;
use d2_sim::rules::data_mod::blocks::{self, Family};
use d2_sim::rules::data_mod::{Objective, Reward};
use eframe::egui::{self, RichText};

pub(super) fn list(ui: &mut egui::Ui, kit: &mut Kit, names: &Names, sel: &mut usize) -> bool {
    list_header(ui, "Quests");
    let mut changed = false;
    let len = kit.quests.len();
    let mut swap = None;
    let mut remove = None;
    for i in 0..len {
        let line = kit.quests[i].summary(names);
        ui.horizontal(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.label(RichText::new(format!("{}.", kit.quests[i].id)).weak());
            if ui.selectable_label(*sel == i, line).clicked() {
                *sel = i;
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let (to, rm) = move_buttons(ui, i, len);
            if let Some(to) = to {
                swap = Some((i, to));
            }
            if rm {
                remove = Some(i);
            }
        });
    }
    if let Some((a, b)) = swap {
        kit.quests.swap(a, b);
        *sel = b;
        changed = true;
    }
    if let Some(i) = remove {
        kit.quests.remove(i);
        *sel = sel.saturating_sub(1);
        changed = true;
    }
    if ui.button("Add quest").clicked() {
        let id = QuestEntry::free_id(&kit.quests);
        kit.quests.push(QuestEntry::new(id, "New quest"));
        *sel = kit.quests.len() - 1;
        changed = true;
    }
    panel_section(ui, kit, names);
    changed
}

/// Writes the quest log panel from the quests as they are on screen.
fn panel_section(ui: &mut egui::Ui, kit: &Kit, names: &Names) {
    ui.separator();
    ui.strong("Quest log panel");
    let (used, cap) = quest_panel::variable_use(&kit.quests);
    let line = format!(
        "{used} panel variables with every quest begun (up to {cap} are published per player)"
    );
    if used > cap {
        ui.colored_label(RED, line);
    } else {
        ui.weak(line);
    }
    let id = egui::Id::new("quest_panel_message");
    if ui
        .button("Write quest log panel")
        .on_hover_text("Writes files/ui/quest_log.toml, which replaces the game's quest log with these quests and their progress.")
        .clicked()
    {
        let message = match quest_panel::write(&kit.root, &kit.quests, names) {
            Ok(made) if made.left_out == 0 => format!("Wrote {}.", quest_panel::PANEL_FILE),
            Ok(made) => format!(
                "Wrote {}; {} quest(s) did not fit and are left out.",
                quest_panel::PANEL_FILE,
                made.left_out
            ),
            Err(e) => format!("{e:#}"),
        };
        ui.ctx().data_mut(|d| d.insert_temp(id, message));
    }
    if let Some(message) = ui.ctx().data(|d| d.get_temp::<String>(id)) {
        ui.label(message);
    }
}

pub(super) fn form(ui: &mut egui::Ui, kit: &mut Kit, names: &Names, i: usize) -> bool {
    let others: Vec<u16> = kit
        .quests
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != i)
        .map(|(_, q)| q.id)
        .collect();
    let Some(q) = kit.quests.get_mut(i) else {
        ui.label("Add a quest, or pick one from the list.");
        return false;
    };
    let mut c = false;
    ui.heading(&q.name);
    c |= labelled(ui, "Id", |ui| drag(ui, &mut q.id, 1..=u16::MAX));
    if others.contains(&q.id) {
        ui.colored_label(RED, "Another quest has this id.");
    }
    c |= labelled(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut q.name).desired_width(260.0))
            .changed()
    });
    c |= labelled(ui, "Log text", |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut q.text)
                .desired_width(360.0)
                .desired_rows(2),
        )
        .changed()
    });
    c |= labelled(ui, "Log text when done", |ui| {
        ui.add(egui::TextEdit::singleline(&mut q.done_text).desired_width(360.0))
            .changed()
    });

    // The NPC that gives the quest.
    let mut has_giver = q.giver.is_some();
    if labelled(ui, "Given by an NPC", |ui| {
        ui.checkbox(&mut has_giver, "").changed()
    }) {
        q.giver = has_giver.then_some(0);
        c = true;
    }
    if let Some(g) = &mut q.giver {
        c |= pick_field(ui, "NPC", g, &names.monsters);
    } else {
        ui.weak("Without an NPC the quest starts as soon as its required quests are done.");
    }

    ui.separator();
    ui.strong("Quests to finish first");
    let mut gone = None;
    for (k, r) in q.requires.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            c |= drag(ui, r, 1..=u16::MAX);
            if !others.contains(r) {
                ui.colored_label(RED, "no such quest");
            }
            if ui.small_button("Remove").clicked() {
                gone = Some(k);
            }
        });
    }
    if let Some(k) = gone {
        q.requires.remove(k);
        c = true;
    }
    if ui.small_button("Add a required quest").clicked() {
        q.requires.push(others.first().copied().unwrap_or(1));
        c = true;
    }

    ui.separator();
    ui.strong("Objectives (all of them, to finish)");
    c |= objectives(ui, &mut q.objectives, names);
    ui.separator();
    ui.strong("Rewards");
    c |= rewards(ui, &mut q.rewards, names);
    c
}

fn objectives(ui: &mut egui::Ui, list: &mut Vec<Objective>, names: &Names) -> bool {
    let mut c = false;
    let len = list.len();
    let (mut swap, mut remove) = (None, None);
    for (k, o) in list.iter_mut().enumerate() {
        ui.push_id(("objective", k), |ui| {
            ui.group(|ui| {
                ui.label(RichText::new(objective_text(names, o)).strong());
                ui.weak(blocks::find(Family::Objective, objective_kind(o)).map_or("", |b| b.about));
                match o {
                    Objective::Kill { monster, count } => {
                        c |= pick_field(ui, "Monster", monster, &names.monsters);
                        c |= labelled(ui, "How many", |ui| drag(ui, count, 1..=10_000));
                    }
                    Objective::Enter { level } => {
                        c |= labelled(ui, "Level", |ui| drag(ui, level, 0..=u32::MAX));
                    }
                    Objective::Talk { npc } => {
                        c |= pick_field(ui, "NPC", npc, &names.monsters);
                    }
                    Objective::UseSkill { skill, count } => {
                        c |= pick_field(ui, "Skill", skill, &names.skills);
                        c |= labelled(ui, "How many times", |ui| drag(ui, count, 1..=10_000));
                    }
                    Objective::ReachStat { stat, value } => {
                        c |= pick_field(ui, "Stat", stat, &names.stats);
                        c |= labelled(ui, "Value to reach", |ui| drag(ui, value, 1..=i32::MAX));
                    }
                    Objective::Collect {
                        item,
                        count,
                        consume,
                    } => {
                        c |= item_field(ui, item);
                        c |= labelled(ui, "How many", |ui| drag(ui, count, 1..=10_000));
                        c |= ui
                            .checkbox(consume, "Take the items when the quest is completed")
                            .changed();
                    }
                }
                let (to, rm) = move_buttons(ui, k, len);
                swap = swap.or(to.map(|t| (k, t)));
                if rm {
                    remove = Some(k);
                }
            });
        });
    }
    if let Some((a, b)) = swap {
        list.swap(a, b);
        c = true;
    }
    if let Some(k) = remove {
        list.remove(k);
        c = true;
    }
    ui.horizontal_wrapped(|ui| {
        for b in blocks::family(Family::Objective) {
            if ui
                .button(format!("Add: {}", b.label))
                .on_hover_text(b.about)
                .clicked()
            {
                list.extend(new_objective(b.name));
                c = true;
            }
        }
    });
    c
}

fn rewards(ui: &mut egui::Ui, list: &mut Vec<Reward>, names: &Names) -> bool {
    let mut c = false;
    let len = list.len();
    let (mut swap, mut remove) = (None, None);
    for (k, r) in list.iter_mut().enumerate() {
        ui.push_id(("reward", k), |ui| {
            ui.group(|ui| {
                ui.label(RichText::new(reward_text(names, r)).strong());
                ui.weak(blocks::find(Family::Reward, reward_kind(r)).map_or("", |b| b.about));
                match r {
                    Reward::Stat { stat, value } => {
                        c |= pick_field(ui, "Stat", stat, &names.stats);
                        c |= labelled(ui, "Add", |ui| drag(ui, value, i32::MIN..=i32::MAX));
                    }
                    Reward::Item { item, count } => {
                        c |= item_field(ui, item);
                        c |= labelled(ui, "How many", |ui| drag(ui, count, 1..=20));
                    }
                    Reward::Message { text } => {
                        c |= labelled(ui, "Line", |ui| {
                            ui.add(egui::TextEdit::singleline(text).desired_width(360.0))
                                .changed()
                        });
                    }
                }
                let (to, rm) = move_buttons(ui, k, len);
                swap = swap.or(to.map(|t| (k, t)));
                if rm {
                    remove = Some(k);
                }
            });
        });
    }
    if let Some((a, b)) = swap {
        list.swap(a, b);
        c = true;
    }
    if let Some(k) = remove {
        list.remove(k);
        c = true;
    }
    ui.horizontal_wrapped(|ui| {
        for b in blocks::family(Family::Reward) {
            if ui
                .button(format!("Add: {}", b.label))
                .on_hover_text(b.about)
                .clicked()
            {
                list.extend(new_reward(b.name));
                c = true;
            }
        }
    });
    c
}

/// The code of an item, typed (the editor has no item tables to pick from).
fn item_field(ui: &mut egui::Ui, code: &mut String) -> bool {
    let c = labelled(ui, "Item code", |ui| {
        ui.add(
            egui::TextEdit::singleline(code)
                .desired_width(60.0)
                .char_limit(4),
        )
        .changed()
    });
    if code.trim().is_empty() {
        ui.colored_label(RED, "The item needs a code, like hp1.");
    }
    c
}

#[cfg(test)]
mod palette_tests {
    use super::*;
    #[test]
    fn every_objective_and_reward_fits_a_normal_editor_pane() {
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(540., 400.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    objectives(ui, &mut Vec::new(), &Names::default());
                    rewards(ui, &mut Vec::new(), &Names::default());
                });
                let painted = crate::input_script::painted_text(ctx);
                for family in [Family::Objective, Family::Reward] {
                    for block in blocks::family(family) {
                        let label = format!("Add: {}", block.label);
                        let target = painted
                            .iter()
                            .find(|p| p.text == label)
                            .unwrap_or_else(|| panic!("{label} is clipped"));
                        assert!(
                            target.bounds[0] >= 0. && target.bounds[2] <= 540.,
                            "{label}: {:?}",
                            target.bounds
                        );
                    }
                }
            },
        );
    }
}
