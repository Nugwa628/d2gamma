//! The front end through the library's entry point (`app::front_session`,
//! F1), headless on the user's game install: the session D2Gamma's exe
//! runs (no client hooks, its screens, its rules), and a mod game's
//! session with its own main menu screen, rules and save folder. The game
//! the front end starts is made by the session's game maker from the
//! front end's choice; the test's maker records the config and stops the
//! start, so the session goes back to the main menu. A mod game whose
//! characters are in its own format lists, starts and loads its own files.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use d2_client::app::front_host::{write_stub, write_stub_with};
use d2_client::app::front_session::CodecFiles;
use d2_client::app::front_session::{FrontEndConfig, FrontGame, HeadlessFrontEnd};
use d2_client::app::single_player::{Character, CharacterCodec, D2sCodec, LiveData, Saves};
use d2_client::mods::ui_hooks::FrontScreenFactory;
use d2_client::mods::ClientMods;
use d2_client::ui::front_end::control::{vk, Action, Control};
use d2_client::ui::front_end::flow::Trigger;
use d2_client::ui::front_end::screens::create::{Class, NewCharacter};
use d2_client::ui::front_end::{FrontCtx, Screen, CHAR_SELECT, MAIN_MENU, TRADEMARK};
use d2_formats::d2s::{D2s, SaveTables};
use d2_sim::rules::{ModRules, Ruleset};

use crate::app_support;

fn temp_dir(tag: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("front-session-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A character saved in `dir`, so Single Player opens at character select
/// with it chosen.
fn saved(dir: &Path, name: &str) {
    let c = NewCharacter {
        name: name.into(),
        class: Class::Sorceress,
        hardcore: false,
        expansion: true,
    };
    write_stub(dir, &c, 1).unwrap();
}

/// What the session's game maker was handed and made.
#[derive(Debug)]
struct Made {
    name: String,
    class: u8,
    rules: Option<String>,
    folder: PathBuf,
    save_path: Option<PathBuf>,
    mods: bool,
    /// The character was read from its save (not a new one).
    loaded: bool,
}

type Record = Rc<RefCell<Vec<Made>>>;

/// The session of a game with `rules` and `mods`, characters in `dir`.
/// Its game maker records the game the front end chose, made as the
/// default maker makes it, then stops the start.
fn open(dir: &Path, rules: Ruleset, mods: ClientMods) -> (HeadlessFrontEnd, Record) {
    open_with(Saves::new(dir, Arc::new(D2sCodec)), rules, mods)
}

/// [`open`] on the game's own save folder and format.
fn open_with(saves: Saves, rules: Ruleset, mods: ClientMods) -> (HeadlessFrontEnd, Record) {
    let mut config = FrontEndConfig::new(
        app_support::game_data(),
        None,
        rules.clone(),
        saves.clone(),
        mods.clone(),
    );
    let made: Record = Rc::default();
    let record = made.clone();
    config.make_game = Box::new(move |g: FrontGame| {
        let choice = g.choice.clone().expect("the front end's choice");
        let c = g.play_config(rules.clone(), saves.clone(), mods.clone(), false)?;
        record.borrow_mut().push(Made {
            name: choice.name,
            class: choice.class,
            rules: c.rules.mod_rules().map(|r| r.name().to_owned()),
            folder: c.saves.folder.clone(),
            save_path: c.save_path.clone(),
            mods: !c.mods.is_empty(),
            loaded: matches!(c.character, Character::Save(..)),
        });
        anyhow::bail!("the test stops the game start")
    });
    (HeadlessFrontEnd::open(config).unwrap(), made)
}

/// Enter (OK) on the saved character at character select: the session
/// enters the game, whose start the test's maker stops (back to the front
/// end).
fn start_game(s: &mut HeadlessFrontEnd, made: &Record) {
    assert_eq!(s.screen(), Some(CHAR_SELECT));
    s.key(vk::ENTER);
    let mut frames = 0;
    while made.borrow().is_empty() {
        s.frames(1);
        frames += 1;
        assert!(frames < 50, "the session starts the chosen game");
    }
    s.frames(3);
}

/// The number of controls on the screen shown.
fn controls(s: &mut HeadlessFrontEnd) -> Option<usize> {
    s.host().map(|h| h.front.controls().len())
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn d2gammas_front_end_runs_through_the_library_entry_point() {
    let dir = temp_dir("d2gamma");
    saved(&dir, "Gamma");
    let (mut s, made) = open(&dir, Ruleset::Original, ClientMods::default());
    assert!(s.in_front_end());
    // D2Gamma's own client: no hooks are inserted, its screens show.
    assert!(!s.app().world().contains_resource::<ClientMods>());
    assert_eq!(s.screen(), Some(TRADEMARK));
    s.trigger(Trigger::Continue);
    assert_eq!(s.screen(), Some(MAIN_MENU));
    let version = d2_client::identity::current().version_line();
    assert!(s.texts().contains(&version), "{:?}", s.texts());
    s.trigger(Trigger::SinglePlayer);
    start_game(&mut s, &made);
    {
        let made = made.borrow();
        assert_eq!(made.len(), 1);
        let m = &made[0];
        assert_eq!((m.name.as_str(), m.class), ("Gamma", Class::Sorceress.id()));
        assert_eq!(m.rules, None, "D2Gamma's own rules");
        assert_eq!(m.folder, dir);
        assert_eq!(m.save_path, Some(dir.join("Gamma.d2s")));
        assert!(!m.mods);
    }
    // A game that did not start: back to the front end, at the main menu.
    assert!(s.in_front_end());
    assert_eq!(s.screen(), Some(MAIN_MENU));
}

/// A mod game's rules: its name only.
#[derive(Debug)]
struct TinyRules;

impl ModRules for TinyRules {
    fn name(&self) -> &str {
        "tiny-mod"
    }
}

/// A mod game's main menu: Enter is Single Player, Esc exits (no art).
struct TinyMenu;

impl Screen for TinyMenu {
    fn build(&mut self, _ctx: &mut FrontCtx) -> Vec<Control> {
        vec![
            Control::key_only(vk::ENTER, Action::Trigger(Trigger::SinglePlayer)),
            Control::key_only(vk::ESC, Action::Trigger(Trigger::Exit)),
        ]
    }
}

struct MakeTinyMenu;

impl FrontScreenFactory for MakeTinyMenu {
    fn make(&self) -> Box<dyn Screen> {
        Box::new(TinyMenu)
    }
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_mod_games_front_end_shows_its_screen_and_starts_its_game() {
    let dir = temp_dir("mod");
    saved(&dir, "Tiny");
    let mods = ClientMods::new().with_front_screen(MAIN_MENU, MakeTinyMenu);
    let (mut s, made) = open(&dir, Ruleset::Mod(Arc::new(TinyRules)), mods);
    // The mod game's hooks are the session's, for the front end and its
    // games.
    assert!(s.app().world().contains_resource::<ClientMods>());
    s.trigger(Trigger::Continue);
    assert_eq!(s.screen(), Some(MAIN_MENU));
    // The mod's own main menu: its two key-only controls, no texts.
    assert_eq!(controls(&mut s), Some(2));
    assert!(s.texts().is_empty());
    // Its Enter is Single Player.
    s.key(vk::ENTER);
    start_game(&mut s, &made);
    {
        let made = made.borrow();
        assert_eq!(made.len(), 1);
        let m = &made[0];
        assert_eq!(m.name, "Tiny");
        assert_eq!(m.rules.as_deref(), Some("tiny-mod"));
        assert_eq!(m.folder, dir);
        assert_eq!(m.save_path, Some(dir.join("Tiny.d2s")));
        assert!(m.mods);
    }
    // Back at the mod's main menu.
    assert!(s.in_front_end());
    assert_eq!(s.screen(), Some(MAIN_MENU));
    assert_eq!(controls(&mut s), Some(2));
}

/// A mod game's own save format: a `.tst` file is `TST!` then the `.d2s`
/// bytes.
#[derive(Debug)]
struct TstCodec;

const TAG: &[u8] = b"TST!";

impl CharacterCodec for TstCodec {
    fn extension(&self) -> &str {
        "tst"
    }

    fn read(&self, data: &LiveData, bytes: &[u8], difficulty: u8) -> Result<D2s, String> {
        let body = bytes.strip_prefix(TAG).ok_or("not a tst save")?;
        D2sCodec.read(data, body, difficulty)
    }

    fn write(&self, save: &D2s, tables: &dyn SaveTables) -> Result<Vec<u8>, String> {
        Ok([TAG, &D2sCodec.write(save, tables)?].concat())
    }
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_mod_games_own_save_format_is_listed_started_and_loaded() {
    let dir = temp_dir("tst");
    let codec: Arc<dyn CharacterCodec> = Arc::new(TstCodec);
    let files = CodecFiles::new(codec.clone(), app_support::live());
    // A character in the game's format, and a D2Gamma one that is not the
    // game's.
    let c = NewCharacter {
        name: "Tester".into(),
        class: Class::Paladin,
        hardcore: false,
        expansion: true,
    };
    let path = write_stub_with(&dir, &c, 1, &files).unwrap();
    assert_eq!(path, dir.join("Tester.tst"));
    assert!(std::fs::read(&path).unwrap().starts_with(TAG));
    saved(&dir, "Gamma");
    let (mut s, made) = open_with(
        Saves::new(&dir, codec),
        Ruleset::Mod(Arc::new(TinyRules)),
        ClientMods::default(),
    );
    s.trigger(Trigger::Continue);
    s.trigger(Trigger::SinglePlayer);
    // Character select lists the game's own character only.
    let texts = s.texts();
    assert!(texts.iter().any(|t| t == "Tester"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Gamma"), "{texts:?}");
    start_game(&mut s, &made);
    let made = made.borrow();
    assert_eq!(made.len(), 1);
    let m = &made[0];
    assert_eq!((m.name.as_str(), m.class), ("Tester", Class::Paladin.id()));
    assert_eq!(m.save_path, Some(path));
    assert!(m.loaded, "the save was read with the game's codec");
}
