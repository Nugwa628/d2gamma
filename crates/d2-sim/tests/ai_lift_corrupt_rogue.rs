// D2RUST's own: mod-kit tooling.
//! The lifted CorruptRogue AI (`templates/ai-lift-corrupt-rogue`) is a mod folder whose `ai.toml` reads and holds the graph of AI row
//! 10.

use std::path::Path;

use d2_sim::rules::data_mod::AiRules;

#[test]
fn the_corrupt_rogue_lift_is_a_mod_folder_with_a_graph_that_reads() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/ai-lift-corrupt-rogue");
    let folder = d2_modkit::ModFolder::scan(&dir).unwrap();
    assert_eq!(folder.manifest.id, "ai-lift-corrupt-rogue");
    let paths: Vec<&str> = folder.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["files/ai.toml"], "the ai file and no game files");
    let text = std::fs::read_to_string(dir.join("files/ai.toml")).unwrap();
    let rules = AiRules::from_toml(&text).unwrap_or_else(|e| panic!("{e}"));
    assert!(rules.get(10).is_some(), "no graph for AI row 10");
    assert!(dir.join("README.md").is_file());
}
