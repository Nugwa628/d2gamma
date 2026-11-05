//! Read-only comparison of an egui-authored showcase with the reference template.
//! Run explicitly after the Studio tour, with STUDIO_SHOWCASE_ROOT set.
use anyhow::{bail, Context, Result};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};
fn roots() -> Result<(PathBuf, PathBuf)> {
    Ok((
        PathBuf::from(
            std::env::var_os("STUDIO_SHOWCASE_ROOT")
                .context("set STUDIO_SHOWCASE_ROOT to the UI-created project")?,
        ),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates/showcase"),
    ))
}
fn read(root: &Path, name: &str) -> Result<String> {
    std::fs::read_to_string(root.join(name)).with_context(|| format!("{}: {name}", root.display()))
}
fn equal<T: Debug + PartialEq>(name: &str, actual: T, expected: T) -> Result<()> {
    if actual != expected {
        bail!("{name}: saved {actual:?}; expected {expected:?}");
    }
    Ok(())
}
fn ai(root: &Path) -> Result<Vec<mod_studio::ai::Entry>> {
    let mut entries = mod_studio::ai::parse(&read(root, "files/ai.toml")?)?;
    for entry in &mut entries {
        if entry.target.is_none() {
            entry.target = Some("enemy".into());
        }
        if entry.start.is_none() {
            entry.start = entry.states.first().map(|s| s.name.clone());
        }
    }
    Ok(entries)
}
fn files(root: &Path) -> Result<Vec<PathBuf>> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for item in std::fs::read_dir(at)? {
            let item = item?;
            if item.file_type()?.is_dir() {
                walk(root, &item.path(), out)?;
            } else {
                out.push(item.path().strip_prefix(root)?.into());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}
fn runtime_paths(root: &Path, studio: bool) -> Result<Vec<String>> {
    let mut paths = files(&root.join("files"))?
        .into_iter()
        // Empty-directory placeholders are scaffold documentation, not mod data.
        .filter(|p| p.file_name().is_none_or(|name| name != ".gitkeep"))
        .map(|p| {
            let name = p.to_string_lossy().replace('\\', "/");
            if studio {
                match name.as_str() {
                    "patches/~studio.d2patch" => "patches/lantern.d2patch".into(),
                    "effects/studio.toml" => "effects/lantern.toml".into(),
                    _ => name,
                }
            } else {
                name
            }
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}
#[test]
#[ignore = "requires a project created through the Studio input tour"]
fn studio_authored_showcase_documents_match_reference() -> Result<()> {
    let (actual, expected) = roots()?;
    let mut failures = Vec::new();
    macro_rules! check {
        ($name:expr, $saved:expr, $reference:expr $(,)?) => {
            if let Err(error) = (|| -> Result<()> { equal($name, $saved, $reference) })() {
                failures.push(format!("{error:#}"));
            }
        };
    }
    check!(
        "runtime file inventory (Studio ownership names normalized)",
        runtime_paths(&actual, true)?,
        runtime_paths(&expected, false)?,
    );
    check!(
        "manifest",
        d2_modkit::Manifest::parse(&read(&actual, "mod.toml")?)?,
        d2_modkit::Manifest::parse(&read(&expected, "mod.toml")?)?,
    );
    for name in ["files/rules.toml", "files/crowd_control.toml"] {
        check!(
            name,
            read(&actual, name)?.parse::<toml::Table>()?,
            read(&expected, name)?.parse::<toml::Table>()?,
        );
    }
    check!(
        "stats",
        d2_sim::stats::named::StatRules::from_toml(&read(&actual, "files/stats.toml")?)?,
        d2_sim::stats::named::StatRules::from_toml(&read(&expected, "files/stats.toml")?)?,
    );
    check!(
        "difficulties",
        d2_sim::rules::difficulties::Difficulties::from_toml(&read(
            &actual,
            "files/difficulties.toml",
        )?)?,
        d2_sim::rules::difficulties::Difficulties::from_toml(&read(
            &expected,
            "files/difficulties.toml",
        )?)?,
    );
    check!(
        "acts",
        d2_sim::rules::acts::Acts::from_toml(&read(&actual, "files/acts.toml")?)
            .map_err(anyhow::Error::msg)?,
        d2_sim::rules::acts::Acts::from_toml(&read(&expected, "files/acts.toml")?)
            .map_err(anyhow::Error::msg)?,
    );
    check!(
        "maps",
        d2_sim::drlg::maps::Maps::parse(&read(&actual, "files/levels.toml")?)
            .map_err(anyhow::Error::msg)?,
        d2_sim::drlg::maps::Maps::parse(&read(&expected, "files/levels.toml")?)
            .map_err(anyhow::Error::msg)?,
    );
    check!("AI", ai(&actual)?, ai(&expected)?);
    check!(
        "state/skill/trigger/quest",
        mod_studio::kit::Kit::load(&actual)?.texts()?,
        mod_studio::kit::Kit::load(&expected)?.texts()?,
    );
    check!(
        "network",
        d2_modkit::net::Definition::parse(&read(&actual, "files/net.toml")?)
            .map_err(anyhow::Error::msg)?,
        d2_modkit::net::Definition::parse(&read(&expected, "files/net.toml")?)
            .map_err(anyhow::Error::msg)?,
    );
    check!(
        "graph",
        d2_modkit::graph::Graph::parse(&read(&actual, "files/scripts/lantern.graph.toml")?)?,
        d2_modkit::graph::Graph::parse(&read(&expected, "files/scripts/lantern.graph.toml")?)?,
    );
    check!(
        "script",
        read(&actual, "files/scripts/ward.luau")?.replace("\r\n", "\n"),
        read(&expected, "files/scripts/ward.luau")?.replace("\r\n", "\n"),
    );
    for name in ["files/ui/lantern.toml", "files/ui/quest_log.toml"] {
        check!(
            name,
            d2_modkit::ui::Panel::parse(&read(&actual, name)?).map_err(anyhow::Error::msg)?,
            d2_modkit::ui::Panel::parse(&read(&expected, name)?).map_err(anyhow::Error::msg)?,
        );
    }
    let mut effects = Vec::new();
    for file in files(&actual.join("files/effects"))? {
        effects.extend(
            d2_modkit::effects::parse(&read(
                &actual.join("files/effects"),
                file.to_str().context("effect path")?,
            )?)
            .map_err(anyhow::Error::msg)?,
        );
    }
    check!(
        "effects",
        effects,
        d2_modkit::effects::parse(&read(&expected, "files/effects/lantern.toml")?)
            .map_err(anyhow::Error::msg)?,
    );
    check!(
        "native asset paths",
        files(&actual.join("files/data"))?,
        files(&expected.join("files/data"))?,
    );
    for file in files(&expected.join("files/data"))? {
        let relative = Path::new("files/data").join(file);
        let name = relative.to_str().context("asset path")?;
        if relative.extension().is_some_and(|e| e == "toml") {
            equal(
                name,
                read(&actual, name)?.parse::<toml::Table>()?,
                read(&expected, name)?.parse::<toml::Table>()?,
            )?;
        } else {
            equal(
                name,
                std::fs::read(actual.join(&relative))?,
                std::fs::read(expected.join(&relative))?,
            )?;
        }
    }
    check!(
        "icon",
        std::fs::read(actual.join("files/icons/lantern.png"))?,
        std::fs::read(expected.join("files/icons/lantern.png"))?,
    );
    let actual_addon = actual
        .parent()
        .context("mods folder")?
        .join("showcase-notes");
    let expected_addon = expected
        .parent()
        .context("templates")?
        .join("showcase-notes");
    check!(
        "add-on runtime file inventory",
        runtime_paths(&actual_addon, true)?,
        runtime_paths(&expected_addon, false)?,
    );
    check!(
        "add-on manifest",
        d2_modkit::Manifest::parse(&read(&actual_addon, "mod.toml")?)?,
        d2_modkit::Manifest::parse(&read(&expected_addon, "mod.toml")?)?,
    );
    check!(
        "add-on panel",
        d2_modkit::ui::Panel::parse(&read(&actual_addon, "files/ui/field-notes.toml")?)
            .map_err(anyhow::Error::msg)?,
        d2_modkit::ui::Panel::parse(&read(&expected_addon, "files/ui/field-notes.toml")?)
            .map_err(anyhow::Error::msg)?,
    );
    if !failures.is_empty() {
        bail!(
            "{} showcase differences:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    Ok(())
}
#[test]
#[ignore = "requires the Studio tour project and a converted install"]
fn studio_authored_showcase_tables_match_reference() -> Result<()> {
    let (actual, expected) = roots()?;
    let native =
        PathBuf::from(std::env::var_os("D2_SHOWCASE_NATIVE").context("set D2_SHOWCASE_NATIVE")?);
    let installed = actual.parent().context("mods folder")?;
    let actual = mod_studio::tables::Tables::load(&native, installed, &actual)?.edited;
    let expected = mod_studio::tables::Tables::load(&native, installed, &expected)?.edited;
    equal("table count", actual.tables.len(), expected.tables.len())?;
    for (actual, expected) in actual.tables.iter().zip(&expected.tables) {
        equal("table name", actual.name(), expected.name())?;
        equal(actual.name(), &actual.header, &expected.header)?;
        equal(actual.name(), actual.rows.len(), expected.rows.len())?;
        for (row, (a, e)) in actual.rows.iter().zip(&expected.rows).enumerate() {
            equal(&format!("{} row {row}", actual.name()), &a.cells, &e.cells)?;
        }
    }
    equal(
        "strings",
        actual.strings.iter().map(|s| &s.edit).collect::<Vec<_>>(),
        expected.strings.iter().map(|s| &s.edit).collect::<Vec<_>>(),
    )?;
    Ok(())
}
