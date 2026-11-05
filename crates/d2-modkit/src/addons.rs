//! Explicit add-on selection and maker-approved composition.
use crate::{manifest::check_id, Kind, Manifest};
use std::{collections::BTreeSet, path::Path};

pub const SELECTION_FILE: &str = "addons.toml";

/// `mods/addons.toml`: `[enabled]` maps target ids to ordered add-on ids.
/// An absent file enables nothing. Approval alone never enables an add-on.
pub fn enabled(mods: &Path, target: &str) -> Result<Vec<String>, String> {
    let path = mods.join(SELECTION_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_enabled(&text, target).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
pub fn parse_enabled(text: &str, target: &str) -> Result<Vec<String>, String> {
    let mut root: toml::Table = text.parse().map_err(|e| format!("invalid TOML: {e}"))?;
    let table = match root.remove("enabled") {
        None => toml::Table::new(),
        Some(toml::Value::Table(t)) => t,
        _ => return Err("enabled must be a table of target ids and add-on lists".into()),
    };
    if !root.is_empty() {
        return Err("unknown key (expected [enabled])".into());
    }
    let mut selected = Vec::new();
    for (id, value) in table {
        if id != "d2gamma" {
            check_id(&id).map_err(|e| format!("target {id}: {e}"))?;
        }
        let list = value
            .as_array()
            .ok_or_else(|| format!("{id}: expected an ordered list of add-on ids"))?;
        if list.len() > 64 {
            return Err(format!("{id}: at most 64 add-ons"));
        }
        let mut seen = BTreeSet::new();
        for value in list {
            let addon = value.as_str().ok_or("add-on ids must be strings")?;
            check_id(addon).map_err(|e| format!("add-on {addon}: {e}"))?;
            if !seen.insert(addon) {
                return Err(format!("{id}: add-on {addon} is listed twice"));
            }
            if id == target {
                selected.push(addon.to_owned());
            }
        }
    }
    Ok(selected)
}

/// Owner policy in the converted install, outside all add-on payloads.
pub const PLAIN_APPROVAL_FILE: &str = "plain-addons.toml";
/// Missing policy is deliberately empty. Approval does not enable an add-on.
pub fn plain_approved(native: &Path) -> Result<Vec<String>, String> {
    let path = native.join(PLAIN_APPROVAL_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_plain_approved(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
pub fn parse_plain_approved(text: &str) -> Result<Vec<String>, String> {
    let mut root: toml::Table = text.parse().map_err(|e| format!("invalid TOML: {e}"))?;
    let approved = root
        .remove("approved")
        .unwrap_or(toml::Value::Array(Vec::new()));
    if !root.is_empty() {
        return Err("unknown owner policy key (expected approved)".into());
    }
    let mut table = toml::Table::new();
    table.insert("d2gamma".into(), approved);
    let mut doc = toml::Table::new();
    doc.insert("enabled".into(), toml::Value::Table(table));
    parse_enabled(
        &toml::to_string(&doc).map_err(|e| e.to_string())?,
        "d2gamma",
    )
}

/// Validate a kit stack: a single full mod (possibly its declared base chain),
/// then only add-ons declared for and approved by the selected full mod.
/// Plain-game owner approval is checked separately by the native loader.
pub fn check_stack(mods: &[&Manifest]) -> Result<(), String> {
    let picked = mods.iter().rev().find(|m| m.kind == Kind::Full).copied();
    let mut seen = BTreeSet::new();
    let mut previous_full: Option<&Manifest> = None;
    let mut addons_started = false;
    for m in mods {
        if !seen.insert(m.id.as_str()) {
            return Err(format!("mod {} is loaded twice", m.id));
        }
        if m.kind == Kind::Full {
            if addons_started {
                return Err(format!(
                    "{}: full mods must load before their add-ons",
                    m.id
                ));
            }
            if let Some(base) = previous_full {
                m.check_builds_on(base)
                    .map_err(|e| format!("only one full mod may run at a time: {e}"))?;
            } else if m.base.is_some() {
                return Err(format!("{}: its declared base must load first", m.id));
            }
            previous_full = Some(m);
        } else {
            addons_started = true;
            m.check_addon_for(picked)?;
        }
        if let Some(required) = m
            .requires
            .iter()
            .find(|id| m.kind == Kind::Addon && (!seen.contains(id.as_str()) || *id == &m.id))
        {
            return Err(format!(
                "{} requires {} earlier in the load order",
                m.id, required
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn full(id: &str) -> Manifest {
        Manifest::parse(&format!("id='{id}'\nname='Test'\nversion='1.0.0'\nauthor='Test'\ndescription=''\napi=1\nkind='full'\napproved_addons=['test-ui','test-extra']\n")).unwrap()
    }
    fn addon(id: &str) -> Manifest {
        Manifest::parse(&format!("id='{id}'\nname='Test'\nversion='1.0.0'\nauthor='Test'\ndescription=''\napi=1\nkind='addon'\nfor='test-mod'\n")).unwrap()
    }
    #[test]
    fn owner_policy_defaults_empty_and_is_strict() {
        assert!(parse_plain_approved("").unwrap().is_empty());
        assert!(parse_plain_approved("approved=[]").unwrap().is_empty());
        assert_eq!(
            parse_plain_approved("approved=['plain-hud']").unwrap(),
            ["plain-hud"]
        );
        for bad in [
            "approved=['plain-hud','plain-hud']",
            "approved=['../x']",
            "approved=[true]",
            "approval=[]",
        ] {
            assert!(parse_plain_approved(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn explicit_order_defaults_empty_and_rejects_duplicates_and_typos() {
        assert!(parse_enabled("", "test-mod").unwrap().is_empty());
        assert_eq!(
            parse_enabled("[enabled]\ntest-mod=['test-extra','test-ui']", "test-mod").unwrap(),
            ["test-extra", "test-ui"]
        );
        assert!(
            parse_enabled("[enabled]\ntest-mod=['test-ui']", "other-mod")
                .unwrap()
                .is_empty()
        );
        for text in [
            "[enabled]\ntest-mod=['test-ui','test-ui']",
            "[enabled]\ntest-mod=[3]",
            "[enabled]\ntest-mod=['../x']",
            "enable=[]",
        ] {
            assert!(parse_enabled(text, "test-mod").is_err());
        }
    }
    #[test]
    fn maker_approval_target_single_full_and_load_order_are_enforced() {
        let base = full("test-mod");
        let addon = addon("test-ui");
        assert!(check_stack(&[]).is_ok());
        assert!(check_stack(&[&base]).is_ok());
        assert!(check_stack(&[&base, &addon]).is_ok());
        assert!(check_stack(&[&addon, &base])
            .unwrap_err()
            .contains("before"));
        assert!(check_stack(&[&base, &addon, &addon])
            .unwrap_err()
            .contains("twice"));
        assert!(check_stack(&[&base, &full("other-mod")]).is_err());
        assert!(check_stack(&[&base, &self::addon("unapproved")])
            .unwrap_err()
            .contains("approved"));
        assert!(check_stack(&[&addon]).is_err());
        let mut dependent = self::addon("test-extra");
        dependent.requires = vec!["test-ui".into()];
        assert!(check_stack(&[&base, &dependent, &addon])
            .unwrap_err()
            .contains("earlier"));
        assert!(check_stack(&[&base, &addon, &dependent]).is_ok());
    }
    #[test]
    fn resolver_appends_only_enabled_addons_and_keeps_full_mod_identity() {
        let root = crate::testutil::temp("addon-resolver");
        let base = full("test-mod");
        let extra = addon("test-ui");
        for m in [&base, &extra] {
            let dir = root.join(&m.id);
            std::fs::create_dir_all(dir.join("files")).unwrap();
            std::fs::write(dir.join("mod.toml"), m.to_toml()).unwrap();
        }
        let spec = crate::ModSpec::Id(base.id.clone());
        assert_eq!(crate::resolve(&spec, &root).unwrap().mods.len(), 1);
        std::fs::write(root.join(SELECTION_FILE), "[enabled]\ntest-mod=['test-ui']").unwrap();
        let stack = crate::resolve(&spec, &root).unwrap();
        assert_eq!(
            stack
                .mods
                .iter()
                .map(|m| m.manifest.id.as_str())
                .collect::<Vec<_>>(),
            ["test-mod", "test-ui"]
        );
        assert_eq!(stack.picked().manifest.id, "test-mod");
        assert_eq!(stack.layer_entries().len(), 2);
        let mut closed = base;
        closed.approved_addons.clear();
        std::fs::write(root.join("test-mod/mod.toml"), closed.to_toml()).unwrap();
        assert!(crate::resolve(&spec, &root)
            .unwrap_err()
            .to_string()
            .contains("approved"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
