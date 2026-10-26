//! How the front end reads and writes a game's character files: their
//! extension, the header character select shows for each, the stub file
//! character create writes and the expansion conversion of character
//! select's Convert.
//!
//! D2Gamma's characters are `.d2s` files ([`D2sFiles`]): the select list
//! reads their header bytes, the stub is the 335-byte header, Convert sets
//! the expansion bit in place. A mod game whose characters are in its own
//! format hands the front end its own [`CharacterFiles`]
//! (`app::front_session::CodecFiles` reads them with the game's character
//! codec), so its select and create screens list and write its own files.

use std::fmt;
use std::sync::Arc;

use d2_formats::d2s::{self, D2s, SaveTables, StatSave, MAX_FILE};

/// A game's character files, as the front end lists, creates and converts
/// them.
pub trait CharacterFiles: fmt::Debug + Send + Sync {
    /// The files' extension, without the dot (`d2s`).
    fn extension(&self) -> &str;

    /// The `.d2s` header layout of the character in the file `bytes` (its
    /// first [`d2s::HEADER_SIZE`] bytes at least), as the select list
    /// reads name, class, level, status and look from it; `None`: not a
    /// character of this game.
    fn header(&self, bytes: &[u8]) -> Option<Vec<u8>>;

    /// The file bytes of a new character's stub (`stub`: a header with no
    /// body, `D2s::new_stub`).
    fn stub(&self, stub: &D2s) -> Result<Vec<u8>, String>;

    /// The file bytes of the character in `bytes` turned into an expansion
    /// character; `None`: it cannot be converted (the select screen shows
    /// its "conversion failed" message).
    fn to_expansion(&self, bytes: &[u8]) -> Option<Vec<u8>>;
}

/// The tables of a stub's write: a stub has no body, so none is read.
pub struct NoTables;

impl SaveTables for NoTables {
    fn stat_save(&self, _: u16) -> Option<StatSave> {
        None
    }
    fn item_entry_len(&self, _: &[u8]) -> Result<usize, String> {
        Err("stub has no items".into())
    }
}

/// D2Gamma's `.d2s` character files.
#[derive(Clone, Copy, Debug, Default)]
pub struct D2sFiles;

impl CharacterFiles for D2sFiles {
    fn extension(&self) -> &str {
        "d2s"
    }

    fn header(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        Some(bytes[..bytes.len().min(MAX_FILE)].to_vec())
    }

    fn stub(&self, stub: &D2s) -> Result<Vec<u8>, String> {
        d2s::write(stub, &NoTables).map_err(|e| e.to_string())
    }

    fn to_expansion(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        super::screens::char_select::d2s_to_expansion(bytes)
    }
}

/// D2Gamma's character files, shared.
pub fn d2s_files() -> Arc<dyn CharacterFiles> {
    Arc::new(D2sFiles)
}

/// The `name` character's file name with `files`' extension.
pub fn file_name(files: &dyn CharacterFiles, name: &str) -> String {
    format!("{name}.{}", files.extension())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d2s_files_are_the_d2s_header_and_stub() {
        let f = D2sFiles;
        assert_eq!(f.extension(), "d2s");
        assert_eq!(file_name(&f, "Hero"), "Hero.d2s");
        let stub = D2s::new_stub(b"Hero", 1, d2s::status::EXPANSION, 7).unwrap();
        let bytes = f.stub(&stub).unwrap();
        assert_eq!(bytes, d2s::write(&stub, &NoTables).unwrap());
        // The header is the file itself, cut at the largest .d2s.
        assert_eq!(f.header(&bytes).as_deref(), Some(&bytes[..]));
        let big = vec![0u8; MAX_FILE + 10];
        assert_eq!(f.header(&big).map(|h| h.len()), Some(MAX_FILE));
    }

    /// A mod game's own format: a `.tst` file is `TST!` then the `.d2s`
    /// bytes.
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
            Ok([TAG, &D2sFiles.stub(stub)?].concat())
        }
        fn to_expansion(&self, bytes: &[u8]) -> Option<Vec<u8>> {
            let b = D2sFiles.to_expansion(bytes.strip_prefix(TAG)?)?;
            Some([TAG, &b].concat())
        }
    }

    fn temp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("d2gamma-files-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn character(name: &str, expansion: bool) -> super::super::screens::create::NewCharacter {
        super::super::screens::create::NewCharacter {
            name: name.into(),
            class: super::super::screens::create::Class::Necromancer,
            hardcore: false,
            expansion,
        }
    }

    /// A game's own files: create writes its stub in its format and
    /// extension, select lists only its files, the name check and Convert
    /// use them; D2Gamma's `.d2s` reading of the same folder is unchanged.
    #[test]
    fn select_and_create_use_the_games_own_character_files() {
        use super::super::screens::char_select::{scan, scan_files, Model};
        use super::super::screens::create::{name_taken_in, name_taken_with};
        use crate::app::front_host::{write_stub, write_stub_with, FilesIn};
        use crate::ui::front_end::SaveFolder;
        let dir = temp("tst");
        let files: Arc<dyn CharacterFiles> = Arc::new(TstFiles);
        let probe = FilesIn {
            dir: dir.clone(),
            files: files.clone(),
        };
        assert!(!probe.has_saves());
        let path = write_stub_with(&dir, &character("Modder", false), 5, &TstFiles).unwrap();
        assert_eq!(path, dir.join("Modder.tst"));
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(TAG), "the game's own format");
        assert!(probe.has_saves());
        assert!(name_taken_with(&dir, "modder", "tst"));
        assert!(!name_taken_in(&dir, "Modder"), "no .d2s of that name");
        // A D2Gamma character in the same folder is not the game's.
        write_stub(&dir, &character("Gamma", true), 5).unwrap();
        let listed = scan_files(&dir, true, &TstFiles);
        assert_eq!(
            listed.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["Modder"]
        );
        assert_eq!(listed[0].class, 2);
        let d2s: Vec<String> = scan(&dir, true).into_iter().map(|e| e.name).collect();
        assert_eq!(d2s, ["Gamma"]);
        // Convert: the game's files turn the classic character into an
        // expansion one, in its own format.
        let mut m = Model::with_files(dir.clone(), true, files);
        assert_eq!(m.entries.len(), 1);
        assert!(!m.entries[0].expansion());
        m.convert_selected();
        assert!(m.entries[0].expansion());
        let converted = std::fs::read(&path).unwrap();
        assert!(converted.starts_with(TAG));
        m.rescan();
        assert!(m.entries[0].expansion(), "the file was written");
        // Delete takes the game's file.
        m.delete_selected();
        assert!(!path.exists());
        assert!(m.entries.is_empty());
        assert!(dir.join("Gamma.d2s").exists());
    }
}
