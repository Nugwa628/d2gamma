//! Source selection and character codecs for the headless game.
use super::realm::{BlockIds, GrantError, ItemIdSource, ItemIds, RealmStart};
use super::*;
use d2_data::bin::DataRules;
use d2_formats::d2s::SaveTables;
use std::fmt::Debug;
use std::path::PathBuf;
/// The layers a converted native folder stacks over its `base/`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Layers {
    /// `base/`, then the mods `mods/order.toml` lists, in its order.
    #[default]
    OrderFile,
    /// `base/` and exactly these mods, in this order: folders under `mods/`
    /// or data packs (`docs/mods/data-pack.md`); `mods/order.toml` is not
    /// read. Empty is `base/` alone.
    List(Vec<String>),
}

impl Layers {
    /// `base/` alone.
    pub fn base() -> Layers {
        Layers::List(Vec::new())
    }

    /// The explicit mod list, or `None` for the order file.
    pub fn order(&self) -> Option<&[String]> {
        match self {
            Layers::OrderFile => None,
            Layers::List(mods) => Some(mods),
        }
    }
}

/// Where a game reads the player's files from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameSource {
    /// The game's install folder and its archives (`--game-dir`).
    Archives(PathBuf),
    /// The install folder's archives with a mod game's own archives on
    /// top: its tables and its art, searched first, the first listed first
    /// (`ArchiveSet::open_layered`).
    LayeredArchives(PathBuf, Vec<PathBuf>),
    /// A converted native folder (`--native`) with the layers the game
    /// stacks on it.
    Native(PathBuf, Layers),
}

impl GameData {
    /// Loads the game's tables and level data from `source`, under
    /// D2Gamma's own data rules.
    pub fn open(source: &GameSource) -> Result<GameData, BuildError> {
        GameData::open_with(source, DataRules::Original)
    }

    /// [`GameData::open`] under the data `rules` of the game: a mod game
    /// passes its `Ruleset::data_rules` (`DataRules::Mod`), and every
    /// table load from the files (the game's and the client's readers
    /// alike, through `TableFiles::data_rules`) checks and reads the
    /// tables under them; a native folder's patches and compile run under
    /// them too.
    pub fn open_with(source: &GameSource, rules: DataRules) -> Result<GameData, BuildError> {
        let archives = |dir: &std::path::Path, set: Result<ArchiveSet, _>| {
            set.map(|s| GameFiles::archives(Arc::new(s)))
                .map_err(|e: d2_formats::mpq::MpqError| BuildError::Archives {
                    dir: dir.display().to_string(),
                    message: e.to_string(),
                })
        };
        let files = match source {
            GameSource::Archives(dir) => archives(dir, ArchiveSet::open_dir(dir))?,
            GameSource::LayeredArchives(dir, layers) => {
                archives(dir, ArchiveSet::open_layered(dir, layers))?
            }
            GameSource::Native(dir, layers) => GameFiles::native_under(dir, layers.order(), rules)
                .map_err(|message| BuildError::Archives {
                    dir: dir.display().to_string(),
                    message,
                })?,
        };
        let files = files.with_data_rules(rules);
        Ok(GameData::Live(Arc::new(LiveData::load(Arc::new(files))?)))
    }
}

/// A game's character save format: the bytes of its save files, read
/// into and written from the character the game plays ([`D2s`]). A mod
/// game whose characters hold more than D2RUST's keeps its own data in
/// its own format through its own codec.
pub trait CharacterCodec: Send + Sync + Debug {
    /// The save files' extension, without the dot (`d2s`).
    fn extension(&self) -> &str;

    /// Reads the save `bytes` for a game on `difficulty`, with the game's
    /// tables. The error is a message for the player.
    fn read(&self, data: &LiveData, bytes: &[u8], difficulty: u8) -> Result<D2s, String>;

    /// The bytes of `save`. The error is a message for the player.
    fn write(&self, save: &D2s, tables: &dyn SaveTables) -> Result<Vec<u8>, String>;
}

/// D2RUST's own character format: the `.d2s` reader with the app game's
/// header checks ([`LiveData::read_save`]) and the `.d2s` writer.
#[derive(Debug, Clone, Copy, Default)]
pub struct D2sCodec;

impl CharacterCodec for D2sCodec {
    fn extension(&self) -> &str {
        "d2s"
    }

    fn read(&self, data: &LiveData, bytes: &[u8], difficulty: u8) -> Result<D2s, String> {
        data.read_save(bytes, difficulty).map_err(|e| e.to_string())
    }

    fn write(&self, save: &D2s, tables: &dyn SaveTables) -> Result<Vec<u8>, String> {
        d2s::write(save, tables).map_err(|e| e.to_string())
    }
}

/// A wired game built on its owning thread, with file-backed animation names.
/// File loading, rules and character selection are independent of any client.
pub struct GameBuilder {
    pub data: GameData,
    pub seed: u32,
    pub character: Character,
    pub rules: Ruleset,
    appearance: Option<Option<Arc<dyn crate::local_seams::UnitAppearance>>>,
    /// Where the game's item IDs come from (default: the items' init
    /// seeds; a realm game: granted blocks, [`super::realm`]).
    item_ids: ItemIdSource,
}
impl GameBuilder {
    /// Start from the normal game defaults and file-backed animation names.
    pub fn new(data: GameData) -> Self {
        Self {
            data,
            seed: DEFAULT_SEED,
            character: Character::New,
            rules: Ruleset::Original,
            appearance: None,
            item_ids: ItemIdSource::InitSeed,
        }
    }
    /// Override the file-backed animation names, including explicitly disabling them.
    /// The client supplies its existing appearance table through this seam.
    pub fn appearance(
        mut self,
        appearance: Option<Arc<dyn crate::local_seams::UnitAppearance>>,
    ) -> Self {
        self.appearance = Some(appearance);
        self
    }
    /// The game's seed: the map seeds of every act and the game's random
    /// seed come from it. A realm game's seed is the one the main server
    /// picked ([`Self::realm`]); without a call, [`DEFAULT_SEED`].
    pub fn seed(mut self, seed: u32) -> Self {
        self.seed = seed;
        self
    }
    pub fn character(mut self, character: Character) -> Self {
        self.character = character;
        self
    }
    pub fn rules(mut self, rules: Ruleset) -> Self {
        self.rules = rules;
        self
    }
    /// Where the game's item IDs come from.
    pub fn item_ids(mut self, source: ItemIdSource) -> Self {
        self.item_ids = source;
        self
    }
    /// A realm game: the main server's seed and realm item IDs from its
    /// first block (`StartGame`, plan C3). More blocks arrive through
    /// [`Sim::grant_item_ids`].
    pub fn realm(self, start: RealmStart) -> Result<Self, GrantError> {
        let ids = BlockIds::with_block(start.item_ids)?;
        Ok(self.seed(start.seed).item_ids(ItemIdSource::Blocks(ids)))
    }
    /// Construct the simulation before its first client joins.
    pub fn build(self) -> Result<Sim, BuildError> {
        let scripts = load_scripts(&self.rules, || {
            let GameData::Live(data) = &self.data;
            data.archives.script_sources()
        })
        .map_err(BuildError::Setup)?;
        let ui_actions = load_ui_actions(&self.rules, scripts.as_ref(), || {
            let GameData::Live(data) = &self.data;
            crate::ui_actions::UiActions::load(data.archives.source().as_ref())
        })
        .map_err(BuildError::Setup)?;
        let mut network = load_network(&self.rules, scripts.as_ref(), || {
            let GameData::Live(data) = &self.data;
            crate::mod_network::Network::load(data.archives.source().as_ref())
        })
        .map_err(BuildError::Setup)?;
        let appearance = self.appearance.unwrap_or_else(|| {
            let GameData::Live(d) = &self.data;
            super::appearance::AnimationNames::load(d.archives.as_ref())
                .ok()
                .map(|names| Arc::new(names) as Arc<dyn crate::local_seams::UnitAppearance>)
        });
        let mut sim = build_game(
            &self.data,
            self.seed,
            self.character,
            self.rules,
            appearance,
        )?;
        network.set_legacy(ui_actions);
        sim.mod_network = network;
        *sim.item_ids() = ItemIds::new(self.item_ids);
        if let Some(scripts) = scripts {
            let scripts = Arc::new(scripts);
            sim.events.action.sys.hooks.scripts = Some(scripts.clone());
            sim.script_tick = Some(|game, events| events.action.run_scripts(game));
            sim.scripts = Some(scripts);
        }
        Ok(sim)
    }
}

fn load_network(
    rules: &Ruleset,
    scripts: Option<&crate::scripts::ScriptHost>,
    read: impl FnOnce() -> Result<crate::mod_network::Network, String>,
) -> Result<crate::mod_network::Network, String> {
    if matches!(rules, Ruleset::Original) {
        return Ok(Default::default());
    }
    let network = read()?;
    network.validate_scripts(scripts)?;
    Ok(network)
}

fn load_ui_actions(
    rules: &Ruleset,
    scripts: Option<&crate::scripts::ScriptHost>,
    read: impl FnOnce() -> Result<crate::ui_actions::UiActions, String>,
) -> Result<crate::ui_actions::UiActions, String> {
    if matches!(rules, Ruleset::Original) {
        return Ok(Default::default());
    }
    let actions = read()?;
    actions.validate_scripts(scripts)?;
    Ok(actions)
}

fn load_scripts(
    rules: &Ruleset,
    read: impl FnOnce() -> Result<crate::scripts::Sources, String>,
) -> Result<Option<crate::scripts::ScriptHost>, String> {
    if matches!(rules, Ruleset::Original) {
        return Ok(None);
    }
    read()?.instantiate_for(rules)
}

#[cfg(test)]
mod script_tests {
    use super::*;

    #[test]
    fn original_rules_never_read_network_or_send_declared_values() {
        assert!(load_network(&Ruleset::Original, None, || panic!(
            "base game must not read network contracts"
        ))
        .unwrap()
        .definition
        .empty());
    }
    #[test]
    fn original_rules_do_not_read_ui_actions() {
        assert!(load_ui_actions(&Ruleset::Original, None, || panic!(
            "base game must not read UI actions"
        ))
        .is_ok());
    }
    #[test]
    fn original_rules_do_not_read_scripts_or_initialize_luau() {
        assert!(load_scripts(&Ruleset::Original, || panic!(
            "base game must not read scripts"
        ))
        .unwrap()
        .is_none());
    }
}
