// D2RUST's own: single player's shared stash in the running game.
//! The single-player store of the shared stash ([`LocalStash`], next to
//! the characters in the save folder) bound to the in-process game. Each
//! whole move runs on the server thread between two frames
//! ([`SharedStash::with_game`]): the item leaves (or joins) the
//! character, the store keeps it and the character file is rewritten, in
//! that one call, so no periodic save of the game falls inside a move.
//!
//! The panel ([`crate::ui::shared_stash`]) queues its asks; the app runs
//! them here ([`run_asks`]) and gives the panel the new view.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bevy::prelude::{
    resource_exists, App, IntoScheduleConfigs, Local, NonSendMut, Res, Resource, Update,
};
use d2_server::stash_items::{self, PlaceTo};
use d2_shared_stash::local::Arrive;
use d2_shared_stash::{
    Catalog, CategoryPage, Cell, EconomyKey, GameError, ItemFacts, Lift, Lifted, LocalGame,
    LocalStash, Put, PutDone, Refusal, StashShape, StoreError, Take, TakeTo, STACK_CAP,
};
use d2_sim::units::UnitId;

use super::save::FileStore;
use super::single_player::{self, Sim};
use crate::bridge::mirror::BridgeResource;
use crate::ui::shared_stash::{StashAsk, StashView};
use crate::world_view::WorldViewUi;

/// The mod name of D2RUST's own game's stash files.
pub const D2RUST: &str = "d2rust";

/// A game's single-player shared stash: whose stash files it opens (the
/// name the files carry), the item pages' shape, the count one stackable
/// slot holds and the game's own category pages. D2Gamma's by default; a
/// mod game's client sets its own (`ClientMods::with_shared_stash`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashSettings {
    /// The game's name in its stash files (1 to 32 letters, digits, `-`
    /// or `_`).
    pub mod_id: String,
    pub shape: StashShape,
    /// The highest count of one stackable slot.
    pub cap: u32,
    /// Category pages after Runes, Gems and Misc.
    pub categories: Vec<CategoryPage>,
}

impl Default for StashSettings {
    fn default() -> StashSettings {
        StashSettings {
            mod_id: D2RUST.to_owned(),
            shape: StashShape::DEFAULT,
            cap: STACK_CAP,
            categories: Vec::new(),
        }
    }
}

impl StashSettings {
    /// `base` (the game's own slots) with this game's cap and categories.
    pub fn catalog(&self, base: Catalog) -> Catalog {
        base.with_cap(self.cap)
            .with_categories(self.categories.clone())
    }
}

/// The stash files of a save folder, one per split: softcore and
/// hardcore, expansion and classic.
fn splits(mod_id: &str) -> impl Iterator<Item = EconomyKey> + '_ {
    [(false, true), (true, true), (false, false), (true, false)]
        .into_iter()
        .map(move |(hardcore, expansion)| EconomyKey {
            hardcore,
            expansion,
            ..EconomyKey::new(mod_id)
        })
}

/// Finishes what a stopped game left in the stash files of `folder`
/// (takes in flight put back, character files rewritten from their
/// pending records). Run before a character file of the folder is read.
/// A file of `mod_id` (or naming no game) is opened as before; a file
/// naming another game (a mod game's folder) is recovered as that game,
/// its cells left for the game's own open, which knows its item pages.
/// Returns one line per fault; a split without a file is skipped.
pub fn recover_folder(folder: &Path, mod_id: &str) -> Vec<String> {
    let mut faults = Vec::new();
    for key in splits(mod_id) {
        let path = folder.join(d2_shared_stash::local::file_name(&key));
        if !path.exists() {
            continue;
        }
        let catalog = Catalog::base(|c| Some(c.to_owned()));
        let done = match LocalStash::file_mod(&path) {
            Ok(Some(owner)) if owner != mod_id => LocalStash::recover_only(
                folder,
                EconomyKey {
                    mod_id: owner,
                    ..key
                },
                catalog,
            )
            .map(drop),
            Ok(_) => LocalStash::open(folder, key, catalog).map(drop),
            Err(e) => Err(e),
        };
        if let Err(e) = done {
            faults.push(format!("shared stash in {}: {e}", folder.display()));
        }
    }
    faults
}

/// The game's half of a move, on the server thread: the local player of
/// the in-process game, and how its file is written.
struct StashGame<'a> {
    sim: &'a mut Sim,
    player: UnitId,
    recipe: &'a FileStore,
    character: String,
    /// The client's look and names of the items the ask names, by GUID.
    looks: &'a BTreeMap<u32, ItemLook>,
    /// Where each lifted item was (cursor, else the inventory cell).
    from: BTreeMap<u32, (bool, (i32, i32))>,
    /// What the last place put on the character.
    placed: Vec<u32>,
}

/// How the game client shows an item: its message stream and its tip's
/// lines (name first).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemLook {
    pub stream: Vec<u8>,
    pub lines: Vec<String>,
}

impl<'a> StashGame<'a> {
    fn new(
        sim: &'a mut Sim,
        recipe: &'a FileStore,
        looks: &'a BTreeMap<u32, ItemLook>,
    ) -> Result<Self, StoreError> {
        let (player, _) = single_player::local_player(sim)
            .ok_or_else(|| StoreError::Game("no player in the game".into()))?;
        let character = recipe
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(StashGame {
            sim,
            player,
            recipe,
            character,
            looks,
            from: BTreeMap::new(),
            placed: Vec::new(),
        })
    }
}

fn refused(e: String) -> GameError {
    GameError::Refused(e)
}

impl LocalGame for StashGame<'_> {
    fn character(&self) -> String {
        self.character.clone()
    }

    fn save_path(&self) -> PathBuf {
        self.recipe.path.clone()
    }

    fn kind(&mut self, unit: u32) -> Option<(String, Option<u32>)> {
        let h = stash_items::held_item(self.sim, self.player, unit)?;
        Some((h.code, h.plain_count))
    }

    fn lift(&mut self, what: Lift) -> Result<Lifted, GameError> {
        match what {
            Lift::Unit(guid) => {
                let l = stash_items::lift_item(self.sim, self.player, guid).map_err(refused)?;
                let h = l.held;
                self.from.insert(guid, (h.cursor, h.cell));
                let look = self.looks.get(&guid);
                let lines = look.map(|l| l.lines.clone()).unwrap_or_default();
                let facts = ItemFacts {
                    name: lines.first().cloned().unwrap_or_else(|| h.code.clone()),
                    name_parts: lines,
                    code: h.code,
                    item_type: h.item_type,
                    quality: h.quality,
                    level_req: h.level_req,
                    ilvl: h.ilvl,
                    sockets: h.sockets,
                    ethereal: h.ethereal,
                    class: h.class,
                    stats: Vec::new(),
                    ..ItemFacts::default()
                };
                Ok(Lifted::Item {
                    unit: guid,
                    record: l.bytes,
                    facts,
                    size: h.size,
                    count: h.plain_count,
                    look: look.map(|l| l.stream.clone()),
                })
            }
            Lift::Gold(g) => {
                let have = stash_items::gold(self.sim, self.player);
                if g == 0 || g > have {
                    return Err(GameError::Refused("not that much gold".into()));
                }
                stash_items::set_gold(self.sim, self.player, have - g);
                Ok(Lifted::Gold(g))
            }
        }
    }

    fn restore(&mut self, lifted: &Lifted) -> Result<(), GameError> {
        match lifted {
            Lifted::Item { unit, record, .. } => {
                let to = match self.from.get(unit) {
                    Some((true, _)) => PlaceTo::Cursor,
                    Some((false, cell)) => PlaceTo::Inventory(Some(*cell)),
                    None => PlaceTo::Inventory(None),
                };
                stash_items::place_item(self.sim, self.player, record, to)
                    .map(|_| ())
                    .map_err(refused)
            }
            Lifted::Gold(g) => {
                let have = stash_items::gold(self.sim, self.player);
                stash_items::set_gold(self.sim, self.player, have + g);
                Ok(())
            }
        }
    }

    fn room(&mut self, arrive: &Arrive, to: TakeTo) -> u64 {
        match arrive {
            Arrive::Item { record } => u64::from(stash_items::entry_fits(
                self.sim,
                self.player,
                record,
                place_to(to),
            )),
            Arrive::Stack { code, .. } => stash_items::plain_room(self.sim, self.player, code),
            Arrive::Gold(_) => {
                let (have, max) = (
                    stash_items::gold(self.sim, self.player),
                    stash_items::gold_max(self.sim, self.player),
                );
                max.saturating_sub(have)
            }
        }
    }

    fn place(&mut self, arrive: &Arrive, to: TakeTo) -> Result<(), GameError> {
        self.placed.clear();
        match arrive {
            Arrive::Item { record } => {
                let g = stash_items::place_item(self.sim, self.player, record, place_to(to))
                    .map_err(|_| GameError::NoRoom)?;
                self.placed.push(g);
            }
            Arrive::Stack { code, count } => {
                self.placed = stash_items::give_plain(self.sim, self.player, code, *count)
                    .map_err(|_| GameError::NoRoom)?;
            }
            Arrive::Gold(g) => {
                let have = stash_items::gold(self.sim, self.player);
                if have + g > stash_items::gold_max(self.sim, self.player) {
                    return Err(GameError::NoRoom);
                }
                stash_items::set_gold(self.sim, self.player, have + g);
            }
        }
        Ok(())
    }

    fn unplace(&mut self, arrive: &Arrive, _to: TakeTo) {
        match arrive {
            Arrive::Gold(g) => {
                let have = stash_items::gold(self.sim, self.player);
                stash_items::set_gold(self.sim, self.player, have.saturating_sub(*g));
            }
            _ => {
                for g in std::mem::take(&mut self.placed) {
                    stash_items::remove_item(self.sim, self.player, g);
                }
            }
        }
    }

    fn record(&mut self) -> Result<Vec<u8>, GameError> {
        self.recipe.record(self.sim).map_err(GameError::Refused)
    }
}

fn place_to(to: TakeTo) -> PlaceTo {
    match to {
        TakeTo::Cursor => PlaceTo::Cursor,
        TakeTo::Inventory => PlaceTo::Inventory(None),
    }
}

/// Runs a closure on the game between two frames (the server thread).
type Runner = dyn Fn(Box<dyn FnOnce(&mut Sim) + Send>) -> Result<(), String> + Send + Sync;

/// Single player's shared stash of the running game.
#[derive(Clone, Resource)]
pub struct SharedStash {
    stash: Arc<Mutex<LocalStash>>,
    recipe: Arc<FileStore>,
    run: Arc<Runner>,
}

impl std::fmt::Debug for SharedStash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedStash").finish_non_exhaustive()
    }
}

impl SharedStash {
    /// Opens the stash of the character saved by `recipe` (its folder),
    /// in `key`'s split, run on the game through `server`.
    pub fn open<C: d2_server::seams::Clock + Send + 'static>(
        key: EconomyKey,
        catalog: Catalog,
        recipe: FileStore,
        server: Arc<Mutex<super::server_thread::ThreadLink<single_player::Link<C>>>>,
    ) -> Result<SharedStash, StoreError> {
        SharedStash::open_with(key, catalog, StashShape::DEFAULT, recipe, server)
    }

    /// [`SharedStash::open`] with the game's own item page `shape`.
    pub fn open_with<C: d2_server::seams::Clock + Send + 'static>(
        key: EconomyKey,
        catalog: Catalog,
        shape: StashShape,
        recipe: FileStore,
        server: Arc<Mutex<super::server_thread::ThreadLink<single_player::Link<C>>>>,
    ) -> Result<SharedStash, StoreError> {
        let folder = recipe
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let stash = LocalStash::open_with(&folder, key, catalog, shape)?;
        let run = move |f: Box<dyn FnOnce(&mut Sim) + Send>| {
            let mut link = server.lock().unwrap_or_else(|e| e.into_inner());
            link.with(move |l| f(&mut l.host_mut().game))
                .map_err(|e| e.to_string())
        };
        Ok(SharedStash {
            stash: Arc::new(Mutex::new(stash)),
            recipe: Arc::new(recipe),
            run: Arc::new(run),
        })
    }

    /// The catalog of the stackable pages.
    pub fn catalog(&self) -> Catalog {
        self.lock().catalog().clone()
    }

    /// Runs `f` on the store (the tests).
    pub fn with_store<R>(&self, f: impl FnOnce(&mut LocalStash) -> R) -> R {
        f(&mut self.lock())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LocalStash> {
        self.stash.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `f` with the store and the game's half of a move, on the
    /// server thread between frames.
    fn with_game<R: Send + 'static>(
        &self,
        looks: BTreeMap<u32, ItemLook>,
        f: impl FnOnce(&mut LocalStash, &mut StashGame<'_>) -> Result<R, StoreError> + Send + 'static,
    ) -> Result<R, StoreError> {
        let out: Arc<Mutex<Option<Result<R, StoreError>>>> = Arc::new(Mutex::new(None));
        let (stash, recipe, put) = (self.stash.clone(), self.recipe.clone(), out.clone());
        (self.run)(Box::new(move |sim: &mut Sim| {
            let r = StashGame::new(sim, &recipe, &looks).and_then(|mut g| {
                let mut st = stash.lock().unwrap_or_else(|e| e.into_inner());
                f(&mut st, &mut g)
            });
            *put.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
        }))
        .map_err(StoreError::Game)?;
        let r = out.lock().unwrap_or_else(|e| e.into_inner()).take();
        r.unwrap_or_else(|| Err(StoreError::Game("the game did not run the move".into())))
    }

    /// The view of item page `page` (or none), the slots and the gold, and
    /// the search's hits.
    pub fn view(&self, page: Option<u8>, search: &str) -> Result<StashView, StoreError> {
        let mut st = self.lock();
        Ok(StashView {
            items: match page {
                Some(p) => st.page(p)?,
                None => Vec::new(),
            },
            stacks: st.stacks()?,
            gold: st.gold()?,
            hits: if search.trim().is_empty() {
                Vec::new()
            } else {
                st.search(search)?
            },
        })
    }

    /// Runs one ask of the panel. `looks`: the client's look of the items
    /// the ask names. Returns the line to show, if any.
    pub fn run(
        &self,
        ask: &StashAsk,
        looks: BTreeMap<u32, ItemLook>,
    ) -> Result<Option<String>, StoreError> {
        match ask.clone() {
            StashAsk::Refresh | StashAsk::Search { .. } => Ok(None),
            StashAsk::PutItem { guid, page, at } => {
                // A tab drop: the page's first free cells (from its top
                // left on).
                let (x, y) = at.unwrap_or((0, 0));
                let at = Some(Cell::new(page, x, y));
                self.with_game(looks, move |st, g| {
                    st.put(g, Put::Item { unit: guid, at }).map(|_| None)
                })
            }
            StashAsk::PutStack { guid } => self.with_game(looks, move |st, g| {
                st.put(g, Put::Stacks { units: vec![guid] })
                    .map(|d| left_line(&d, true))
            }),
            StashAsk::SendStackables { guids } => self.with_game(looks, move |st, g| {
                st.put(g, Put::Stacks { units: guids })
                    .map(|d| left_line(&d, false))
            }),
            StashAsk::TakeItem { id, to_cursor } => {
                let to = if to_cursor {
                    TakeTo::Cursor
                } else {
                    TakeTo::Inventory
                };
                self.with_game(looks, move |st, g| {
                    st.take(g, Take::Item { id, to }).map(|_| None)
                })
            }
            StashAsk::TakeStack { code, count } => self.with_game(looks, move |st, g| {
                let n = st.take(g, Take::Stack { code, count })?;
                Ok((u64::from(count) > n && count != u32::MAX)
                    .then(|| format!("Only {n} fit in the inventory")))
            }),
            StashAsk::Arrange { page } => self.lock().arrange(page).map(|_| None),
            StashAsk::Gold { put: true } => self.with_game(looks, move |st, g| {
                let have = stash_items::gold(g.sim, g.player);
                if have == 0 {
                    return Ok(None);
                }
                st.put(g, Put::Gold(have)).map(|_| None)
            }),
            StashAsk::Gold { put: false } => {
                let have = self.lock().gold()?;
                if have == 0 {
                    return Ok(None);
                }
                self.with_game(looks, move |st, g| {
                    let room = g.room(&Arrive::Gold(have), TakeTo::Inventory);
                    let n = have.min(room);
                    if n == 0 {
                        return Err(StoreError::NoRoom);
                    }
                    st.take(g, Take::Gold(n)).map(|_| None)
                })
            }
        }
    }
}

/// The line for an item of a kind with no slot dropped on a slot page.
pub(crate) const NO_SLOT: &str = "Only the kinds with a slot go on this page";

/// The short line for stackables left on the character. The kinds with
/// no slot ([`Refusal::BadRequest`]) are not stackables: they stay without
/// a word, except a single item dropped on a stackable page (`single`).
pub(crate) fn left_line(d: &PutDone, single: bool) -> Option<String> {
    let not_kind = |why: &Refusal| matches!(why, Refusal::BadRequest);
    if single && d.left.iter().any(|(_, why)| not_kind(why)) {
        return Some(NO_SLOT.into());
    }
    let stayed: Vec<&Refusal> = d
        .left
        .iter()
        .map(|(_, w)| w)
        .filter(|w| !not_kind(w))
        .collect();
    if stayed.is_empty() {
        return None;
    }
    let full = stayed.iter().any(|w| matches!(w, Refusal::StashFull));
    Some(if full {
        format!("{} stayed: the stash slot is full", stayed.len())
    } else {
        format!("{} stayed in the inventory", stayed.len())
    })
}

/// The line a refused move shows.
pub fn refusal_line(e: &StoreError) -> String {
    match e {
        StoreError::NoRoom => "Not enough room".into(),
        StoreError::Refused(Refusal::StashFull) => "The shared stash is full".into(),
        StoreError::Refused(r) => format!("Not moved ({r:?})"),
        other => format!("Shared stash: {other}"),
    }
}

/// Installs single player's shared stash: the store as a resource and the
/// frame system that runs the panel's asks on it.
pub fn add_shared_stash(app: &mut impl super::session::Setup, stash: SharedStash) {
    app.register("shared-stash", register_shared_stash);
    app.world_mut().insert_resource(stash);
}

/// The shared stash's frame system, once per app.
pub fn register_shared_stash(app: &mut App) {
    app.add_systems(
        Update,
        run_asks
            .run_if(resource_exists::<SharedStash>)
            .run_if(resource_exists::<BridgeResource>),
    );
}

/// The GUIDs of the character's items an ask moves.
fn ask_items(ask: &StashAsk) -> Vec<u32> {
    match ask {
        StashAsk::PutItem { guid, .. } | StashAsk::PutStack { guid } => vec![*guid],
        StashAsk::SendStackables { guids } => guids.clone(),
        _ => Vec::new(),
    }
}

/// Runs the panel's asks on the store and gives the panel the new view.
/// The game's first frame turns its panel on (every game of the session
/// has a new UI); opening the stash reads the store.
fn run_asks(
    stash: Res<SharedStash>,
    bridge: Res<BridgeResource>,
    ui: Option<NonSendMut<WorldViewUi>>,
    mut was_open: Local<bool>,
) {
    let Some(mut ui) = ui else {
        return;
    };
    let Some(o) = ui.original.as_mut() else {
        return;
    };
    {
        let mut st = o.shared_stash();
        if !st.on {
            st.on = true;
            st.catalog = Some(stash.catalog());
        }
    }
    let open = o.stash_open();
    let mut asks = o.shared_stash().take_asks();
    if open && !*was_open {
        asks.insert(0, StashAsk::Refresh);
    }
    if !open && *was_open {
        let mut st = o.shared_stash();
        st.search_focus = false;
        st.count_box = None;
    }
    *was_open = open;
    if asks.is_empty() {
        return;
    }
    let world = bridge.0.world();
    for ask in &asks {
        let looks = ask_items(ask)
            .into_iter()
            .filter_map(|g| {
                let (stream, lines) = o.item_look(world, g)?;
                Some((g, ItemLook { stream, lines }))
            })
            .collect();
        match stash.run(ask, looks) {
            Ok(Some(line)) => o.add_note(&line),
            Ok(None) => {}
            Err(e) => o.add_note(&refusal_line(&e)),
        }
    }
    let (page, search) = {
        let st = o.shared_stash();
        (st.view_page(), st.search.clone())
    };
    match stash.view(page, &search) {
        Ok(v) => o.show_shared_stash(v),
        Err(e) => o.add_note(&refusal_line(&e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::ClientMods;
    use d2_shared_stash::{StackPage, Stackable};

    fn wide() -> StashSettings {
        StashSettings {
            mod_id: "modgame".into(),
            shape: StashShape {
                pages: 3,
                cols: 14,
                rows: 14,
                renamable: true,
            },
            cap: 9_999_999,
            categories: vec![CategoryPage {
                name: "Orbs".into(),
                cols: 4,
                codes: vec![Stackable::new("orb1", "Orb One")],
                item_slots: 2,
            }],
        }
    }

    #[test]
    fn d2gammas_stash_settings_are_the_defaults() {
        let d = StashSettings::default();
        assert_eq!(d.mod_id, D2RUST);
        assert_eq!(d.shape, StashShape::DEFAULT);
        assert_eq!(d.cap, STACK_CAP);
        let base = Catalog::base(|c| Some(c.to_owned()));
        assert_eq!(d.catalog(base.clone()), base);
        let mods = ClientMods::new();
        assert_eq!(mods.shared_stash(), &d);
        assert!(mods.is_empty());
    }

    #[test]
    fn a_mod_games_settings_reach_its_catalog() {
        let mods = ClientMods::new().with_shared_stash(wide());
        assert!(!mods.is_empty());
        assert_eq!(mods.shared_stash().mod_id, "modgame");
        let cat = mods
            .shared_stash()
            .catalog(Catalog::base(|c| Some(c.to_owned())));
        assert_eq!(cat.cap(), 9_999_999);
        assert_eq!(
            cat.slot("orb1").map(|s| s.page),
            Some(StackPage::Category(0))
        );
    }

    #[test]
    fn recovery_opens_each_file_as_the_game_it_names() {
        let dir =
            std::env::temp_dir().join(format!("d2-client-stash-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let s = wide();
        drop(
            LocalStash::open_with(
                &dir,
                EconomyKey::new(s.mod_id.clone()),
                s.catalog(Catalog::base(|c| Some(c.to_owned()))),
                s.shape,
            )
            .unwrap(),
        );
        // D2Gamma's start recovers the folder without the mod's settings.
        assert_eq!(recover_folder(&dir, D2RUST), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
