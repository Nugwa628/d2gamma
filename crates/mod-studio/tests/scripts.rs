use d2_sim::rules::scripts::EventKind;
use mod_studio::{
    scripts::{insert_event, tokens, Kind, Scripts, STARTER},
    scripts_ui::{highlight, ScriptEditor},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "studio-scripts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn script_buffers_create_nested_files_and_detect_external_edits() {
    let dir = Temp::new();
    let mut scripts = Scripts::open(&dir.0).unwrap();
    assert!(!scripts.dirty());
    assert!(!scripts.save().unwrap());
    let a = scripts.add("combat/reaction.luau").unwrap();
    let b = scripts.add("timers.luau").unwrap();
    scripts.files[a].text.push_str("-- player edits\n");
    assert!(scripts.dirty());
    assert!(scripts.save().unwrap());
    assert!(!scripts.dirty());
    let path = dir.0.join("files/scripts/combat/reaction.luau");
    assert_eq!(fs::read_to_string(&path).unwrap(), scripts.files[a].text);
    fs::write(&path, "-- external editor\nreturn {}\n").unwrap();
    scripts.files[a].text.push_str("-- Studio edits\n");
    scripts.files[b].text.push_str("-- another edit\n");
    assert!(scripts
        .save()
        .unwrap_err()
        .to_string()
        .contains("changed outside Studio"));
    assert!(!fs::read_to_string(dir.0.join("files/scripts/timers.luau"))
        .unwrap()
        .contains("another edit"));
    assert!(fs::read_to_string(path)
        .unwrap()
        .contains("external editor"));
    assert!(Scripts::open(&dir.0).unwrap().files[0]
        .text
        .contains("external editor"));
    scripts.reload(a).unwrap();
    assert!(scripts.files[a].text.contains("external editor"));
    assert!(!scripts.files[a].dirty());
    assert!(scripts.files[b].dirty());
}

#[test]
fn script_names_cannot_escape_or_overwrite_existing_modules() {
    let dir = Temp::new();
    let mut scripts = Scripts::open(&dir.0).unwrap();
    for name in [
        "../escape.luau",
        "/absolute.luau",
        "C:/escape.luau",
        "a\\b.luau",
        "bad name.luau",
        "main.lua",
        "main.graph.toml",
        "a/./main.luau",
    ] {
        assert!(scripts.add(name).is_err(), "{name}");
    }
    scripts.add("main.luau").unwrap();
    assert!(scripts.add("MAIN.luau").is_err());
    fs::create_dir_all(dir.0.join("files/scripts")).unwrap();
    fs::write(dir.0.join("files/scripts/new.luau"), "return {}").unwrap();
    assert!(scripts.add("new.luau").is_err());
    scripts.files[0].text = "x".repeat(1024 * 1024 + 1);
    assert!(scripts.save().is_err());
    assert!(!dir.0.join("files/scripts/main.luau").exists());
}

#[cfg(unix)]
#[test]
fn script_directory_links_are_rejected() {
    let dir = Temp::new();
    let outside = Temp::new();
    fs::create_dir_all(dir.0.join("files")).unwrap();
    std::os::unix::fs::symlink(&outside.0, dir.0.join("files/scripts")).unwrap();
    assert!(Scripts::open(&dir.0).is_err());
}

#[test]
fn script_project_limits_match_the_host_before_any_write() {
    let dir = Temp::new();
    let mut scripts = Scripts::open(&dir.0).unwrap();
    for i in 0..64 {
        scripts.add(&format!("module{i}.luau")).unwrap();
    }
    assert!(scripts.add("overflow.luau").is_err());
    scripts.files[0].text = " ".repeat(600_000);
    scripts.files[1].text = " ".repeat(600_000);
    assert!(scripts
        .save()
        .unwrap_err()
        .to_string()
        .contains("total source limit"));
    assert!(!dir.0.join("files/scripts").exists());
    fs::create_dir_all(dir.0.join("files/scripts")).unwrap();
    fs::write(dir.0.join("files/scripts/a.luau"), &scripts.files[0].text).unwrap();
    fs::write(dir.0.join("files/scripts/b.luau"), &scripts.files[1].text).unwrap();
    assert!(Scripts::open(&dir.0).is_err());
}

#[test]
fn script_picker_inserts_each_event_and_preserves_module_forms() {
    let mut source = STARTER.to_string();
    for event in EventKind::ALL {
        insert_event(&mut source, event).unwrap();
        let saved = source.clone();
        assert!(insert_event(&mut source, event).is_err());
        assert_eq!(source, saved);
        assert!(source.contains(&format!("function M.on_{}(e, api)", event.name())));
    }
    assert!(source.ends_with("return M\n"));
    for original in [
        "return {}",
        "return { run = function() return '}' end } -- tail",
        "return { run = function() end, };",
        "return { --[=[ } ignored ]=]\n}",
    ] {
        let mut source = original.to_string();
        insert_event(&mut source, EventKind::Timer).unwrap();
        assert!(source.contains("on_timer = function(e, api)"));
    }
    let mut template =
        include_str!("../../../templates/script-reaction/files/scripts/reaction.luau").to_owned();
    insert_event(&mut template, EventKind::AttackHit).unwrap();
    assert!(template.contains("on_attack_hit = function"));
    let mut unknown = "return make_module()".to_string();
    assert!(insert_event(&mut unknown, EventKind::Timer).is_err());
    assert_eq!(unknown, "return make_module()");
}

#[test]
fn script_highlighter_preserves_unicode_and_long_delimiters() {
    let source = "local s = [==[字 } -- not a comment]==]\n--[=[ long\ncomment ]=]\nreturn { n = 42, s = 'é\\\'字', b = `hello {s}` }";
    let tokens = tokens(source);
    assert_eq!(
        tokens
            .iter()
            .map(|t| &source[t.start..t.end])
            .collect::<String>(),
        source
    );
    assert_eq!(tokens.iter().filter(|t| t.kind == Kind::Comment).count(), 1);
    assert_eq!(tokens.iter().filter(|t| t.kind == Kind::String).count(), 3);
    let job = highlight(source);
    assert_eq!(job.text, source);
    assert!(
        job.sections
            .iter()
            .map(|s| s.format.color)
            .collect::<std::collections::HashSet<_>>()
            .len()
            >= 4
    );
    for unfinished in ["--[[字", "[==[字", "'字\\", "`字"] {
        let _ = highlight(unfinished);
    }
}

#[test]
fn script_pane_draws_empty_and_loaded_projects_without_game_assets() {
    use eframe::egui;
    let ctx = egui::Context::default();
    let dir = Temp::new();
    let mut editor = ScriptEditor::default();
    editor.open(&dir.0);
    for filled in [false, true] {
        if filled {
            editor.scripts.add("main.luau").unwrap();
        }
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| editor.draw(ui));
        });
    }
    assert!(editor.dirty());
    editor.save().unwrap();
    assert!(!editor.dirty());
}
