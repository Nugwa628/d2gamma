//! Capability boundary for screen-only add-ons, shared by load and publication.
use crate::layers::{Layer, LayerError};
use d2_modkit::{ModError, PackSource};

fn bad(path: &str, reason: impl std::fmt::Display) -> String {
    format!("screen-only add-on: {path}: {reason}")
}
/// Check a folder or verified pack before its rules or identity are used.
pub fn check_source(source: &dyn PackSource) -> Result<(), ModError> {
    let manifest = d2_modkit::read_manifest(source)?;
    if !manifest.screen_only {
        return Ok(());
    }
    for path in source.paths() {
        if path == "mod.toml" || path == d2_modkit::publish::RECORD {
            continue;
        }
        let bytes = source.read(&path)?;
        check_file(&path, &bytes).map_err(ModError::Load)?;
    }
    Ok(())
}
pub(crate) fn check_layer(layer: &Layer) -> Result<(), LayerError> {
    if !layer
        .info
        .as_ref()
        .and_then(|i| i.kit.as_ref())
        .is_some_and(|m| m.screen_only)
    {
        return Ok(());
    }
    for path in layer.files_under("")? {
        if path == "d2gamma-pack-signatures.toml" || path.rsplit('/').next() == Some(".gitkeep") {
            continue;
        }
        let bytes = layer.read(&path).ok_or_else(|| LayerError::Bad {
            file: path.clone(),
            detail: "file vanished".into(),
        })??;
        check_file(&format!("files/{path}"), &bytes).map_err(|detail| LayerError::Bad {
            file: format!("mod {}/files/{path}", layer.name),
            detail,
        })?;
    }
    Ok(())
}
/// Positive capability list; new engine file kinds are forbidden by default.
pub fn check_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    d2_modkit::layout::check_rel_path(path).map_err(|e| bad(path, e))?;
    let text = || std::str::from_utf8(bytes).map_err(|e| bad(path, e));
    if path.starts_with("files/ui/") && path.ends_with(".toml") {
        let panel = d2_modkit::ui::Panel::parse(text()?).map_err(|e| bad(path, e))?;
        if !panel.actions.is_empty() || panel.elements.iter().any(|e| e.action.is_some()) {
            return Err(bad(
                path,
                "UI server actions and action buttons are forbidden",
            ));
        }
    } else if path.starts_with("files/effects/") && path.ends_with(".toml") {
        d2_modkit::effects::parse(text()?).map_err(|e| bad(path, e))?;
    } else if path.starts_with("files/patches/") && path.ends_with(".d2patch") {
        let (layer, findings) = d2_data::patch::parse_layer(path, bytes, 0);
        if d2_data::patch::has_errors(&findings)
            || layer
                .statements
                .iter()
                .any(|s| !matches!(s.stmt, d2_data::patch::Stmt::Str { .. }))
        {
            return Err(bad(
                path,
                "only stradd/strset string edits are allowed; table operations are forbidden",
            ));
        }
    } else if path == "files/mod/picture.png"
        || (path.starts_with("files/data/global/ui/")
            && (path.ends_with(".dc6.toml") || path.ends_with(".dc6.png")))
    {
        // UI art is still checked for complete native sidecar/pixel pairs by layers.
    } else {
        return Err(bad(path,"only UI panels/bindings/art, effects visuals and string edits are allowed; rules, tables, scripts and net actions are forbidden"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_pack_cannot_smuggle_rules_through_a_screen_only_manifest() {
        let root = std::env::temp_dir().join(format!("screen-pack-{}", std::process::id()));
        let folder = root.join("addon");
        std::fs::create_dir_all(folder.join("files")).unwrap();
        std::fs::write(folder.join("mod.toml"), "id='screen-hud'\nname='HUD'\nversion='1.0.0'\nauthor='Test'\ndescription=''\napi=1\nkind='addon'\nfor='d2gamma'\nscreen_only=true").unwrap();
        std::fs::write(folder.join("files/rules.toml"), "version=1").unwrap();
        let mut memory =
            d2_modkit::MemoryPack::from_folder(&d2_modkit::ModFolder::scan(&folder).unwrap())
                .unwrap();
        let key = d2_modkit::publish::author_key(&root.join("author.key")).unwrap();
        d2_modkit::publish::seal(&mut memory, &key).unwrap();
        for (name, bytes) in memory.files {
            let path = folder.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let path = root.join("addon.d2pack");
        crate::pack::build(&folder, &path, 1).unwrap();
        let pack = crate::pack::Pack::open(&path).unwrap();
        d2_modkit::publish::verify(&pack).unwrap();
        assert!(
            crate::package::open_mod(&path, d2_modkit::api::Date::today())
                .unwrap_err()
                .to_string()
                .contains("screen-only")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn screen_only_is_a_positive_capability_list_with_semantic_ui_and_patch_checks() {
        for path in [
            "files/rules.toml",
            "files/tables/extra.txt",
            "files/scripts/a.luau",
            "files/scripts/.hidden.luau",
            "files/scripts/a.graph.toml",
            "files/net.toml",
            "files/states.toml",
            "files/skills.toml",
            "files/acts.toml",
            "files/ui/../rules.toml",
            "files/new-engine-feature.toml",
        ] {
            assert!(check_file(path, b"version=1").is_err(), "{path}");
        }
        assert!(check_file("files/ui/hud.toml", b"version=1\nname='addon-hud'").is_ok());
        assert!(check_file(
            "files/patches/strings.d2patch",
            b"d2patch 1\nstradd [addon-caption] [Hello]\n"
        )
        .is_ok());
        for patch in [
            "d2patch 1\ntable skills\n",
            "d2patch 1\nstradd [caption] [table skills]\ntable skills\n",
            "d2patch 1\nunknown [x]\n",
        ] {
            assert!(check_file("files/patches/sneak.d2patch", patch.as_bytes()).is_err());
        }
        let ui =
            "version=1\nname='addon-hud'\n[[actions]]\nname='cheat'\nstat='strength'\ndelta=100";
        assert!(check_file("files/ui/hud.toml", ui.as_bytes()).is_err());
        assert!(check_file(
            "files/effects/spark.toml",
            b"version=1\n[[effects]]\nname='spark'\n[effects.light]\nradius=8"
        )
        .is_ok());
    }
}
