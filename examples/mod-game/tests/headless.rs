//! The example mod game runs headless on the player's files: its game is
//! built under its own rules and save folder, a character joins and the
//! game ticks (`docs/plans/mods/seams.md` MS16). Its windowed game carries
//! the same rules and saves, and it brands itself as its own game (E1).
//! Its window opens at its own front end, through the library's entry
//! point (F1).

use d2_client::app::single_player::{GameData, GameSource, Layers};
use d2_sim::rules::{ModRules, SkillFn};
use mod_game_example::{
    builder, front_end_config, identity, rules_name, run_frames, saves, start_and_join,
    window_config, ExampleRules, Options, StepClock, CREDITS, EXAMPLE_DO_SLOT, FOLDER, NAME, TITLE,
};

#[test]
fn the_example_adds_one_skill_body_past_d2rusts_table() {
    let rules = ExampleRules::default();
    assert_eq!(rules.name(), NAME);
    let skills = rules.skills().expect("skill bodies");
    assert!(skills.has(SkillFn::Do, EXAMPLE_DO_SLOT));
    assert!(!skills.has(SkillFn::Do, 1));
    assert!(!skills.has(SkillFn::Start, EXAMPLE_DO_SLOT));
}

#[test]
fn options_choose_the_source_layers_and_saves() {
    let args = |a: &[&str]| Options::parse(a.iter().map(|s| s.to_string()));
    let o = args(&[
        "--native", "n", "--layer", "a", "--layer", "b", "--saves", "s",
    ])
    .unwrap();
    assert_eq!(
        o.source,
        GameSource::Native("n".into(), Layers::List(vec!["a".into(), "b".into()]))
    );
    assert_eq!(o.saves, Some(std::path::PathBuf::from("s")));
    assert_eq!(o.saves_dir(), std::path::PathBuf::from("s"));
    assert!(!o.window);
    assert_eq!(o.name, None, "the window opens at the front end");
    assert_eq!(o.game_dir(), None);
    let o = args(&["--game-dir", "g"]).unwrap();
    assert_eq!(o.source, GameSource::Archives("g".into()));
    assert_eq!(o.saves, None);
    assert!(args(&["--game-dir", "g", "--layer", "a"]).is_err());
    assert!(args(&[]).is_err());
}

#[test]
fn options_lay_archives_over_an_install_and_open_a_window() {
    let args = |a: &[&str]| Options::parse(a.iter().map(|s| s.to_string()));
    let o = args(&[
        "--game-dir",
        "g",
        "--archive",
        "top.mpq",
        "--archive",
        "next.mpq",
        "--window",
        "--frames",
        "60",
        "--name",
        "Tester",
    ])
    .unwrap();
    assert_eq!(
        o.source,
        GameSource::LayeredArchives("g".into(), vec!["top.mpq".into(), "next.mpq".into()])
    );
    assert!(o.window);
    assert_eq!(o.frames, Some(60));
    assert_eq!(o.name.as_deref(), Some("Tester"));
    assert_eq!(o.game_dir(), Some("g".into()));
    // Archives go over an install, not a native folder.
    assert!(args(&["--native", "n", "--archive", "a.mpq"]).is_err());
}

#[test]
fn the_example_brands_itself_as_its_own_game() {
    let id = identity();
    assert_eq!(id.name, TITLE);
    assert_eq!(id.window_title, TITLE);
    assert_eq!(id.folder, FOLDER);
    assert_eq!(id.version, env!("CARGO_PKG_VERSION"));
    assert!(!id.is_d2gamma_folder());
    let credits = id.credits.as_ref().expect("its own credits");
    assert_eq!(credits(false).as_deref(), Some(CREDITS.as_bytes()));
    assert_eq!(credits(true).as_deref(), Some(CREDITS.as_bytes()));
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_example_game_runs_under_its_own_rules() {
    let dir = std::env::var_os("D2_GAME_DIR").expect("D2_GAME_DIR");
    let data = GameData::open(&GameSource::Archives(dir.into())).expect("the install loads");
    let saves_dir = std::env::temp_dir().join(format!("d2rust-mod-game-{}", std::process::id()));
    let b = builder(data.clone(), saves_dir.clone());
    assert_eq!(b.saves.folder, saves_dir);
    let clock = StepClock::default();
    let (mut link, _) = start_and_join(b, &clock).expect("started");
    assert!(run_frames(&mut link, &clock, 10).unwrap() > 0);
    assert_eq!(rules_name(&mut link).unwrap().as_deref(), Some(NAME));
    // The windowed game is configured with the same rules and saves.
    let saves = saves(saves_dir.clone());
    let path = saves.file("Hero");
    let c = window_config(
        data,
        saves,
        d2_client::app::single_player::new_character("sorceress", "Hero").unwrap(),
        Some(path.clone()),
        Some(1),
    );
    assert_eq!(
        c.rules.mod_rules().map(|r| r.name().to_owned()).as_deref(),
        Some(NAME)
    );
    assert_eq!(c.saves.folder, saves_dir);
    assert_eq!(c.save_path, Some(path));
    assert_eq!(c.exit_after, Some(1));
}

/// The example's front end, headless (`HeadlessFrontEnd`, the session
/// `run_front_end` plays): branded with its identity, its characters in
/// its own save folder, each game it starts under its rules and saves.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_examples_window_opens_at_its_own_front_end() {
    use d2_client::app::front_session::{FrontGame, HeadlessFrontEnd};
    use d2_client::app::front_start::StartChoice;
    use d2_client::ui::front_end::flow::Trigger;
    use d2_client::ui::front_end::{MAIN_MENU, TRADEMARK};
    // First thing, as the example's binary does.
    d2_client::identity::install(identity()).expect("the identity is set once");
    let dir = std::env::var_os("D2_GAME_DIR").expect("D2_GAME_DIR");
    let data =
        GameData::open(&GameSource::Archives(dir.clone().into())).expect("the install loads");
    let saves_dir =
        std::env::temp_dir().join(format!("d2gamma-mod-game-front-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&saves_dir);
    std::fs::create_dir_all(&saves_dir).unwrap();
    // The game maker: a character the front end chose, under the
    // example's rules and saves.
    let mut config = front_end_config(data.clone(), Some(dir.into()), saves_dir.clone());
    assert_eq!(config.save_dir, saves_dir);
    let file = saves_dir.join("Hero.d2s");
    let c = (config.make_game)(FrontGame {
        data,
        game_dir: None,
        choice: Some(StartChoice {
            name: "Hero".into(),
            class: 1,
            status: 0x20,
            difficulty: 0,
            save: None,
            file: file.clone(),
        }),
        menu_difficulty: None,
        load: None,
    })
    .expect("the game config");
    assert_eq!(
        c.rules.mod_rules().map(|r| r.name().to_owned()).as_deref(),
        Some(NAME)
    );
    assert_eq!(c.saves.folder, saves_dir);
    assert_eq!(c.save_path, Some(file));
    // The front end: the example's name on the main menu's version line.
    let mut s = HeadlessFrontEnd::open(config).expect("the session");
    assert!(s.in_front_end());
    assert_eq!(s.screen(), Some(TRADEMARK));
    s.trigger(Trigger::Continue);
    assert_eq!(s.screen(), Some(MAIN_MENU));
    let line = format!("{TITLE} {}", env!("CARGO_PKG_VERSION"));
    assert!(s.texts().contains(&line), "{:?}", s.texts());
}
