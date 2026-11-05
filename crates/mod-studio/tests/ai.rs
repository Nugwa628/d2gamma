//! The monster AI editor reads and writes the file the engine reads.
use d2_sim::rules::data_mod::ai::{ACTIONS, CONDITIONS};
use d2_sim::rules::data_mod::AiRules;
use mod_studio::ai::*;
use mod_studio::arena_ui::ArenaTools;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

const TEMPLATE: &str = include_str!("../../../templates/monster-ai-graph/files/ai.toml");

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "mod-studio-ai-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join("files")).unwrap();
        Self(p)
    }
    fn write(&self, rel: &str, text: &str) {
        fs::write(self.0.join(rel), text).unwrap();
    }
    fn template() -> Self {
        let t = Self::new();
        t.write(AI_FILE, TEMPLATE);
        t
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn graph(file: &AiFile) -> &Entry {
    file.entries.iter().find(|e| e.preset.is_none()).unwrap()
}

#[test]
fn the_template_round_trips_to_what_the_engine_reads() {
    let temp = Temp::template();
    let before = AiRules::from_toml(TEMPLATE).unwrap();
    let mut file = AiFile::load(&temp.0).unwrap();
    assert_eq!(file.entries.len(), 2);
    file.dirty = true;
    file.save().unwrap();
    let text = fs::read_to_string(temp.0.join(AI_FILE)).unwrap();
    assert_eq!(AiRules::from_toml(&text).unwrap(), before);
    // The editor reads its own output back to the same entries.
    assert_eq!(AiFile::load(&temp.0).unwrap().entries, file.entries);
    assert!(file.problems().is_empty(), "{:?}", file.problems());
    // And writing is stable: a second save changes nothing.
    assert_eq!(file.text(), text);
}

#[test]
fn a_file_the_editor_cannot_hold_is_not_overwritten() {
    let temp = Temp::new();
    temp.write(AI_FILE, "version = 1\nmystery = 3\n");
    let e = AiFile::load(&temp.0).unwrap_err();
    assert!(format!("{e:#}").contains("mystery"), "{e:#}");
    temp.write(
        AI_FILE,
        "[[ai]]\nindex = 5\n[[ai.state]]\nname = \"a\"\nspeed = 3\n",
    );
    assert!(AiFile::load(&temp.0).is_err());
    assert!(fs::read_to_string(temp.0.join(AI_FILE))
        .unwrap()
        .contains("speed"));
}

#[test]
fn saving_refuses_a_file_changed_outside() {
    let temp = Temp::template();
    let mut file = AiFile::load(&temp.0).unwrap();
    temp.write(AI_FILE, "version = 1\n");
    file.dirty = true;
    let e = file.save().unwrap_err().to_string();
    assert!(e.contains("changed outside Studio"), "{e}");
}

#[test]
fn an_empty_project_writes_nothing_until_there_is_something() {
    let temp = Temp::new();
    let mut file = AiFile::load(&temp.0).unwrap();
    assert!(file.entries.is_empty());
    file.save().unwrap();
    assert!(!temp.0.join(AI_FILE).exists());
    file.entries
        .push(Entry::new_graph(file.next_index(), "Fresh"));
    file.save().unwrap();
    assert!(AiRules::from_toml(&fs::read_to_string(temp.0.join(AI_FILE)).unwrap()).is_ok());
}

#[test]
fn every_condition_and_action_in_the_lists_is_accepted_by_the_engine() {
    let mut file = AiFile::default();
    let mut e = Entry::new_graph(300, "All kinds");
    e.cooldowns.push(Cooldown {
        name: "c".into(),
        seconds: seconds(2.0),
    });
    e.states.push(State {
        name: "b".into(),
        transitions: Vec::new(),
        actions: Vec::new(),
    });
    let cooldowns = e.cooldown_names();
    // A condition list holds at most six.
    for chunk in CONDITIONS.chunks(6) {
        e.states[0].transitions.push(Transition {
            to: "b".into(),
            when: chunk.iter().map(|k| Item::new(k, &cooldowns)).collect(),
        });
    }
    // A state holds at most eight actions.
    for (n, chunk) in ACTIONS.chunks(6).enumerate() {
        e.states.push(State {
            name: format!("c{n}"),
            transitions: Vec::new(),
            actions: chunk
                .iter()
                .map(|k| Action {
                    item: Item::new(k, &cooldowns),
                    when: Vec::new(),
                    starts_cooldown: None,
                })
                .collect(),
        });
    }
    file.entries.push(e);
    assert_eq!(file.reads(), Ok(()));
    // And the text reads back to the same entries.
    assert_eq!(parse(&file.text()).unwrap(), file.entries);
}

#[test]
fn renaming_a_state_follows_every_reference() {
    let temp = Temp::template();
    let mut file = AiFile::load(&temp.0).unwrap();
    let e = &mut file.entries[0];
    e.rename_state(0, "hunt");
    assert_eq!(e.start.as_deref(), Some("hunt"));
    assert!(e
        .states
        .iter()
        .flat_map(|s| &s.transitions)
        .any(|t| t.to == "hunt"));
    assert!(!e
        .states
        .iter()
        .flat_map(|s| &s.transitions)
        .any(|t| t.to == "stalk"));
    file.dirty = true;
    assert!(file.problems().is_empty(), "{:?}", file.problems());
}

#[test]
fn deleting_a_state_removes_what_led_to_it() {
    let temp = Temp::template();
    let mut file = AiFile::load(&temp.0).unwrap();
    let e = &mut file.entries[0];
    let flee = e.states.iter().position(|s| s.name == "flee").unwrap();
    e.delete_state(flee);
    assert!(!e
        .states
        .iter()
        .flat_map(|s| &s.transitions)
        .any(|t| t.to == "flee"));
    assert!(file.problems().is_empty(), "{:?}", file.problems());
}

#[test]
fn renaming_and_deleting_a_cooldown_follow_its_uses() {
    let temp = Temp::template();
    let mut file = AiFile::load(&temp.0).unwrap();
    file.entries[0].rename_cooldown(0, "pounce");
    assert!(file.text().contains("name = \"pounce\""));
    assert!(file.text().contains("starts_cooldown = \"pounce\""));
    assert!(file.problems().is_empty(), "{:?}", file.problems());
    file.entries[0].delete_cooldown(0);
    assert!(!file.text().contains("pounce"));
    assert!(file.problems().is_empty(), "{:?}", file.problems());
}

#[test]
fn a_mistake_is_told_with_the_place_it_is_at() {
    let temp = Temp::template();
    let mut file = AiFile::load(&temp.0).unwrap();
    file.entries[0].states[1].transitions[0].to = "nowhere".into();
    let problems = file.problems();
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].message.contains("nowhere"),
        "{}",
        problems[0].message
    );
    assert_eq!(
        problems[0].at,
        Some(At {
            entry: 0,
            state: Some(1),
            phase: None
        })
    );
}

#[test]
fn a_phase_that_wants_immunity_needs_the_mods_state() {
    let temp = Temp::template();
    let mut file = AiFile::load(&temp.0).unwrap();
    let e = &mut file.entries[0];
    e.phases.push(Phase {
        name: "enrage".into(),
        when: vec![Item {
            kind: "life_below".into(),
            keys: [("percent".to_owned(), toml::Value::Integer(50))].into(),
        }],
        state: Some("fight".into()),
        graph: None,
        immune_seconds: Some(seconds(3.0)),
    });
    let problems = file.problems();
    assert!(
        problems[0].message.contains("immune_state"),
        "{}",
        problems[0].message
    );
    assert_eq!(
        problems[0].at.map(|a| (a.entry, a.phase)),
        Some((0, Some(0)))
    );
    temp.write(
        "files/crowd_control.toml",
        "version = 1\nimmune_state = 900\n",
    );
    assert!(file.problems().is_empty(), "{:?}", file.problems());
}

#[test]
fn a_preset_entry_keeps_its_parameters() {
    let temp = Temp::template();
    let file = AiFile::load(&temp.0).unwrap();
    let p = &file.entries[1];
    assert_eq!(p.preset.as_deref(), Some("Zombie"));
    assert_eq!(
        p.params,
        vec![
            ("approach".to_owned(), Param::All(70)),
            ("att1_att2".to_owned(), Param::All(60)),
            ("aware_dist".to_owned(), Param::Each([8, 12, 16])),
        ]
    );
    let mut added = AiFile::default();
    added.entries.push(Entry::new_preset(310, "Skeleton"));
    assert_eq!(added.reads(), Ok(()));
}

#[test]
fn locating_reads_the_engines_paths() {
    assert_eq!(
        locate("invalid ai.toml setting: ai[2].state[1].action[0].slot: 1 to 8"),
        Some(At {
            entry: 2,
            state: Some(1),
            phase: None
        })
    );
    assert_eq!(
        locate("ai[0].phase[3].graph"),
        Some(At {
            entry: 0,
            state: None,
            phase: Some(3)
        })
    );
    assert_eq!(locate("version is 2"), None);
}

#[test]
fn the_layout_puts_the_start_first_and_unreached_states_last() {
    let temp = Temp::template();
    let mut file = AiFile::load(&temp.0).unwrap();
    let e = &mut file.entries[0];
    // stalk -> fight, flee; so fight and flee are one step on.
    assert_eq!(layout(e), vec![(0, 0), (1, 0), (1, 1)]);
    e.states.push(State {
        name: "lost".into(),
        transitions: Vec::new(),
        actions: Vec::new(),
    });
    assert_eq!(layout(e)[3], (2, 0));
    assert!(edges(e).iter().any(|x| x.label.contains("Life below 20")));
    assert!(edges(e).iter().any(|x| x.label == "Target in melee reach"));
    let _ = graph(&file);
}

#[test]
fn names_are_made_unique() {
    let taken = vec!["a".to_owned(), "a_2".to_owned()];
    assert_eq!(unique_name(&taken, "b"), "b");
    assert_eq!(unique_name(&taken, "a"), "a_3");
}

fn frame(editor: &mut mod_studio::ai_ui::AiEditor) {
    use eframe::egui;
    let ctx = egui::Context::default();
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let tools = ArenaTools {
                    game: Path::new(""),
                    native: Path::new(""),
                    root: Path::new(""),
                };
                editor.draw(ui, None, &tools)
            });
        });
    }
}

#[test]
fn the_editor_draws_each_entry_the_template_has() {
    let temp = Temp::template();
    let mut editor = mod_studio::ai_ui::AiEditor::default();
    editor.open(&temp.0);
    for entry in 0..2 {
        editor.entry = entry;
        frame(&mut editor);
    }
    // A graph with a phase and a mistake in it draws too.
    editor.entry = 0;
    let file = editor.file.as_mut().unwrap();
    file.entries[0].phases.push(Phase {
        name: "enrage".into(),
        when: Vec::new(),
        state: Some("fight".into()),
        graph: None,
        immune_seconds: Some(seconds(2.5)),
    });
    file.entries[0].states[1].transitions[0].to = "nowhere".into();
    editor.phase = Some(0);
    editor.state = 1;
    frame(&mut editor);
}

#[test]
fn saving_from_the_editor_refuses_while_the_engine_objects() {
    let temp = Temp::template();
    let mut editor = mod_studio::ai_ui::AiEditor::default();
    editor.open(&temp.0);
    let file = editor.file.as_mut().unwrap();
    file.entries[0].states[1].transitions[0].to = "nowhere".into();
    file.dirty = true;
    let e = editor.save().unwrap_err().to_string();
    assert!(e.contains("nowhere"), "{e}");
    assert_eq!(fs::read_to_string(temp.0.join(AI_FILE)).unwrap(), TEMPLATE);
    // Fixed, it saves.
    let file = editor.file.as_mut().unwrap();
    file.entries[0].states[1].transitions[0].to = "stalk".into();
    editor.save().unwrap();
    assert!(!editor.dirty());
}

#[test]
fn an_unreadable_file_is_shown_not_replaced() {
    let temp = Temp::new();
    temp.write(AI_FILE, "version = 1\nmystery = 3\n");
    let mut editor = mod_studio::ai_ui::AiEditor::default();
    editor.open(&temp.0);
    assert!(editor.file.is_none());
    assert!(editor.error.contains("mystery"), "{}", editor.error);
    frame(&mut editor);
    assert!(editor.save().is_ok());
    assert!(fs::read_to_string(temp.0.join(AI_FILE))
        .unwrap()
        .contains("mystery"));
}

#[test]
fn phase_immunity_can_be_authored_and_preserves_source_declarations() {
    let temp = Temp::new();
    temp.write(
        CONTROL_FILE,
        "version=1\n[[source]]\nmonster=1\n[[source.effect]]\nkind='interrupt'\n",
    );
    let mut file = AiFile::load(&temp.0).unwrap();
    file.set_immune_state(Some(185));
    file.save().unwrap();
    let saved = fs::read_to_string(temp.0.join(CONTROL_FILE)).unwrap();
    assert!(saved.contains("immune_state = 185"));
    assert!(saved.contains("monster = 1"));
    assert_eq!(AiFile::load(&temp.0).unwrap().immune_state(), Some(185));
    file.set_immune_state(None);
    file.save().unwrap();
    assert_eq!(AiFile::load(&temp.0).unwrap().immune_state(), None);
    assert!(fs::read_to_string(temp.0.join(CONTROL_FILE))
        .unwrap()
        .contains("monster = 1"));
    file.set_immune_state(Some(186));
    temp.write(CONTROL_FILE, "version=1\nimmune_state=1\n");
    assert!(file
        .save()
        .unwrap_err()
        .to_string()
        .contains("changed outside Studio"));
    assert_eq!(
        fs::read_to_string(temp.0.join(CONTROL_FILE)).unwrap(),
        "version=1\nimmune_state=1\n"
    );
}
