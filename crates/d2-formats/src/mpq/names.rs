//! The file names a whole-install check enumerates.
//!
//! An archive's own `(listfile)` names most of its files, but
//! `patch_d2.mpq` has none, so a union of listfiles misses the patch's
//! files; [`EXTRA_NAMES`] adds the ones known to exist. Names are kept once
//! per archive lookup key ([`name_key`]), since listfiles of different
//! archives spell some names in different case. `mpq-tool formats` and the
//! `game_sweep` tests both take their names from [`known_names`], so their
//! counts share one scope.

use std::collections::BTreeMap;

use super::{Archive, MpqError};

/// Names known to exist but missing from every `(listfile)` (mostly in
/// `patch_d2.mpq`, which has none).
pub const EXTRA_NAMES: &[&str] = &[
    r"data\local\lng\eng\patchstring.tbl",
    r"data\local\lng\eng\string.tbl",
    r"data\local\lng\eng\expansionstring.tbl",
];

/// The archive's name key (`a`–`z` → `A`–`Z`, `/` → `\`, every other byte
/// unchanged, as [`super::crypto::hash`] normalizes). Two names with one
/// key hash alike, so they name the same file of an archive.
pub fn name_key(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' => '\\',
            c => c.to_ascii_uppercase(),
        })
        .collect()
}

/// The distinct file names of `lists`, keyed by [`name_key`]; the first
/// spelling met is kept, so one file spelled in different case (or with
/// `/` for `\`) by different listfiles counts once.
pub fn name_set(lists: impl IntoIterator<Item = Vec<String>>) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for name in lists.into_iter().flatten() {
        names.entry(name_key(&name)).or_insert(name);
    }
    names
}

/// [`EXTRA_NAMES`] plus every `(listfile)` of `archives`, as a [`name_set`].
pub fn known_names(archives: &[Archive]) -> Result<BTreeMap<String, String>, MpqError> {
    let (names, unreadable) = readable_names(archives);
    match unreadable.into_iter().next() {
        Some((_, error)) => Err(error),
        None => Ok(names),
    }
}

/// [`known_names`] for callers that go on without one archive's listing: an
/// archive whose `(listfile)` cannot be read adds no names, as one without a
/// listfile, and its index in `archives` is returned with the error.
pub fn readable_names(archives: &[Archive]) -> (BTreeMap<String, String>, Vec<(usize, MpqError)>) {
    let mut lists = vec![EXTRA_NAMES.iter().map(|s| s.to_string()).collect()];
    let mut unreadable = Vec::new();
    for (i, a) in archives.iter().enumerate() {
        match a.listfile() {
            Ok(Some(list)) => lists.push(list),
            Ok(None) => {}
            Err(error) => unreadable.push((i, error)),
        }
    }
    (name_set(lists), unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    // Two listfiles spelling one file in different case (or with `/`) give
    // one name: the archive lookup normalizes case and separators.
    #[test]
    fn name_set_is_case_and_separator_insensitive() {
        let names = name_set([
            list(&[r"data\global\ui\panel\invchar6.DC6", r"data\global\a.dt1"]),
            list(&[r"DATA\GLOBAL\UI\PANEL\INVCHAR6.dc6", "data/global/a.dt1"]),
            list(&[r"data\global\b.dt1"]),
        ]);
        let kept: Vec<&str> = names.values().map(String::as_str).collect();
        assert_eq!(
            kept,
            [
                r"data\global\a.dt1",
                r"data\global\b.dt1",
                r"data\global\ui\panel\invchar6.DC6",
            ]
        );
        assert_eq!(
            name_key("data/global/ui/Panel.dc6"),
            r"DATA\GLOBAL\UI\PANEL.DC6"
        );
    }

    // An archive without a `(listfile)` (like `patch_d2.mpq`) adds nothing
    // of its own; the extra names are always present.
    #[test]
    fn extra_names_survive_without_listfiles() {
        let names = known_names(&[]).unwrap();
        assert_eq!(names.len(), EXTRA_NAMES.len());
        assert!(names.contains_key(&name_key(EXTRA_NAMES[0])));
    }

    // A `(listfile)` that cannot be read (here a sector mask with no
    // decoder) fails `known_names`, while `readable_names` keeps the other
    // archives' names and reports the one it skipped.
    #[test]
    fn unreadable_listfile_is_skipped_by_readable_names() {
        use super::super::compression;
        use super::super::writer::{FileOptions, Method, MpqWriter};
        let archive = |name: &str, damage: bool| {
            let list: Vec<String> = (0..100)
                .map(|i| format!(r"data\global\x{i}.dc6"))
                .chain([name.to_owned()])
                .collect();
            let options = FileOptions {
                method: Method::Zlib,
                single_unit: true,
                ..FileOptions::default()
            };
            let mut w = MpqWriter::new();
            w.add("(listfile)", list.join("\r\n"), options);
            let mut bytes = w.to_bytes().unwrap();
            // The single-unit listfile is the first file: its mask byte
            // follows the 32-byte header.
            assert_eq!(bytes[32], compression::ZLIB);
            if damage {
                bytes[32] = compression::SPARSE;
            }
            let path = std::env::temp_dir()
                .join(format!("d2rust-names-{}-{name}.mpq", std::process::id()));
            std::fs::write(&path, bytes).unwrap();
            let a = Archive::open(&path).unwrap();
            #[cfg(unix)]
            let _ = std::fs::remove_file(&path);
            a
        };
        let archives = [archive("good.txt", false), archive("bad.txt", true)];
        assert!(known_names(&archives).is_err());
        let (names, unreadable) = readable_names(&archives);
        assert!(names.contains_key("GOOD.TXT"));
        assert!(!names.contains_key("BAD.TXT"));
        assert!(names.contains_key(&name_key(EXTRA_NAMES[0])));
        assert_eq!(unreadable.len(), 1);
        assert_eq!(unreadable[0].0, 1);
        assert_eq!(known_names(&archives[..1]).unwrap(), names);
    }
}
