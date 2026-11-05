//! The mod kit's building-block files: `files/states.toml`, `skills.toml`,
//! `triggers.toml` and `crowd_control.toml`.
//!
//! Two checks run over them. The first reads every mod of the stack against
//! the block vocabulary (`d2_sim::rules::data_mod::blocks`): a name the
//! vocabulary does not have is an error with the nearest known name, a block
//! newer than the mod's `api` is an error, and a deprecated block is a
//! warning. The second builds the data mod host from the files the stack
//! plays under, as starting the mod does, so a wrong value, a missing state
//! entry or a state that would never end is reported here and not at play.

use super::{Report, Severity};
use d2_modkit::resolve::LoadedMod;
use d2_modkit::ModStack;
use d2_sim::rules::data_mod::blocks::{self, FileKind};
use d2_sim::rules::Ruleset;

/// The files with block names, and the kind the vocabulary reads them as.
const NAMED: [(&str, FileKind); 3] = [
    ("files/skills.toml", FileKind::Skills),
    ("files/triggers.toml", FileKind::Triggers),
    ("files/quests.toml", FileKind::Quests),
];

/// The text of `relative` in mod `m` (a folder or a pack); `None` when the mod
/// has no such file.
pub(super) fn read_file(m: &LoadedMod, relative: &str) -> anyhow::Result<Option<String>> {
    crate::kit_mod::read_file(m, relative).map_err(Into::into)
}

pub(super) fn check(stack: &ModStack, report: &mut Report) -> Option<Ruleset> {
    names(stack, report);
    let rules = host(stack, report);
    report
        .completed
        .push("building-block names and rules files".into());
    rules
}

fn names(stack: &ModStack, report: &mut Report) {
    for m in &stack.mods {
        for (relative, kind) in NAMED {
            let location = format!("{}/{relative}", m.dir.display());
            let text = match read_file(m, relative) {
                Ok(Some(text)) => text,
                Ok(None) => continue,
                Err(e) => {
                    report.finding(
                        Severity::Error,
                        "unreadable building-block files",
                        &location,
                        format!("{e:#}"),
                    );
                    continue;
                }
            };
            // A text that is not TOML is reported by the host check below.
            let Ok(problems) = blocks::check(kind, &text, m.manifest.api) else {
                continue;
            };
            for p in problems {
                let (severity, category) = match p.severity {
                    blocks::Severity::Error => (Severity::Error, "unknown or too-new blocks"),
                    blocks::Severity::Warning => (Severity::Warning, "deprecated blocks"),
                };
                report.finding(
                    severity,
                    category,
                    &format!("{location}: {}", p.at),
                    p.message,
                );
            }
        }
    }
}

/// The host the stack plays under: each file comes from the last mod that has
/// it, as at play.
fn host(stack: &ModStack, report: &mut Report) -> Option<Ruleset> {
    match crate::kit_mod::host(stack) {
        Ok(host) => Some(
            if stack
                .mods
                .iter()
                .any(|m| m.manifest.kind == d2_modkit::Kind::Full)
            {
                host.into_ruleset()
            } else {
                Ruleset::Original
            },
        ),
        Err(error) => {
            report.finding(
                Severity::Error,
                "invalid building-block files",
                "files",
                error.to_string(),
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn manifest(api: u32) -> String {
        format!(
            "id = \"kit-test\"\nname = \"Kit Test\"\nversion = \"1.0.0\"\nauthor = \"Nugwa\"\n\
             description = \"fixture\"\napi = {api}\nkind = \"full\"\n"
        )
    }

    fn stack(tag: &str, files: &[(&str, &str)]) -> (ModStack, PathBuf) {
        let dir = std::env::temp_dir().join(format!("check-kit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        put(&dir, "mod.toml", &manifest(d2_modkit::CURRENT_API));
        for (path, text) in files {
            put(&dir, path, text);
        }
        let manifest =
            d2_modkit::Manifest::parse(&manifest(d2_modkit::CURRENT_API)).expect("manifest");
        let stack = ModStack {
            mods: vec![LoadedMod {
                manifest,
                dir: dir.clone(),
            }],
        };
        (stack, dir)
    }

    fn put(dir: &Path, path: &str, text: &str) {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn run(tag: &str, files: &[(&str, &str)]) -> Report {
        let (stack, dir) = stack(tag, files);
        let mut report = Report::new("kit-test".into());
        check(&stack, &mut report);
        let _ = std::fs::remove_dir_all(dir);
        report
    }

    #[test]
    fn a_mod_with_no_kit_files_passes() {
        let report = run("none", &[]);
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert_eq!(report.exit_code, 0);
        assert!(report
            .completed
            .contains(&"building-block names and rules files".to_owned()));
    }

    #[test]
    fn runtime_only_declarations_are_validated_too() {
        for (name, file) in [
            ("ai", "files/ai.toml"),
            ("acts", "files/acts.toml"),
            ("difficulties", "files/difficulties.toml"),
        ] {
            let report = run(name, &[(file, "version=999\n")]);
            assert_eq!(report.exit_code, 1, "{file}: {:?}", report.findings);
            assert!(
                report
                    .findings
                    .iter()
                    .any(|f| f.category == "invalid building-block files"),
                "{file}: {:?}",
                report.findings
            );
        }
    }

    #[test]
    fn a_known_skill_passes() {
        let skills = "[[skill]]\nid = 390\n[[skill.part]]\nkind = \"area_hit\"\nradius = 4\n\
                      damage = { min = 1, max = 2 }\n";
        let report = run("known", &[("files/skills.toml", skills)]);
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    #[test]
    fn an_unknown_skill_part_is_named_with_a_suggestion() {
        let skills = "[[skill]]\nid = 390\n[[skill.part]]\nkind = \"aera_hit\"\nradius = 4\n";
        let report = run("unknown", &[("files/skills.toml", skills)]);
        assert_eq!(report.exit_code, 1);
        let names: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.category == "unknown or too-new blocks")
            .collect();
        assert_eq!(names.len(), 1, "{:?}", report.findings);
        assert!(names[0]
            .location
            .ends_with("skills.toml: skill[0].part[0].kind"));
        assert!(names[0].message.contains("did you mean `area_hit`?"));
        // The host check reports the file as the game would refuse it.
        assert!(report
            .findings
            .iter()
            .any(|f| f.category == "invalid building-block files"));
    }

    #[test]
    fn an_unknown_trigger_action_is_reported() {
        let triggers = "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n\
                        [[trigger.action]]\nkind = \"apply_stat\"\nstate = 1\n";
        let report = run("trigger", &[("files/triggers.toml", triggers)]);
        assert_eq!(report.exit_code, 1);
        assert!(report
            .findings
            .iter()
            .any(|f| f.category == "unknown or too-new blocks"
                && f.message.contains("did you mean `apply_state`?")));
    }

    #[test]
    fn a_state_that_would_never_end_is_reported_by_the_host_check() {
        let skills = "[[skill]]\nid = 390\n[[skill.part]]\nkind = \"state\"\nstate = 380\n";
        let states = "[[state]]\nid = 380\n";
        let report = run(
            "never-ends",
            &[("files/skills.toml", skills), ("files/states.toml", states)],
        );
        assert_eq!(report.exit_code, 1);
        assert!(report.findings.iter().any(
            |f| f.category == "invalid building-block files" && f.message.contains("never end")
        ));
    }

    #[test]
    fn the_vocabulary_is_not_newer_than_the_api() {
        const { assert!(blocks::VOCABULARY_API <= d2_modkit::CURRENT_API) };
        assert!(blocks::all().all(|b| b.since <= d2_modkit::CURRENT_API));
    }
}
