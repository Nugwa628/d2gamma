//! Explicit, opt-in copies from plain saves to an installed mod's saves.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn no_links(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "save path leaves D2Gamma")?;
    let mut current = root.to_path_buf();
    for part in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(part) = part {
            current.push(part);
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                let linked = metadata.file_type().is_symlink();
                #[cfg(windows)]
                let linked = linked || {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                if linked {
                    return Err("Character copies cannot use linked folders or files.".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// List only plain characters. Merely opening this picker creates nothing.
pub fn characters(root: &Path) -> Vec<String> {
    let saves = root.join("saves");
    if no_links(root, &saves).is_err() {
        return Vec::new();
    }
    crate::ui::front_end::screens::char_select::scan(&saves, true)
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

/// Recheck the maker's permission at the click. Convert before creating a
/// destination, never replace a character, and never write to the source.
pub fn copy(
    root: &Path,
    id: &str,
    name: &str,
    convert: impl FnOnce(&[u8]) -> Result<Vec<u8>, String>,
) -> Result<PathBuf, String> {
    d2_modkit::manifest::check_id(id)?;
    if !(2..=15).contains(&name.len())
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Invalid character name.".into());
    }
    let folder = root.join("mods").join(id);
    let manifest = folder.join("mod.toml");
    let source = root.join("saves").join(format!("{name}.d2s"));
    let destination = folder.join("saves").join(format!("{name}.d2m"));
    for path in [&manifest, &source, &destination] {
        no_links(root, path)?;
    }
    let text = std::fs::read_to_string(manifest).map_err(|e| e.to_string())?;
    let maker = d2_modkit::Manifest::parse(&text).map_err(|e| e.to_string())?;
    if maker.id != id || maker.kind != d2_modkit::Kind::Full || !maker.accepts_d2gamma_characters {
        return Err("This mod does not accept D2Gamma characters.".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&source)
        .map_err(|e| e.to_string())?
        .take(d2_formats::d2s::MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > d2_formats::d2s::MAX_FILE
        || d2_formats::d2s::header_name(&bytes) != name.as_bytes()
    {
        return Err("Invalid D2Gamma character file.".into());
    }
    let encoded = convert(&bytes)?;
    let saves = destination.parent().unwrap();
    if std::fs::read_dir(saves).is_ok_and(|mut entries| {
        entries.any(|e| {
            e.is_ok_and(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&format!("{name}.d2m"))
                    || e.file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&format!("{name}.d2s"))
            })
        })
    }) {
        return Err("That character already exists in this mod. Nothing was replaced.".into());
    }
    std::fs::create_dir_all(saves).map_err(|e| e.to_string())?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|e| format!("Character was not copied: {e}"))?;
    if let Err(error) = output.write_all(&encoded).and_then(|()| output.sync_all()) {
        drop(output);
        let _ = std::fs::remove_file(&destination);
        return Err(error.to_string());
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(tag: &str, allowed: bool) -> (PathBuf, Vec<u8>) {
        let root = std::env::temp_dir().join(format!("d2g-copy-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("mods/new-world")).unwrap();
        std::fs::create_dir_all(root.join("saves")).unwrap();
        std::fs::write(root.join("mods/new-world/mod.toml"), format!("id='new-world'\nname='New World'\nauthor='Nugwa'\ndescription=''\nversion='1.0.0'\napi=1\nkind='full'\naccepts_d2gamma_characters={allowed}\n")).unwrap();
        let save = d2_formats::d2s::D2s::new_stub(b"Traveller", 0, 0, 17).unwrap();
        let bytes = d2_formats::d2s::write(&save, &crate::ui::front_end::character_files::NoTables)
            .unwrap();
        std::fs::write(root.join("saves/Traveller.d2s"), &bytes).unwrap();
        (root, bytes)
    }
    #[test]
    fn character_copy_requires_maker_permission_preserves_source_and_refuses_collisions() {
        for allowed in [false, true] {
            let (root, bytes) = setup(if allowed { "on" } else { "off" }, allowed);
            assert_eq!(characters(&root), ["Traveller"]);
            let result = copy(&root, "new-world", "Traveller", |b| {
                assert_eq!(b, bytes);
                Ok(b"converted".to_vec())
            });
            assert_eq!(result.is_ok(), allowed, "{result:?}");
            assert_eq!(
                std::fs::read(root.join("saves/Traveller.d2s")).unwrap(),
                bytes
            );
            if allowed {
                let path = result.unwrap();
                assert_eq!(path, root.join("mods/new-world/saves/Traveller.d2m"));
                assert_eq!(std::fs::read(&path).unwrap(), b"converted");
                assert!(copy(&root, "new-world", "Traveller", |_| Ok(vec![9])).is_err());
                assert_eq!(std::fs::read(path).unwrap(), b"converted");
            } else {
                assert!(!root.join("mods/new-world/saves").exists());
            }
            assert!(copy(&root, "../escape", "Traveller", |_| unreachable!()).is_err());
            assert!(copy(&root, "new-world", "../Traveller", |_| unreachable!()).is_err());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn failed_conversion_creates_no_save_folder() {
        let (root, bytes) = setup("bad", true);
        assert!(copy(&root, "new-world", "Traveller", |_| Err("bad item".into())).is_err());
        assert!(!root.join("mods/new-world/saves").exists());
        assert_eq!(
            std::fs::read(root.join("saves/Traveller.d2s")).unwrap(),
            bytes
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
