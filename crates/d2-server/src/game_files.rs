//! The game's files for the play app, from the user's archives or from a
//! converted native folder: one value that answers both the table reads
//! (`TableFiles`) and the asset reads (`FileSource`, typed through
//! `read_native`), so `play` runs the same code on either source.
//!
//! On a native folder the excel `.bin` / `.txt` reads come from the
//! compiled [`NativeTables`] (built once at open); the typed assets come
//! from the native files, sounds included.

use std::path::Path;
use std::sync::Arc;

use d2_data::bin::{DataRules, LoadError, TableFiles};
use d2_data::strings::StringsError;
use d2_formats::mpq::ArchiveSet;
use d2_formats::tbl::StringTable;
use d2_native::source::{fold, NativeSource, NativeTables};

use crate::file_source::{CanonicalPath, FileSource};

const EXCEL_DIR: &str = "data/global/excel/";

/// The native folder as a [`FileSource`]: [`NativeSource`] plus the table
/// set compiled once, which serves the excel files (the `.bin` set has no
/// native file of its own).
struct NativeFiles {
    src: Arc<NativeSource>,
    tables: NativeTables,
}

impl FileSource for NativeFiles {
    fn custom_stat_names(&self) -> Result<std::collections::BTreeMap<String, u16>, String> {
        self.src.custom_stat_names()
    }
    fn mod_versions(&self) -> Vec<(String, String)> {
        self.src.mod_versions()
    }
    fn net_document(&self) -> Result<Option<Vec<u8>>, String> {
        self.src.net_document()
    }
    fn ui_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        self.src.ui_documents()
    }
    fn server_ui_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        self.src.server_ui_documents()
    }
    fn effect_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        self.src.effect_documents()
    }
    fn read_file(&self, archive_name: &str) -> Option<Result<Vec<u8>, String>> {
        let p = fold(archive_name);
        match p.strip_prefix(EXCEL_DIR) {
            Some(file) if !file.contains('/') => match self.tables.read_excel(file) {
                Ok(hit) => hit.map(|(_, bytes)| Ok(bytes)),
                Err(e) => Some(Err(e.to_string())),
            },
            _ => FileSource::read_file(&*self.src, archive_name),
        }
    }

    fn read_native(
        &self,
        path: &CanonicalPath,
    ) -> Option<Result<d2_native::source::NativeAsset, String>> {
        FileSource::read_native(&*self.src, path)
    }

    fn is_native(&self) -> bool {
        true
    }
}

/// The user's game files (archives or native), see the module docs.
#[derive(Clone)]
pub struct GameFiles {
    native_source: Option<Arc<NativeSource>>,
    files: Arc<dyn FileSource>,
    tables: Arc<dyn TableFiles + Send + Sync>,
    native: bool,
    /// The data rules every table load from these files runs under
    /// ([`GameFiles::with_data_rules`]).
    rules: DataRules,
}

impl std::fmt::Debug for GameFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameFiles")
            .field("native", &self.native)
            .field("rules", &self.rules)
            .finish_non_exhaustive()
    }
}

impl GameFiles {
    /// Combine an asset reader and its table reader for a headless game or tool.
    pub fn from_sources(
        files: Arc<dyn FileSource>,
        tables: Arc<dyn TableFiles + Send + Sync>,
    ) -> Self {
        let native = files.is_native();
        let rules = tables.data_rules();
        Self {
            native_source: None,
            files,
            tables,
            native,
            rules,
        }
    }

    /// The archives of `D2_GAME_DIR`.
    pub fn archives(set: Arc<ArchiveSet>) -> Self {
        GameFiles {
            native_source: None,
            files: set.clone(),
            tables: set,
            native: false,
            rules: DataRules::Original,
        }
    }

    /// These files with every table load from them run under the data
    /// `rules` of the game (`TableFiles::data_rules`): a mod game's
    /// `DataRules::Mod`. A native folder's tables are compiled under its
    /// rules when it opens ([`GameFiles::native_under`]).
    pub fn with_data_rules(mut self, rules: DataRules) -> Self {
        self.rules = rules;
        self
    }

    /// A converted native folder (`SourceError` text names the cause and
    /// says to re-run the converter).
    pub fn native(dir: &Path) -> Result<Self, String> {
        Self::native_with(dir, None)
    }

    /// [`GameFiles::native`] with the caller's layer list
    /// (`NativeSource::open_with`): `Some(mods)` stacks `base/` and those
    /// mods, `None` reads `mods/order.toml`.
    pub fn native_with(dir: &Path, order: Option<&[String]>) -> Result<Self, String> {
        Self::native_under(dir, order, DataRules::Original)
    }

    /// [`GameFiles::native_with`] with the tables patched, compiled and
    /// loaded under the data `rules` of the game
    /// (`NativeSource::tables_with`).
    pub fn native_under(
        dir: &Path,
        order: Option<&[String]>,
        rules: DataRules,
    ) -> Result<Self, String> {
        let src = Arc::new(NativeSource::open_with(dir, order).map_err(|e| e.to_string())?);
        Self::from_native_under(src, rules)
    }

    /// An opened native source: compiles the table set once.
    pub fn from_native(src: Arc<NativeSource>) -> Result<Self, String> {
        Self::from_native_under(src, DataRules::Original)
    }

    /// [`GameFiles::from_native`] under the data `rules` of the game.
    pub fn from_native_under(src: Arc<NativeSource>, rules: DataRules) -> Result<Self, String> {
        let tables = src
            .tables_with(d2_data::bin::DEFAULT_LANGUAGE, rules)
            .map_err(|e| e.to_string())?;
        let shared = Arc::new(tables.clone());
        Ok(GameFiles {
            native_source: Some(src.clone()),
            files: Arc::new(NativeFiles { src, tables }),
            tables: shared,
            native: true,
            rules,
        })
    }

    /// `true` for a native folder.
    pub fn is_native(&self) -> bool {
        self.native
    }

    /// Last explicit mod layer wins. Base games ignore this file entirely.
    pub fn map_rules(&self) -> Result<d2_sim::drlg::maps::Maps, String> {
        if self.rules != DataRules::Mod {
            return Ok(Default::default());
        }
        if let Some(src) = &self.native_source {
            for layer in src.layers.layers.iter().skip(1).rev() {
                if let Some(bytes) = layer.read("levels.toml") {
                    let bytes = bytes.map_err(|e| e.to_string())?;
                    let text = std::str::from_utf8(&bytes)
                        .map_err(|e| format!("files/levels.toml: {e}"))?;
                    return d2_sim::drlg::maps::Maps::parse(text);
                }
            }
        }
        Ok(Default::default())
    }

    /// Mod script sources are read only by the authoritative game builder.
    pub fn script_sources(&self) -> Result<crate::scripts::Sources, String> {
        match &self.native_source {
            Some(src) => crate::scripts::Sources::from_layers(&src.layers),
            None => Ok(crate::scripts::Sources::default()),
        }
    }

    /// The asset reads, shared (`Arc<dyn FileSource>` for the loaders).
    pub fn source(&self) -> Arc<dyn FileSource> {
        self.files.clone()
    }
}

impl FileSource for GameFiles {
    fn custom_stat_names(&self) -> Result<std::collections::BTreeMap<String, u16>, String> {
        self.files.custom_stat_names()
    }
    fn mod_versions(&self) -> Vec<(String, String)> {
        self.files.mod_versions()
    }
    fn net_document(&self) -> Result<Option<Vec<u8>>, String> {
        self.files.net_document()
    }
    fn ui_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        self.files.ui_documents()
    }
    fn server_ui_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        self.files.server_ui_documents()
    }
    fn effect_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        self.files.effect_documents()
    }
    fn read_file(&self, archive_name: &str) -> Option<Result<Vec<u8>, String>> {
        self.files.read_file(archive_name)
    }

    fn read_native(
        &self,
        path: &CanonicalPath,
    ) -> Option<Result<d2_native::source::NativeAsset, String>> {
        self.files.read_native(path)
    }

    fn is_native(&self) -> bool {
        self.native
    }
}

impl TableFiles for GameFiles {
    fn read_excel(&self, file: &str) -> Result<Option<(String, Vec<u8>)>, LoadError> {
        self.tables.read_excel(file)
    }

    fn lod(&self) -> bool {
        self.tables.lod()
    }

    fn string_table(&self, path: &str) -> Result<StringTable, StringsError> {
        self.tables.string_table(path)
    }

    fn string_edits(&self) -> Vec<d2_data::strings::StringEdit> {
        self.tables.string_edits()
    }

    fn data_rules(&self) -> DataRules {
        self.rules
    }
}
