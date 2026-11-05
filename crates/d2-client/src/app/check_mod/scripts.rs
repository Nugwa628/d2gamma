//! Script preflight reads the effective mod layers, equally for folders and
//! packs. Base files never supply mod rules or scripts.
use anyhow::Result;
use d2_native::layers::LayerStack;
use d2_sim::rules::{DataMod, DataModFiles};

pub(super) fn check(
    layers: &LayerStack,
    name: &str,
) -> Result<Option<d2_server::scripts::ScriptHost>, String> {
    let run = || -> Result<Option<d2_server::scripts::ScriptHost>> {
        let read = |path: &str| -> Result<Option<String>> {
            layers
                .layers
                .iter()
                .skip(1)
                .rev()
                .find_map(|layer| layer.read(path))
                .transpose()?
                .map(String::from_utf8)
                .transpose()
                .map_err(Into::into)
        };
        let stats = read("stats.toml")?;
        let rules = read("rules.toml")?;
        let states = read("states.toml")?;
        let skills = read("skills.toml")?;
        let triggers = read("triggers.toml")?;
        let crowd_control = read("crowd_control.toml")?;
        let ai = read("ai.toml")?;
        let quests = read("quests.toml")?;
        let host = DataMod::from_files(
            name,
            &DataModFiles {
                stats: stats.as_deref().map(|s| ("files/stats.toml", s)),
                states: states.as_deref().map(|s| ("files/states.toml", s)),
                skills: skills.as_deref().map(|s| ("files/skills.toml", s)),
                triggers: triggers.as_deref().map(|s| ("files/triggers.toml", s)),
                crowd_control: crowd_control
                    .as_deref()
                    .map(|s| ("files/crowd_control.toml", s)),
                ai: ai.as_deref().map(|s| ("files/ai.toml", s)),
                quests: quests.as_deref().map(|s| ("files/quests.toml", s)),
                ..DataModFiles::with_rules(rules.as_deref().map(|s| ("files/rules.toml", s)))
            },
        )?;
        d2_server::scripts::Sources::from_layers(layers)
            .and_then(|sources| sources.instantiate_for(&host.into_ruleset()))
            .map_err(anyhow::Error::msg)
    };
    run().map_err(|e| format!("{e:#}"))
}
