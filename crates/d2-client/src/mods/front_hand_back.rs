//! How a mod game's own character screens hand the player's choice back to
//! the front end, so the game starts with it.
//!
//! The front end starts a game from what its select and create screens
//! leave in two cells (`app::front_start::StartHandles`): the chosen
//! character and the new character. A mod game that replaces those screens
//! ([`super::ClientMods::with_front_screen`]) gets a [`FrontStart`] when its
//! screen is made ([`super::ui_hooks::FrontScreenFactory::make_with`]). Through it the
//! screen lists the characters the game's own save folder holds (in the
//! game's own save format), picks one or creates a new one, and gets back
//! the flow trigger to return; the engine's own select and create code
//! paths then take over: the game loads the chosen file with the game's
//! character codec, the stub of a new character is written in the game's
//! format and folder when the flow ends.
//!
//! A mod game's lobby screens (the realm's login, characters and lobby) get
//! the realm session the same way: [`FrontStart::realm`]
//! ([`super::front_realm_link::RealmLink`]).
//!
//! ```text
//! screen.key(ctx, vk) -> Some(start.choose(ctx, "Hero")?)       // Trigger::Ok
//! screen.key(ctx, vk) -> Some(start.create(NewCharacter { .. })?) // Trigger::Ok
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::front_realm_link::RealmLink;
use crate::app::front_start::StartHandles;
use crate::ui::front_end::character_files::CharacterFiles;
use crate::ui::front_end::flow::Trigger;
use crate::ui::front_end::screens::char_select::{self, Entry, Selection, SelectionHandle};
use crate::ui::front_end::screens::create::{self, NewCharacter, NewCharacterSink};
use crate::ui::front_end::FrontCtx;

/// Why a screen's choice was not taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartRefusal {
    /// The save folder holds no character of the game's format by that
    /// name.
    NoSuchCharacter,
    /// The difficulty is not open to the character.
    DifficultyClosed,
    /// The name is not one the create screen accepts
    /// ([`create::name_valid`]).
    NameInvalid,
    /// A character of that name exists already.
    NameTaken,
}

impl std::fmt::Display for StartRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            StartRefusal::NoSuchCharacter => "no such character",
            StartRefusal::DifficultyClosed => "that difficulty is not open",
            StartRefusal::NameInvalid => "that name is not allowed",
            StartRefusal::NameTaken => "that character name is already taken",
        })
    }
}

/// The game start's side of a front-end visit, given to a mod game's
/// screens: the game's save folder and character files, and the cells the
/// game start reads. Cheap to clone; one per visit.
#[derive(Clone)]
pub struct FrontStart {
    save_dir: PathBuf,
    files: Arc<dyn CharacterFiles>,
    selection: SelectionHandle,
    created: NewCharacterSink,
    expansion: bool,
    realm: Option<RealmLink>,
}

impl std::fmt::Debug for FrontStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrontStart")
            .field("save_dir", &self.save_dir)
            .field("extension", &self.files.extension())
            .field("expansion", &self.expansion)
            .field("realm", &self.realm.is_some())
            .finish()
    }
}

impl FrontStart {
    /// The start of a visit whose characters are in `save_dir`, with the
    /// cells and character files of `handles`; `expansion`: the front end
    /// is the expansion's (the select list's order).
    pub fn new(save_dir: &Path, handles: &StartHandles, expansion: bool) -> FrontStart {
        FrontStart {
            save_dir: save_dir.to_path_buf(),
            files: handles.character_files(),
            selection: handles.selection.clone(),
            created: handles.created.clone(),
            expansion,
            // The online screens' session, once the registry made it.
            #[cfg(feature = "online")]
            realm: handles.online.borrow().clone().map(RealmLink::new),
            #[cfg(not(feature = "online"))]
            realm: None,
        }
    }

    /// The start with the realm session `session` (a host that wires its
    /// own link, tests).
    pub fn with_realm(
        mut self,
        session: crate::ui::front_end::screens::online::OnlineHandle,
    ) -> FrontStart {
        self.realm = Some(RealmLink::new(session));
        self
    }

    /// The realm session the mod game's lobby screens talk to
    /// ([`RealmLink`]); `None` in a build without online play.
    pub fn realm(&self) -> Option<&RealmLink> {
        self.realm.as_ref()
    }

    /// The folder the game keeps its characters in.
    pub fn save_dir(&self) -> &Path {
        &self.save_dir
    }

    /// The characters of the game's format in the save folder, as the
    /// engine's own character select lists them (newest first on the
    /// expansion's front end).
    pub fn characters(&self) -> Vec<Entry> {
        char_select::scan_files(&self.save_dir, self.expansion, self.files.as_ref())
    }

    /// Whether a character of that name exists already (the create
    /// screen's check, case-insensitive).
    pub fn name_taken(&self, name: &str) -> bool {
        create::name_taken_with(&self.save_dir, name, self.files.extension())
    }

    /// Starts the game with the saved character `name`. Returns the
    /// trigger the screen returns: [`Trigger::Ok`], after which the flow
    /// opens the difficulty box if the character has more than one
    /// difficulty open, else starts the game.
    pub fn choose(&self, ctx: &mut FrontCtx, name: &str) -> Result<Trigger, StartRefusal> {
        let entry = self.find(name).ok_or(StartRefusal::NoSuchCharacter)?;
        Ok(self.choose_entry(ctx, entry))
    }

    /// [`FrontStart::choose`] on a listed [`Entry`].
    pub fn choose_entry(&self, ctx: &mut FrontCtx, entry: Entry) -> Trigger {
        let open = entry.difficulties_open();
        ctx.flow.difficulties_open = open;
        self.leave_selection(entry, open);
        Trigger::Ok
    }

    /// Starts the game with the saved character `name` at `difficulty`
    /// (0 Normal, 1 Nightmare, 2 Hell), for a screen that has its own
    /// difficulty choice: no difficulty box follows. Returns the trigger to
    /// return ([`Trigger::Difficulty`]).
    pub fn choose_at(
        &self,
        ctx: &mut FrontCtx,
        name: &str,
        difficulty: u8,
    ) -> Result<Trigger, StartRefusal> {
        let entry = self.find(name).ok_or(StartRefusal::NoSuchCharacter)?;
        let open = entry.difficulties_open();
        if difficulty >= open {
            return Err(StartRefusal::DifficultyClosed);
        }
        ctx.flow.difficulties_open = open;
        self.leave_selection(entry, open);
        Ok(Trigger::Difficulty(difficulty))
    }

    /// Starts the game with a new character: the choice is left for the
    /// game start, and when the flow ends the engine writes the character's
    /// file in the game's save format and folder (the create screen's own
    /// path). Returns the trigger to return ([`Trigger::Ok`]).
    pub fn create(&self, new: NewCharacter) -> Result<Trigger, StartRefusal> {
        if !create::name_valid(&new.name) {
            return Err(StartRefusal::NameInvalid);
        }
        if self.name_taken(&new.name) {
            return Err(StartRefusal::NameTaken);
        }
        *self.created.borrow_mut() = Some(new);
        Ok(Trigger::Ok)
    }

    fn find(&self, name: &str) -> Option<Entry> {
        self.characters()
            .into_iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    fn leave_selection(&self, entry: Entry, difficulties_open: u8) {
        if let Ok(mut cell) = self.selection.lock() {
            *cell = Some(Selection {
                entry,
                difficulties_open,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::front_host::write_stub_with;
    use crate::ui::front_end::character_files::{D2sFiles, NoTables};
    use crate::ui::front_end::flow::FlowCtx;
    use crate::ui::front_end::screens::create::Class;
    use d2_formats::d2s::{self, D2s};

    /// A game's own format: a `.tst` file is `TST!` then the `.d2s` bytes.
    #[derive(Debug)]
    struct TstFiles;

    const TAG: &[u8] = b"TST!";

    impl CharacterFiles for TstFiles {
        fn extension(&self) -> &str {
            "tst"
        }
        fn header(&self, bytes: &[u8]) -> Option<Vec<u8>> {
            bytes.strip_prefix(TAG).map(<[u8]>::to_vec)
        }
        fn stub(&self, stub: &D2s) -> Result<Vec<u8>, String> {
            Ok([
                TAG,
                &d2s::write(stub, &NoTables).map_err(|e| e.to_string())?,
            ]
            .concat())
        }
        fn to_expansion(&self, _: &[u8]) -> Option<Vec<u8>> {
            None
        }
    }

    fn temp(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("d2gamma-hand-back-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn character(name: &str, class: Class) -> NewCharacter {
        NewCharacter {
            name: name.into(),
            class,
            hardcore: false,
            expansion: true,
        }
    }

    /// A start on `dir` with the game's `.tst` files, and its handles.
    fn start(dir: &Path) -> (FrontStart, StartHandles) {
        let handles = StartHandles {
            files: Some(Arc::new(TstFiles)),
            ..StartHandles::default()
        };
        (FrontStart::new(dir, &handles, true), handles)
    }

    fn ctx(flow: &mut FlowCtx) -> FrontCtx<'_> {
        FrontCtx {
            expansion: true,
            flow,
            now_ms: 0,
        }
    }

    #[test]
    fn lists_and_chooses_the_games_own_characters() {
        let dir = temp("choose");
        write_stub_with(&dir, &character("Modder", Class::Paladin), 5, &TstFiles).unwrap();
        // A D2Gamma character in the folder is not the game's.
        write_stub_with(&dir, &character("Gamma", Class::Amazon), 5, &D2sFiles).unwrap();
        let (start, handles) = start(&dir);
        let names: Vec<_> = start.characters().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Modder"]);
        let mut flow = FlowCtx::default();
        assert_eq!(
            start.choose(&mut ctx(&mut flow), "Gamma"),
            Err(StartRefusal::NoSuchCharacter)
        );
        assert!(handles.selection.lock().unwrap().is_none());
        // The name is matched without case; a one-difficulty character
        // goes straight to the game (the flow reads `difficulties_open`).
        let t = start.choose(&mut ctx(&mut flow), "modder").unwrap();
        assert_eq!(t, Trigger::Ok);
        assert_eq!(flow.difficulties_open, 1);
        let chosen = handles.selection.lock().unwrap().take().unwrap();
        assert_eq!(chosen.entry.name, "Modder");
        assert_eq!(chosen.entry.class, Class::Paladin.id());
        assert_eq!(chosen.difficulties_open, 1);
    }

    #[test]
    fn a_difficulty_is_chosen_only_when_open() {
        let dir = temp("difficulty");
        write_stub_with(&dir, &character("Modder", Class::Paladin), 5, &TstFiles).unwrap();
        let (start, handles) = start(&dir);
        let mut flow = FlowCtx::default();
        assert_eq!(
            start.choose_at(&mut ctx(&mut flow), "Modder", 1),
            Err(StartRefusal::DifficultyClosed)
        );
        assert!(handles.selection.lock().unwrap().is_none());
        assert_eq!(
            start.choose_at(&mut ctx(&mut flow), "Modder", 0),
            Ok(Trigger::Difficulty(0))
        );
        assert!(handles.selection.lock().unwrap().is_some());
    }

    #[test]
    fn a_new_character_is_checked_like_the_create_screen_checks() {
        let dir = temp("create");
        write_stub_with(&dir, &character("Taken", Class::Druid), 5, &TstFiles).unwrap();
        let (start, handles) = start(&dir);
        assert_eq!(
            start.create(character("x", Class::Druid)),
            Err(StartRefusal::NameInvalid)
        );
        assert_eq!(
            start.create(character("taken", Class::Druid)),
            Err(StartRefusal::NameTaken)
        );
        assert!(handles.created.borrow().is_none());
        // A `.d2s` of the name does not take it: the game's files count.
        write_stub_with(&dir, &character("Fresh", Class::Amazon), 5, &D2sFiles).unwrap();
        assert!(!start.name_taken("Fresh"));
        let new = character("Fresh", Class::Assassin);
        assert_eq!(start.create(new.clone()), Ok(Trigger::Ok));
        assert_eq!(handles.created.borrow().clone(), Some(new));
    }
}
