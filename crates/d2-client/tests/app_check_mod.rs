//! Synthetic folder/pack checks use the production CLI/library and native load.
use d2_client::app::check_mod::{self, Options, Report, Severity};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn native() -> &'static Path {
    static NATIVE: OnceLock<PathBuf> = OnceLock::new();
    NATIVE.get_or_init(|| {
        let install = super::app_sweep::install_with("check-mod", |data| {
            // Native shape checks require 708 slots, but the padding is not
            // additional playable content. Keep the three synthetic classes
            // and the vendor, without generating 700 duplicate animation cases.
            let monsters = data.tables.files.get_mut("monstats.txt").unwrap();
            let enabled = monsters
                .columns
                .iter()
                .position(|c| c == "enabled")
                .unwrap();
            for (row, monster) in monsters.rows.iter_mut().enumerate().skip(3) {
                if row != 148 {
                    monster[enabled] = "0".into();
                }
            }
        });
        for archive in d2_convert::convert::REQUIRED_ARCHIVES {
            let path = install.join(archive);
            if !path.exists() {
                d2_formats::mpq::writer::MpqWriter::new()
                    .write(path)
                    .unwrap();
            }
        }
        let native = scratch("native");
        let options = d2_convert::Options::new(&install, &native);
        let converted = d2_convert::convert(&options, &d2_convert::kinds::builtin()).unwrap();
        assert!(converted.failures.is_empty(), "{:?}", converted.failures);
        // Checks must ignore the installed layer order, just like --mod-dir.
        put(&native, "mods/order.toml", "order = [\"absent\"]\n");
        native
    })
}

fn scratch(tag: &str) -> PathBuf {
    // CI reuses target/tmp and process IDs. Keep one fresh root per run,
    // while repeated calls with the same tag still address the same fixture.
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("check-mod-{}-{nonce}", std::process::id()))
    })
    .join(tag)
}
fn put(dir: &Path, path: &str, text: &str) {
    let path = dir.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}
fn manifest() -> String {
    format!("id = \"check-test\"\nname = \"Check Test\"\nversion = \"1.0.0\"\nauthor = \"Nugwa\"\ndescription = \"Synthetic check fixture\"\napi = {}\nkind = \"full\"\n", d2_modkit::CURRENT_API)
}
fn folder(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    let _ = std::fs::remove_dir_all(&dir);
    put(&dir, "mod.toml", &manifest());
    put(&dir, "files/rules.toml", "version = 1\n");
    dir
}

fn custom_act_fixture(tag: &str) -> PathBuf {
    let dir = map_fixture(tag);
    let maps = std::fs::read_to_string(dir.join("files/levels.toml"))
        .unwrap()
        .replace("id=1\n", "id=137\n")
        .replace("id=2\n", "id=138\n")
        .replace("to=1\n", "to=137\n")
        .replace("to=2\n", "to=138\n");
    put(&dir, "files/levels.toml", &maps);
    put(&dir, "files/acts.toml", "version=1\n[[acts]]\nname='Lantern'\nstart=137\nend=139\ntown=137\nwaypoints=[{id=0,level=137}]\n");
    let bytes = std::fs::read(native().join("base/data/global/excel/levels.txt")).unwrap();
    let rules = d2_data::patch::rules_for(d2_data::bin::DataRules::Mod);
    let rule = rules.iter().find(|r| r.name == "levels").unwrap();
    let table = d2_data::patch::PatchTable::from_base(rule, "test", &bytes).unwrap();
    let mut levels = String::from("table levels\n");
    for (id, old) in [(137, 1), (138, 2)] {
        let key = String::from_utf8(d2_data::patch::canonical_token(table.key(old))).unwrap();
        let id_is_key = table.header[table.key_col].eq_ignore_ascii_case(b"Id");
        let new_key = if id_is_key {
            id.to_string()
        } else {
            format!("Lantern{id}")
        };
        levels.push_str(&format!("add #{id} [{new_key}] like #{old} {key}\n"));
        if !id_is_key {
            levels.push_str(&format!("set [{new_key}] [Id] [{old}] -> [{id}]\n"));
        }
    }
    put(
        &dir,
        "files/patches/strength.d2patch",
        &format!(
            "d2patch 1\n{}{levels}",
            edit_row("charstats", 0, "str", "52")
        ),
    );
    dir
}

#[test]
fn autoplay_loads_active_mod_layers_and_server_rules() {
    use d2_client::app::autoplay_host::{Host, HostArgs};
    let dir = custom_act_fixture("autoplay-active-mod");
    d2_client::kit_mod::activate(&d2_modkit::ModSpec::Dir(dir)).unwrap();
    let args = HostArgs {
        native: Some(native().to_path_buf()),
        new: Some(("amazon".into(), "Lantern".into())),
        seed: Some(42),
        ..Default::default()
    };
    let mut host = Host::start(&args).unwrap();
    let mod_rules = host
        .server()
        .lock()
        .unwrap()
        .with(|link| {
            !link
                .host_mut()
                .game
                .events
                .action
                .hooks()
                .rules
                .is_original()
        })
        .unwrap();
    assert!(
        mod_rules,
        "autoplay must use the active mod server rules and wire"
    );
    let state = host.state().unwrap();
    let player = state["snap"]["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|unit| unit["ut"] == 0 && unit["g"] == state["local"])
        .expect("local player");
    assert_eq!(player["str"], 52, "{state}");
    assert_eq!(player["lv"], 137, "{state}");
    let map = host
        .run(d2_client::app::autoplay_host::Command::Map)
        .unwrap()
        .unwrap();
    assert!(
        map["levels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|level| level["id"] == 137),
        "{map}"
    );
    let archives = HostArgs {
        source: Some("mpq".into()),
        ..args
    };
    let error = Host::start(&archives)
        .err()
        .expect("mods cannot use archives");
    assert!(error.to_string().contains("converted native"), "{error}");
}

#[test]
fn explicit_repro_rules_and_checkpoints_preserve_custom_stats() {
    use d2_client::app::repro::{RunOpts, Session};
    use d2_server::game::{GameData, GameSource, Layers};
    use d2_sim::rules::data_mod::{DataMod, DataModFiles};
    let rules = DataMod::from_files(
        "checked",
        &DataModFiles {
            stats: Some((
                "stats.toml",
                "version=1\n[[stat]]\nname='resonance'\nid=40000\n",
            )),
            ..Default::default()
        },
    )
    .unwrap()
    .into_ruleset();
    let data = GameData::open_with(
        &GameSource::Native(native().into(), Layers::List(vec![])),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let mut session = Session::from_data(data);
    let scenario = d2_client::scenario_script::Scenario::parse("scenario 2\nname checked-custom-stat\nend 10\nchar class 0\nchar stat 40000 17\nat 5 checkpoint\n").unwrap();
    let report = session
        .run(
            &scenario,
            &RunOpts {
                rules: Some(rules),
                out: scratch("explicit-mod-rules"),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(report.exit, 0, "{}", report.json);
}

#[test]
fn relative_check_mod_target_uses_the_requested_folder() {
    let relative = PathBuf::from(format!("check-mod-relative-{}", std::process::id()));
    let absolute = std::env::current_dir().unwrap().join(&relative);
    put(&absolute, "mod.toml", &manifest());
    put(&absolute, "files/rules.toml", "version=1\n");
    let result = check_mod::check(&options(Some(relative), "relative-target"));
    std::fs::remove_dir_all(&absolute).unwrap();
    let report = result.unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
}

#[test]
fn visual_graph_check_mod_reports_the_source_node_and_checks_signed_packs() {
    let dir = folder("visual-graph");
    let graph="version=1\n[[nodes]]\nid='entry'\nkind='event'\nevent='kill'\nnext='message'\n[[nodes]]\nid='message'\nkind='api'\nname='message'\n[nodes.inputs]\nunit={node='entry',pin='unit'}\ntext='A friendly echo'";
    put(&dir, "files/scripts/echo.graph.toml", graph);
    let report = check_mod::check(&options(Some(dir.clone()), "valid-graph")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    let pack = scratch("visual-graph").with_extension("d2pack");
    d2_native::package::package(&dir, native(), &pack, &scratch("visual-graph-key")).unwrap();
    let report = check_mod::check(&options(Some(pack), "valid-packed-graph")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    put(
        &dir,
        "files/scripts/echo.graph.toml",
        &graph.replace("name='message'", "name='unknown_api'"),
    );
    let report = check_mod::check(&options(Some(dir), "bad-graph-node")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.category == "invalid scripts"
                && f.location.contains("echo.graph.toml")
                && f.location.contains("node message")),
        "{}",
        report.text()
    );
}

#[test]
fn mod_panels_reach_the_play_source_and_check_mod_rejects_invalid_ui() {
    let dir = folder("ui-documents");
    put(
        &dir,
        "files/ui/counter.toml",
        "version=1\nname='counter'\n[[elements]]\nid='label'\nkind='text'\ntext='Resource'\n[elements.bind]\nsource='stat.strength'\n",
    );
    put(
        &dir,
        "files/patches/ui.d2patch",
        &format!(
            "d2patch 1\n{}",
            edit_row("itemstatcost", 15, "stat", "hud_resource")
        ),
    );
    let text = std::fs::read_to_string(dir.join("files/ui/counter.toml")).unwrap();
    put(
        &dir,
        "files/ui/counter.toml",
        &text.replace("stat.strength", "stat.hud_resource"),
    );
    let entries = vec![dir.to_string_lossy().into_owned()];
    let game_files = d2_client::assets::game_files::GameFiles::native_under(
        native(),
        Some(&entries),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let panels = d2_client::ui::data_panels::load(game_files.source().as_ref()).unwrap();
    assert_eq!(panels.len(), 1);
    assert_eq!(panels[0].name, "counter");
    assert_eq!(
        panels[0].elements[0].binding.as_ref().unwrap().source,
        "stat.15"
    );
    let report = check_mod::check(&options(Some(dir.clone()), "valid-ui-binding")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    put(
        &dir,
        "files/ui/counter.toml",
        "version=999\nname='counter'\n",
    );
    let report = check_mod::check(&options(Some(dir), "invalid-ui")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report
        .findings
        .iter()
        .any(|f| f.category == "invalid UI" && f.message.contains("counter.toml")));
}
#[test]
fn ui_action_validator_links_against_checked_scripts() {
    let dir = folder("ui-actions");
    let panel="version=1\nname='hud'\n[[actions]]\nname='allocate'\nstat='strength'\ndelta=1\n[actions.validator]\nmodule='allow.luau'\nfunction='allow'";
    put(&dir, "files/ui/actions.toml", panel);
    let report = check_mod::check(&options(Some(dir.clone()), "missing-ui-validator")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report
        .findings
        .iter()
        .any(|f| f.category == "invalid UI actions" && f.message.contains("unknown validator")));
    put(
        &dir,
        "files/scripts/allow.luau",
        "return { allow = function(value) return value < 100 and 1 or 0 end }",
    );
    let report = check_mod::check(&options(Some(dir), "valid-ui-validator")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
}
fn options(target: Option<PathBuf>, tag: &str) -> Options {
    Options {
        target,
        native: native().into(),
        mods_dir: scratch("installed"),
        out: scratch(&format!("out-{tag}")),
        jobs: 4,
        full: false,
    }
}

/// A guarded row-0 edit using the fixture's real identity and old cell value.
fn edit(table: &str, column: &str, value: &str) -> String {
    edit_row(table, 0, column, value)
}
fn edit_row(table: &str, row: usize, column: &str, value: &str) -> String {
    let bytes =
        std::fs::read(native().join(format!("base/data/global/excel/{table}.txt"))).unwrap();
    let txt = d2_data::txt::TxtTable::parse(table, &bytes).unwrap();
    let c = txt
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(column.as_bytes()))
        .unwrap();
    let rules = d2_data::patch::rules_for(d2_data::bin::DataRules::Mod);
    let rule = rules.iter().find(|r| r.name == table).unwrap();
    let patched = d2_data::patch::PatchTable::from_base(rule, "test", &bytes).unwrap();
    let token = |s: &[u8]| String::from_utf8(d2_data::patch::canonical_token(s)).unwrap();
    format!(
        "table {table}\nset #{row} {} {} {} -> {}\n",
        token(patched.key(row)),
        token(column.as_bytes()),
        token(&txt.records[row].cells[c]),
        token(value.as_bytes())
    )
}

fn errors(report: &Report) -> String {
    report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("{} {} {}", f.category, f.location, f.message))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn check_mod_base_and_clean_folder_and_pack_pass() {
    for (tag, target) in [("base", None), ("clean", Some(folder("clean")))] {
        let report = check_mod::check(&options(target, tag)).unwrap();
        assert_eq!(report.exit_code, 0, "{}", report.text());
        assert!(report.sweeps.is_empty(), "unchanged rows need no gameplay");
        assert!(report.changed_rows.is_empty(), "{:?}", report.changed_rows);
        assert!(report
            .completed
            .iter()
            .any(|s| s == "item generation sweep"));
        let json = serde_json::to_vec(&report).unwrap();
        let decoded: Report = serde_json::from_slice(&json).unwrap();
        assert_eq!(decoded.format_version, 1);
        assert_eq!(decoded.exit_code, 0);
    }
    let dir = folder("pack-source");
    let pack = scratch("clean-pack").with_extension("d2pack");
    d2_native::pack::build(&dir, &pack, 1).unwrap();
    let unsigned = check_mod::check(&options(Some(pack.clone()), "unsigned-pack")).unwrap();
    assert_eq!(unsigned.exit_code, 1);
    assert!(unsigned.text().contains("no author signature"));
    d2_native::package::package(&dir, native(), &pack, &scratch("clean-pack-key")).unwrap();
    let report = check_mod::check(&options(Some(pack), "pack")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(report.sweeps.is_empty());
}

#[test]
fn check_mod_names_sandboxed_script_failures() {
    let dir = folder("script-errors");
    put(&dir, "files/scripts/broken.luau", "return {");
    let report = check_mod::check(&options(Some(dir.clone()), "script-errors")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(errors(&report).contains("files/scripts/broken.luau"));
    put(
        &dir,
        "files/scripts/broken.luau",
        "return { value = function() return 42 end }",
    );
    let report = check_mod::check(&options(Some(dir), "script-repaired")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(report
        .completed
        .iter()
        .any(|s| s == "sandboxed script load"));
}

#[test]
fn check_mod_checks_declared_script_exports() {
    let dir = folder("script-calls");
    put(&dir, "files/triggers.toml", "[[trigger]]\non='skill_used'\nwhile_stat=12\n[[trigger.action]]\nkind='call_script'\nmodule='main.luau'\nfunction='run'\n");
    put(&dir, "files/scripts/main.luau", "return {}");
    let report = check_mod::check(&options(Some(dir.clone()), "missing-export")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(errors(&report).contains("main.luau::run"));
    let pack = scratch("script-calls").with_extension("d2pack");
    d2_native::package::package(&dir, native(), &pack, &scratch("script-call-pack-key")).unwrap();
    let report = check_mod::check(&options(Some(pack.clone()), "missing-pack-export")).unwrap();
    assert!(
        errors(&report).contains("main.luau::run"),
        "{}",
        report.text()
    );
    put(
        &dir,
        "files/scripts/main.luau",
        "return {run=function(e,a) return 1 end}",
    );
    let report = check_mod::check(&options(Some(dir.clone()), "repaired-export")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    d2_native::package::package(&dir, native(), &pack, &scratch("script-call-pack-key")).unwrap();
    let report = check_mod::check(&options(Some(pack), "repaired-pack-export")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
}

#[test]
fn check_mod_checks_named_stat_script_exports() {
    let dir = folder("named-script-calls");
    put(&dir, "files/stats.toml", "[[stat]]\nname='focus'\n");
    put(&dir, "files/triggers.toml", "[[trigger]]\non='skill_used'\nwhile_stat='focus'\n[[trigger.action]]\nkind='call_script'\nmodule='main.luau'\nfunction='run'\n");
    put(&dir, "files/scripts/main.luau", "return {}");
    let report = check_mod::check(&options(Some(dir.clone()), "named-missing-export")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(errors(&report).contains("main.luau::run"));
    let pack = scratch("named-script-calls").with_extension("d2pack");
    d2_native::package::package(
        &dir,
        native(),
        &pack,
        &scratch("named-script-call-pack-key"),
    )
    .unwrap();
    let report =
        check_mod::check(&options(Some(pack.clone()), "named-missing-pack-export")).unwrap();
    assert!(
        errors(&report).contains("main.luau::run"),
        "{}",
        report.text()
    );
    put(
        &dir,
        "files/scripts/main.luau",
        "return {run=function(e,a) return 1 end}",
    );
    let report = check_mod::check(&options(Some(dir.clone()), "named-repaired-export")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    d2_native::package::package(
        &dir,
        native(),
        &pack,
        &scratch("named-script-call-pack-key"),
    )
    .unwrap();
    let report = check_mod::check(&options(Some(pack), "named-repaired-pack-export")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
}

#[test]
fn check_mod_bad_manifest_and_api_are_content_errors() {
    for (tag, text, expected) in [
        ("manifest", "bad = true".to_owned(), "manifest"),
        ("api", manifest().replace("api = 1", "api = 999"), "newer"),
    ] {
        let dir = folder(tag);
        put(&dir, "mod.toml", &text);
        let report = check_mod::check(&options(Some(dir), tag)).unwrap();
        assert_eq!(report.exit_code, 1);
        assert!(errors(&report).contains(expected), "{}", report.text());
    }
}

#[test]
fn check_mod_patch_reference_asset_rule_and_cube_errors_are_named() {
    let cases = [
        (
            "item-generation",
            format!(
                "d2patch 1\n{}",
                edit_row("itemratio", 1, "UniqueDivisor", "0")
            ),
            "broken item generation rows",
        ),
        ("patch", "not a patch\n".into(), "patch"),
        (
            "reference",
            format!("d2patch 1\n{}", edit("weapons", "type", "zz_bad_type")),
            "zz_bad_type",
        ),
        (
            "missing-art",
            format!(
                "d2patch 1\n{}",
                edit("weapons", "invfile", "zz_missing_art")
            ),
            "zz_missing_art",
        ),
        (
            "skills",
            format!(
                "d2patch 1\n{}",
                edit("skills", "srvmissile", "zz_missing_missile")
            ),
            "zz_missing_missile",
        ),
        (
            "monster",
            format!(
                "d2patch 1\n{}",
                edit("monstats", "TreasureClass1", "zz_missing_tc")
            ),
            "zz_missing_tc",
        ),
        (
            "cube",
            format!("d2patch 1\n{}", edit("cubemain", "numinputs", "99")),
            "broken cube recipes",
        ),
    ];
    for (tag, patch, expected) in cases {
        let dir = folder(tag);
        put(&dir, "files/patches/test.d2patch", &patch);
        let report = check_mod::check(&options(Some(dir), tag)).unwrap();
        assert_eq!(report.exit_code, 1, "{}", report.text());
        assert!(
            errors(&report).contains(expected),
            "expected {expected}: {}",
            report.text()
        );
    }
    let dir = folder("rules-and-asset");
    put(&dir, "files/rules.toml", "version = 999\n");
    put(&dir, "files/data/global/zz_bad_sound.wav", "not wave data");
    let report = check_mod::check(&options(Some(dir), "rules-and-asset")).unwrap();
    assert_eq!(report.exit_code, 1);
    let text = errors(&report);
    assert!(
        text.contains("rules.toml") && text.contains("zz_bad_sound.wav"),
        "{text}"
    );
    let dir = folder("raw-asset");
    put(
        &dir,
        "files/data/global/items/zz_raw.dc6",
        "unconverted asset",
    );
    let report = check_mod::check(&options(Some(dir), "raw-asset")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(
        errors(&report).contains("invalid native layers"),
        "{}",
        report.text()
    );
}

#[test]
fn check_mod_fixed_minimum_monster_hp_is_valid() {
    let dir = folder("fixed-monster-hp");
    put(
        &dir,
        "files/patches/test.d2patch",
        &format!("d2patch 1\n{}", edit("monstats", "MinHP", "30000")),
    );
    let report = check_mod::check(&options(Some(dir), "fixed-monster-hp")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(report.findings.iter().any(|f| f.severity == Severity::Info
        && f.category == "inverted monster stat ranges"
        && f.location == "monstats row 0"));
    assert_eq!(
        report.changed_rows["monstats"],
        std::collections::BTreeSet::from([0])
    );
    assert_eq!(report.sweeps.len(), 1);
    assert_eq!(report.sweeps["anims"]["cases"][0]["case"], "monster-0");
}

#[test]
fn check_mod_cli_writes_json_on_content_failure_and_uses_exit_two_for_usage() {
    let dir = folder("cli-bad");
    put(&dir, "files/rules.toml", "version = 999\n");
    let out = scratch("cli-out");
    let args = [
        dir.display().to_string(),
        "--native".into(),
        native().display().to_string(),
        "--out".into(),
        out.display().to_string(),
    ];
    assert_eq!(check_mod::main(&args), 1);
    let report: Report =
        serde_json::from_slice(&std::fs::read(out.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report.text().contains("Error:"));
    assert_eq!(check_mod::main(&[]), 2);
    assert_eq!(
        check_mod::main(&["--base".into(), "--jobs".into(), "0".into()]),
        2
    );
}

#[test]
#[ignore = "needs converted base install in D2_NATIVE_DIR"]
fn check_mod_real_base_passes() {
    let native = std::env::var_os("D2_NATIVE_DIR")
        .expect("D2_NATIVE_DIR")
        .into();
    let out = scratch("real-out");
    let report = check_mod::check(&Options {
        target: None,
        native,
        mods_dir: scratch("real-installed"),
        out: out.clone(),
        jobs: 4,
        full: false,
    })
    .unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(report.sweeps.is_empty());
}

#[test]
fn check_mod_full_runs_all_gameplay_families() {
    let mut o = options(None, "full");
    o.full = true;
    let report = check_mod::check(&o).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert_eq!(report.gameplay_scope, "full");
    assert_eq!(report.sweeps.len(), 7);
}

#[test]
fn check_mod_item_patch_runs_only_that_items_scenarios() {
    let dir = folder("target-item");
    put(
        &dir,
        "files/patches/test.d2patch",
        &format!("d2patch 1\n{}", edit("weapons", "cost", "1234")),
    );
    let report = check_mod::check(&options(Some(dir), "target-item")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert_eq!(report.sweeps.len(), 1);
    let cases = report.sweeps["items"]["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    assert!(cases
        .iter()
        .all(|c| c["case"].as_str().unwrap().starts_with("item-0-")));
}

#[test]
fn check_mod_formula_edit_targets_only_that_skill() {
    let dir = folder("target-skill");
    put(
        &dir,
        "files/patches/test.d2patch",
        &format!(
            "d2patch 1\n{}",
            edit_row("skills", 2, "calc1", "lvl*par1+1")
        ),
    );
    let report = check_mod::check(&options(Some(dir), "target-skill")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert_eq!(
        report.changed_rows["skills"],
        std::collections::BTreeSet::from([2])
    );
    let cases = report.sweeps["skills"]["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    assert!(cases
        .iter()
        .all(|c| c["case"].as_str().unwrap().contains("-skill-2-")));
}

#[test]
fn check_mod_added_item_targets_new_combined_identity() {
    let table = "weapons";
    let bytes =
        std::fs::read(native().join(format!("base/data/global/excel/{table}.txt"))).unwrap();
    let rules = d2_data::patch::rules_for(d2_data::bin::DataRules::Mod);
    let rule = rules.iter().find(|r| r.name == table).unwrap();
    let original = d2_data::patch::PatchTable::from_base(rule, "test", &bytes).unwrap();
    let txt = d2_data::txt::TxtTable::parse(table, &bytes).unwrap();
    let id = txt.records.len();
    let key = String::from_utf8(d2_data::patch::canonical_token(original.key(0))).unwrap();
    let dir = folder("added-item");
    put(
        &dir,
        "files/patches/test.d2patch",
        &format!(
            "d2patch 1\ntable weapons\nadd #{id} nk9 like {key} {}\n",
            original.pin(0)
        ),
    );
    let report = check_mod::check(&options(Some(dir), "added-item")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert_eq!(
        report.changed_rows["weapons"],
        std::collections::BTreeSet::from([id])
    );
    let cases = report.sweeps["items"]["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    assert!(cases.iter().all(|c| c["case"]
        .as_str()
        .unwrap()
        .starts_with(&format!("item-{id}-"))));
}

// ---- The template mods under `templates/` ----

const TEMPLATE_MODS: [&str; 3] = ["no-hit-recovery", "harder-fire", "new-unique-item"];

fn template_source(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../templates")
        .join(name)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A fresh copy of template `name`, as a modder would start from it.
fn template_copy(name: &str, tag: &str) -> PathBuf {
    let dir = scratch(&format!("template-{name}-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&template_source(name), &dir);
    dir
}

fn template_rules(name: &str) -> d2_sim::rules::global::GlobalRules {
    let text = std::fs::read_to_string(template_source(name).join("files/rules.toml")).unwrap();
    d2_sim::rules::global::GlobalRules::from_toml(&text).unwrap()
}

#[test]
fn templates_are_mod_folders_for_the_current_api() {
    for name in TEMPLATE_MODS {
        let folder = d2_modkit::ModFolder::scan(&template_source(name)).unwrap();
        assert_eq!(folder.manifest.id, name);
        assert_eq!(folder.manifest.kind, d2_modkit::Kind::Full, "{name}");
        assert_eq!(folder.manifest.api, d2_modkit::CURRENT_API, "{name}");
        assert!(template_source(name).join("README.md").is_file(), "{name}");
    }
}

#[test]
fn no_hit_recovery_template_changes_only_hit_recovery() {
    let rules = template_rules("no-hit-recovery");
    assert!(!rules.players.hit_recovery.enabled);
    assert!(!rules.monsters.hit_recovery.enabled);
    // Everything else is D2Gamma's.
    let mut rest = rules.clone();
    rest.players.hit_recovery.enabled = true;
    rest.monsters.hit_recovery.enabled = true;
    assert_eq!(rest, d2_sim::rules::global::GlobalRules::default());
}

#[test]
fn harder_fire_template_changes_only_resistances() {
    let rules = template_rules("harder-fire");
    let default = d2_sim::rules::global::GlobalRules::default();
    assert!(rules.resistances.elemental_cap < default.resistances.elemental_cap);
    assert!(rules.resistances.elemental_max_cap < default.resistances.elemental_max_cap);
    assert!(rules.resistances.elemental_cap <= rules.resistances.elemental_max_cap);
    assert_eq!(rules.resistances.expansion_penalties, Some([0, -60, -120]));
    let mut rest = rules;
    rest.resistances = default.resistances.clone();
    assert_eq!(rest, default);
}

#[test]
fn rules_only_templates_pass_check_mod() {
    for name in ["no-hit-recovery", "harder-fire"] {
        let report =
            check_mod::check(&options(Some(template_copy(name, "synthetic")), name)).unwrap();
        assert_eq!(report.exit_code, 0, "{name}: {}", report.text());
        assert!(report.changed_rows.is_empty(), "{name}: rules only");
        assert!(report.sweeps.is_empty(), "{name}: no table changed");
    }
}

#[test]
fn new_unique_item_patches_are_well_formed() {
    let patches = template_source("new-unique-item").join("files/patches");
    let mut seen = 0;
    for entry in std::fs::read_dir(&patches).unwrap() {
        let path = entry.unwrap().path();
        let bytes = std::fs::read(&path).unwrap();
        let (_, findings) = d2_data::patch::parse_layer(&path.display().to_string(), &bytes, 1);
        assert!(findings.is_empty(), "{}: {findings:?}", path.display());
        seen += 1;
    }
    assert_eq!(seen, 2, "a string patch and a table patch");
}

#[test]
#[ignore = "needs converted base install in D2_NATIVE_DIR"]
fn templates_pass_check_mod_on_the_real_base() {
    let native: PathBuf = std::env::var_os("D2_NATIVE_DIR")
        .expect("D2_NATIVE_DIR")
        .into();
    for name in TEMPLATE_MODS {
        let mut o = options(Some(template_copy(name, "real")), &format!("real-{name}"));
        o.native = native.clone();
        let report = check_mod::check(&o).unwrap();
        assert_eq!(report.exit_code, 0, "{name}: {}", report.text());
        if name == "new-unique-item" {
            assert!(
                report.changed_rows.contains_key("uniqueitems"),
                "{:?}",
                report.changed_rows
            );
        }
    }
}

#[test]
fn publication_checks_staged_content_and_leaves_no_pack_or_key_on_error() {
    use d2_client::app::package_mod::package_checked;
    let root = folder("package-errors");
    let out = scratch("refused.d2pack");
    let key = scratch("refused.pk8");
    let report_dir = scratch("publication-report");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&key);
    put(&root, "files/rules.toml", "not valid rules = [");
    assert!(package_checked(
        &root,
        native(),
        &out,
        &key,
        scratch("installed"),
        report_dir.clone()
    )
    .is_err());
    let report: Report =
        serde_json::from_slice(&std::fs::read(report_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(!out.exists());
    assert!(!key.exists());
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_D2Gamma-Game"))
        .arg("package-mod")
        .arg(&root)
        .arg("--base")
        .arg(native())
        .arg("--out")
        .arg(&out)
        .arg("--report")
        .arg(&report_dir)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert_eq!(
        cli.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    assert!(String::from_utf8_lossy(&cli.stdout).contains("Mod check:"));
    assert!(!out.exists());
    put(&root, "files/rules.toml", "version = 1\n");
    package_checked(
        &root,
        native(),
        &out,
        &key,
        scratch("installed"),
        report_dir.clone(),
    )
    .unwrap();
    assert!(out.exists());
    assert!(key.exists());
    let report: Report =
        serde_json::from_slice(&std::fs::read(report_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.exit_code, 0);
    d2_modkit::publish::verify(&d2_native::pack::Pack::open(&out).unwrap()).unwrap();
    let good_pack = std::fs::read(&out).unwrap();
    std::fs::copy(
        native().join("base/data/global/excel/weapons.txt"),
        root.join("files/copied-base-file"),
    )
    .unwrap();
    assert!(package_checked(
        &root,
        native(),
        &out,
        &key,
        scratch("installed"),
        report_dir.clone()
    )
    .is_err());
    let report: Report =
        serde_json::from_slice(&std::fs::read(report_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(
        report.exit_code, 1,
        "an old successful report must be replaced"
    );
    assert!(report.text().contains("unmodified base file"));
    assert_eq!(
        std::fs::read(&out).unwrap(),
        good_pack,
        "refusal keeps the prior completed pack"
    );
}

// Generated fixtures only: no game tiles or table data are checked in.
fn map_fixture(tag: &str) -> PathBuf {
    use d2_formats::dt1::{Dt1, Dt1Block, Dt1Tile};
    use d2_native::kind::NativeKind;
    let dir = folder(tag);
    let mut ds1 = test_fixtures::ds1::blank(17, 17, 0);
    ds1.floors[0].fill(0x0010_0001);
    put(
        &dir,
        "files/data/global/tiles/test.ds1.toml",
        &d2_native::toml_kinds::write_ds1("test.ds1", &ds1).unwrap(),
    );
    let mut piece = test_fixtures::ds1::blank(3, 3, 0);
    piece.floors[0].fill(0x100001);
    piece.objects.push(d2_formats::ds1::Ds1Object {
        kind: 2,
        id: 150,
        x: 2,
        y: 3,
        flags: 0,
    });
    piece.walls[0][4] = 0x8070_0000;
    piece.orientations[0][4] = 10;
    put(
        &dir,
        "files/data/global/tiles/test-piece.ds1.toml",
        &d2_native::toml_kinds::write_ds1("test-piece.ds1", &piece).unwrap(),
    );
    let dt1 = Dt1 {
        version: 7,
        minor_version: 6,
        tiles: vec![Dt1Tile {
            light_direction: 0,
            roof_height: 0,
            material_flags: 0,
            height: 15,
            width: 32,
            unknown_height: 0,
            orientation: 0,
            main_index: 1,
            sub_index: 0,
            rarity: 1,
            unknown_color: 0,
            subtile_flags: [0; 25],
            unknown_58: 0,
            cache_index: 0,
            unknown_5c: 0,
            blocks: vec![Dt1Block {
                x: 0,
                y: 0,
                unknown1: 0,
                grid_x: 0,
                grid_y: 0,
                format: 1,
                unknown2: 0,
                pixels: (0..15)
                    .flat_map(|y| {
                        let half = if y < 8 { 2 * (y + 1) } else { 2 * (15 - y) };
                        (0..32).map(move |x| {
                            if (16 - half..16 + half).contains(&x) {
                                80
                            } else {
                                0
                            }
                        })
                    })
                    .collect(),
            }],
        }],
    };
    for f in dt1
        .write(
            "data/global/tiles/test.dt1",
            &d2_native::kind::grey_palette(),
        )
        .unwrap()
    {
        let p = dir.join("files").join(f.path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, f.bytes).unwrap();
    }
    put(
        &dir,
        "files/levels.toml",
        r#"version=1
[[level]]
id=1
size=[16,16]
offset=[100,100]
entry=[2,2]
files=['test.ds1']
tiles=['test.dt1']
[[level.preset]]
path='test-piece.ds1'
at=[10,2]
[[level.warp]]
slot=0
to=2
at=[13,13]
[[level.object]]
class=0
at=[5,5]
[[level.spawn]]
monster=0
rect=[6,6,4,4]
count=3
[[level]]
id=2
size=[16,16]
offset=[200,200]
entry=[2,2]
files=['test.ds1']
tiles=['test.dt1']
[[level.warp]]
slot=1
to=1
at=[3,3]
"#,
    );
    dir
}
#[test]
fn authored_maps_load_generate_and_stream_from_folder_and_pack() {
    use d2_server::game::{GameData, GameSource, Layers, LevelSource};
    use d2_sim::drlg::{Drlg, Services};
    let dir = map_fixture("map-source");
    let pack = scratch("map-pack").with_extension("d2pack");
    d2_native::package::package(&dir, native(), &pack, &scratch("map-pack-key")).unwrap();
    let mut hashes = Vec::new();
    for location in [dir.clone(), pack] {
        let source = GameSource::Native(
            native().into(),
            Layers::List(vec![location.display().to_string()]),
        );
        let GameData::Live(base) = GameData::open(&source).unwrap();
        assert!(base.maps.level.is_empty());
        let GameData::Live(live) =
            GameData::open_with(&source, d2_data::bin::DataRules::Mod).unwrap();
        assert_eq!(live.maps.level.len(), 2);
        hashes.push(live.data_hash());
        assert_ne!(live.data_hash(), base.data_hash());
        let levels = LevelSource::live(&live, 42);
        let mut types = levels.shared.clone();
        let mut drlg = Drlg::create(0, 42, 0, 1, false, &levels.data, &mut types).unwrap();
        let mut game = d2_sim::game::Game::new();
        game.lists.ensure_act(0).unwrap();
        for level in [1, 2] {
            let l = drlg
                .get_or_alloc_level(&levels.data, &mut types, level)
                .unwrap();
            drlg.generate_level(&levels.data, &mut types, l).unwrap();
            let rect = drlg.level(l).rect;
            assert_eq!(
                (rect.x, rect.y, rect.w, rect.h),
                if level == 1 {
                    (100, 100, 16, 16)
                } else {
                    (200, 200, 16, 16)
                }
            );
            for r in drlg.level_rooms(l) {
                let mut svc = Services {
                    data: &levels.data,
                    tiles: &live.files.dt1,
                    types: &mut types,
                    rooms: &mut game.lists,
                };
                drlg.stream_room(&mut svc, r).unwrap();
            }
            assert!(drlg.level(l).spawn_tiles.iter().any(|t| t.index == 0));
            assert!(drlg
                .level_rooms(l)
                .iter()
                .any(|r| !drlg.room(*r).warp_links.is_empty()));
        }
        assert!(
            types.borrow().errors.is_empty(),
            "{:?}",
            types.borrow().errors
        );
        let p = types.borrow();
        let units: Vec<_> = p
            .presets
            .values()
            .flat_map(|p| {
                drlg.find_level(1)
                    .into_iter()
                    .flat_map(|l| drlg.level_rooms(l))
                    .flat_map(|r| p.room_units(r).iter())
            })
            .collect();
        assert!(units.iter().any(|u| u.unit_type == 2 && u.class == 0));
        assert!(
            units
                .iter()
                .filter(|u| u.unit_type == 1 && u.class == 0)
                .count()
                >= 3
        );
    }
    assert_eq!(hashes[0], hashes[1]);
    let path = dir.join("files/levels.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("size=[16,16]", "size=[17,16]")).unwrap();
    let report = check_mod::check(&options(Some(dir), "map-size-invalid")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report.text().contains("DS1 version/size"));
}

fn map_add_blocking_tiles(dir: &Path) {
    use d2_formats::dt1::Dt1;
    use d2_native::kind::{MemStore, NativeKind};
    let names = [
        "data/global/tiles/test.dt1.toml",
        "data/global/tiles/test.dt1.d/0.png",
    ];
    let store = MemStore(
        names
            .into_iter()
            .map(|p| (p.into(), std::fs::read(dir.join("files").join(p)).unwrap()))
            .collect(),
    );
    let mut tiles = Dt1::read("data/global/tiles/test.dt1", &store).unwrap();
    let mut wall = tiles.tiles[0].clone();
    wall.main_index = 2;
    wall.orientation = 1;
    wall.subtile_flags = [1; 25];
    tiles.tiles.push(wall);
    let mut barrier = tiles.tiles[0].clone();
    barrier.main_index = 3;
    barrier.subtile_flags = [1; 25];
    tiles.tiles.push(barrier);
    for file in tiles
        .write(
            "data/global/tiles/test.dt1",
            &d2_native::kind::grey_palette(),
        )
        .unwrap()
    {
        std::fs::write(dir.join("files").join(file.path), file.bytes).unwrap();
    }
}
#[test]
fn random_maps_use_the_same_seed_realization_in_preview_and_runtime() {
    use d2_server::game::{GameData, GameSource, Layers, LevelSource};
    use d2_sim::drlg::{
        maps::{generation::Generation, Maps},
        Drlg,
    };
    let dir = map_fixture("map-seeds");
    map_add_blocking_tiles(&dir);
    let path = dir.join("files/levels.toml");
    let mut maps = Maps::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for generation in [
        Generation::Maze {
            cell: 3,
            floor: 0x100001,
            wall: 0x200001,
            orientation: 1,
        },
        Generation::Outdoor {
            density: 37,
            floor: 0x100001,
            wall: 0x200001,
            orientation: 1,
        },
    ] {
        for l in &mut maps.level {
            l.generation = generation.clone();
        }
        std::fs::write(&path, maps.to_toml().unwrap()).unwrap();
        let source = GameSource::Native(
            native().into(),
            Layers::List(vec![dir.display().to_string()]),
        );
        let GameData::Live(live) =
            GameData::open_with(&source, d2_data::bin::DataRules::Mod).unwrap();
        let mut layouts = std::collections::BTreeSet::new();
        for seed in 0..32 {
            let levels = LevelSource::live(&live, seed);
            let mut types = levels.shared.clone();
            let _runtime = Drlg::create(0, seed, 0, 0, false, &levels.data, &mut types).unwrap();
            for id in [1, 2] {
                let preview = maps.preview(id, seed, &live.files.ds1).unwrap();
                let handle = types.borrow();
                let alias = Maps::alias(id);
                assert_eq!(handle.ds1.ds1(&alias), Some(&preview.map));
                assert_eq!(handle.ds1.extra_units(&alias), preview.units);
                if id == 1 {
                    layouts.insert(preview.map.walls);
                }
            }
        }
        assert!(
            layouts.len() > 24,
            "only {} distinct layouts",
            layouts.len()
        );
    }
}

#[test]
fn check_mod_maps_reports_connectivity_and_real_collision_from_folder_and_pack() {
    use d2_sim::drlg::maps::Maps;
    let dir = map_fixture("map-checks");
    map_add_blocking_tiles(&dir);
    let path = dir.join("files/levels.toml");
    let good = Maps::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let report = check_mod::check(&options(Some(dir.clone()), "map-good")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(report
        .completed
        .iter()
        .any(|s| s.contains("reachable warps (1 seeds")));
    let pack = scratch("map-checked-pack").with_extension("d2pack");
    d2_native::package::package(&dir, native(), &pack, &scratch("map-checked-key")).unwrap();
    assert_eq!(
        check_mod::check(&options(Some(pack), "map-good-pack"))
            .unwrap()
            .exit_code,
        0
    );
    let mut bad = good.clone();
    bad.level[1].warp[0].to = 3;
    put(&dir, "files/levels.toml", &bad.to_toml().unwrap());
    let report = check_mod::check(&options(Some(dir.clone()), "map-no-return")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report.text().contains("no return link"));
    let mut bad = good.clone();
    for l in &mut bad.level {
        l.warp.clear();
    }
    put(&dir, "files/levels.toml", &bad.to_toml().unwrap());
    let report = check_mod::check(&options(Some(dir.clone()), "map-disconnected")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report.text().contains("not reachable from the act town"));
    let mut overlapping = good.clone();
    overlapping.level[1].offset = overlapping.level[0].offset;
    put(&dir, "files/levels.toml", &overlapping.to_toml().unwrap());
    let report = check_mod::check(&options(Some(dir.clone()), "map-overlap")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report.text().contains("overlaps level 2"));
    put(&dir, "files/levels.toml", &good.to_toml().unwrap());
    let source = dir.join("files/data/global/tiles/test.ds1.toml");
    let original =
        d2_native::toml_kinds::read_ds1("test", &std::fs::read_to_string(&source).unwrap())
            .unwrap();
    let mut ds1 = original.clone();
    for y in 0..17 {
        ds1.floors[0][y * 17 + 8] = 0x300001;
    }
    std::fs::write(
        &source,
        d2_native::toml_kinds::write_ds1("test", &ds1).unwrap(),
    )
    .unwrap();
    let report = check_mod::check(&options(Some(dir.clone()), "map-barrier")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(
        report
            .text()
            .contains("warp slot 0 to level 2 is unreachable"),
        "{}",
        report.text()
    );
    assert!(report.text().contains("level 1, seed 0"));
    std::fs::write(
        dir.join("files/data/global/tiles/blocked.ds1.toml"),
        d2_native::toml_kinds::write_ds1("blocked", &ds1).unwrap(),
    )
    .unwrap();
    // A preset registry can also vary by seed: inspect alternatives, not only seed zero.
    let mut variants = good.clone();
    variants.level[0].files.push("blocked.ds1".into());
    std::fs::write(
        &source,
        d2_native::toml_kinds::write_ds1("test", &original).unwrap(),
    )
    .unwrap();
    put(&dir, "files/levels.toml", &variants.to_toml().unwrap());
    let report = check_mod::check(&options(Some(dir.clone()), "map-bad-variant")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(
        report.text().contains("is unreachable"),
        "{}",
        report.text()
    );
    assert!(report
        .completed
        .iter()
        .any(|s| s.contains("reachable warps (32 seeds")));
    put(&dir, "files/levels.toml", &good.to_toml().unwrap());
    let mut ds1 = original.clone();
    ds1.floors[0][2 * 17 + 2] = 0x300001;
    std::fs::write(
        &source,
        d2_native::toml_kinds::write_ds1("test", &ds1).unwrap(),
    )
    .unwrap();
    let report = check_mod::check(&options(Some(dir.clone()), "map-entry-blocked")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report.text().contains("entry is blocked"));
    std::fs::write(
        &source,
        d2_native::toml_kinds::write_ds1("test", &original).unwrap(),
    )
    .unwrap();
    let mut random = good;
    for generation in [
        d2_sim::drlg::maps::generation::Generation::Maze {
            cell: 3,
            floor: 0x100001,
            wall: 0x200001,
            orientation: 1,
        },
        d2_sim::drlg::maps::generation::Generation::Outdoor {
            density: 37,
            floor: 0x100001,
            wall: 0x200001,
            orientation: 1,
        },
    ] {
        for l in &mut random.level {
            l.generation = generation.clone();
        }
        put(&dir, "files/levels.toml", &random.to_toml().unwrap());
        let report = check_mod::check(&options(Some(dir.clone()), "map-random")).unwrap();
        assert_eq!(report.exit_code, 0, "{}", report.text());
        assert!(report
            .completed
            .iter()
            .any(|s| s.contains("reachable warps (32 seeds")));
    }
}

#[test]
fn mod_effects_reach_native_play_source_and_check_rejects_bad_recipes() {
    let dir = folder("effects");
    let text="version=1\n[[effects]]\nname='ember'\n[effects.light]\nradius=8\n[effects.particles]\ncount=20";
    put(&dir, "files/effects/ember.toml", text);
    let entries = vec![dir.to_string_lossy().into_owned()];
    let files = d2_client::assets::game_files::GameFiles::native_under(
        native(),
        Some(&entries),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let e = d2_client::world_view::mod_effects::ModEffects::load(files.source().as_ref()).unwrap();
    assert_eq!(e.recipes.len(), 1);
    let report = check_mod::check(&options(Some(dir.clone()), "valid-effects")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    put(
        &dir,
        "files/effects/ember.toml",
        &text.replace("radius=8", "radius=19"),
    );
    let report = check_mod::check(&options(Some(dir), "bad-effects")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report
        .findings
        .iter()
        .any(|f| f.category == "invalid effects" && f.message.contains("ember.toml")));
}

#[test]
fn mod_effects_resolve_patched_native_missile_names() {
    let dir = folder("named-effects");
    put(
        &dir,
        "files/patches/effect-name.d2patch",
        &format!(
            "d2patch 1\n{}",
            [
                edit_row("missiles", 1, "Missile", "custom_ember"),
                edit_row("skills", 2, "srvmissile", "custom_ember")
            ]
            .concat()
        ),
    );
    put(
        &dir,
        "files/effects/name.toml",
        "version=1\n[[effects]]\nname='test'\nmissile='custom_ember'",
    );
    let entries = vec![dir.to_string_lossy().into_owned()];
    let files = d2_client::assets::game_files::GameFiles::native_under(
        native(),
        Some(&entries),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let e = d2_client::world_view::mod_effects::ModEffects::load(files.source().as_ref()).unwrap();
    assert_eq!(e.recipes["test"].missile, Some(1));
}

#[test]
fn network_contract_check_links_patched_values_actions_ui_and_scripts_for_folder_and_pack() {
    let dir = folder("network-contract");
    put(
        &dir,
        "files/patches/net.d2patch",
        &format!(
            "d2patch 1\n{}",
            edit_row("itemstatcost", 15, "stat", "net_resource")
        ),
    );
    let contract="version=1\n[[values]]\nid=1\nname='resource'\nsource='stat.net_resource'\n[[values]]\nid=2\nname='rage'\nsource='var.allow/rage'\n[[actions]]\nid=7\nname='allocate'\nstat='net_resource'\ndelta=1\nmaximum=100\n[actions.validator]\nmodule='allow.luau'\nfunction='allow'";
    put(&dir, "files/net.toml", contract);
    put(
        &dir,
        "files/scripts/allow.luau",
        "return { allow = function(value) return value < 100 and 1 or 0 end }",
    );
    put(&dir,"files/ui/net.toml","version=1\nname='counter'\n[[elements]]\nid='value'\nkind='text'\n[elements.bind]\nsource='sync.resource'\n[[elements]]\nid='allocate'\nkind='button'\naction='net.allocate'");
    let report = check_mod::check(&options(Some(dir.clone()), "valid-network")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(report.completed.iter().any(|s| s == "mod network contract"));
    let pack = scratch("net-contract").with_extension("d2pack");
    d2_native::package::package(&dir, native(), &pack, &scratch("network-contract-key")).unwrap();
    let packed = check_mod::check(&options(Some(pack.clone()), "valid-network-pack")).unwrap();
    assert_eq!(packed.exit_code, 0, "{}", packed.text());
    let entries = vec![pack.to_string_lossy().into_owned()];
    let files = d2_client::assets::game_files::GameFiles::native_under(
        native(),
        Some(&entries),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let network = d2_server::mod_network::Network::load(files.source().as_ref()).unwrap();
    assert_eq!(network.definition.values[0].source, "stat.net_resource");
    let panels = d2_client::ui::data_panels::load(files.source().as_ref()).unwrap();
    assert_eq!(panels[0].elements[1].action.as_deref(), Some("net.7"));
    put(
        &dir,
        "files/net.toml",
        &contract.replace("source='stat.net_resource'", "source='stat.no_such_stat'"),
    );
    let report = check_mod::check(&options(Some(dir.clone()), "bad-network-stat")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report
        .findings
        .iter()
        .any(|f| f.category == "invalid network contract" && f.message.contains("no_such_stat")));
    put(
        &dir,
        "files/net.toml",
        &contract.replace("function='allow'", "function='missing'"),
    );
    let report = check_mod::check(&options(Some(dir), "bad-network-script")).unwrap();
    assert_eq!(report.exit_code, 1);
    assert!(report.findings.iter().any(
        |f| f.category == "invalid network contract" && f.message.contains("unknown validator")
    ));
}

#[test]
fn visual_echo_template_is_graph_only_and_checks_as_folder_and_signed_pack() {
    assert_eq!(
        template_rules("visual-echo"),
        d2_sim::rules::global::GlobalRules::default()
    );
    let dir = template_copy("visual-echo", "graph-only");
    let folder = d2_modkit::ModFolder::scan(&dir).unwrap();
    assert!(folder
        .files
        .iter()
        .any(|f| f.path == "files/scripts/echo.graph.toml"));
    assert!(!folder.files.iter().any(|f| f.path.ends_with(".luau")));
    let report = check_mod::check(&options(Some(dir.clone()), "visual-echo-folder")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(report.changed_rows.is_empty());
    let entries = vec![dir.to_string_lossy().into_owned()];
    let game_files = d2_client::assets::game_files::GameFiles::native_under(
        native(),
        Some(&entries),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let panels = d2_client::ui::data_panels::load(game_files.source().as_ref()).unwrap();
    assert_eq!(
        panels[0].elements[0].binding.as_ref().unwrap().source,
        "var.echo.graph.toml/pulse"
    );
    let pack = scratch("visual-echo-pack").with_extension("d2pack");
    d2_native::package::package(&dir, native(), &pack, &scratch("visual-echo-key")).unwrap();
    let report = check_mod::check(&options(Some(pack), "visual-echo-packed")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
}

#[test]
fn check_mod_and_runtime_both_enforce_screen_only_addon_payloads() {
    let base = folder("screen-addon-base");
    put(
        &base,
        "mod.toml",
        &format!("{}approved_addons=['screen-hud']\n", manifest()),
    );
    let installed = scratch("screen-addon-installed");
    let addon = installed.join("screen-hud");
    put(&addon,"mod.toml","id='screen-hud'\nname='Screen HUD'\nversion='1.0.0'\nauthor='Nugwa'\ndescription=''\napi=1\nkind='addon'\nfor='check-test'\nscreen_only=true\n");
    put(
        &addon,
        "files/ui/hud.toml",
        "version=1\nname='screen-hud'\n",
    );
    put(
        &installed,
        "addons.toml",
        "[enabled]\ncheck-test=['screen-hud']\n",
    );
    let mut o = options(Some(base), "screen-addon");
    o.mods_dir = installed;
    assert_eq!(check_mod::check(&o).unwrap().exit_code, 0);
    for file in [
        "rules.toml",
        "net.toml",
        "scripts/cheat.graph.toml",
        "tables/cheat.txt",
    ] {
        let path = addon.join("files").join(file);
        put(&addon, &format!("files/{file}"), "version=1");
        let report = check_mod::check(&o).unwrap();
        assert_eq!(report.exit_code, 1, "{}", report.text());
        assert!(report.text().contains("screen-only"), "{}", report.text());
        assert!(
            d2_native::package::open_mod(&addon, d2_modkit::api::Date::today())
                .unwrap_err()
                .to_string()
                .contains("screen-only")
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn plain_addons_require_owner_approval_and_keep_plain_rules_and_wire_identity() {
    let installed = scratch("plain-addon-installed");
    let addon = installed.join("plain-hud");
    put(&addon, "mod.toml", "id='plain-hud'\nname='Plain HUD'\nversion='1.0.0'\nauthor='Tester'\ndescription=''\napi=1\nkind='addon'\nfor='d2gamma'\nscreen_only=true");
    put(&addon, "files/ui/hud.toml", "version=1\nname='plain-hud'");
    let native_root = scratch("plain-addon-native");
    std::fs::create_dir_all(&native_root).unwrap();
    assert!(d2_client::kit_mod::plain_layers(&native_root, &installed)
        .unwrap()
        .is_empty());
    put(
        &installed,
        "addons.toml",
        "[enabled]\nd2gamma=['plain-hud']",
    );
    assert!(d2_client::kit_mod::plain_layers(&native_root, &installed)
        .unwrap_err()
        .to_string()
        .contains("owner approval"));
    put(&native_root, "plain-addons.toml", "approved=['plain-hud']");
    let entries = d2_client::kit_mod::plain_layers(&native_root, &installed).unwrap();
    assert_eq!(entries.len(), 1);
    let mut source = d2_native::source::NativeSource::open_with(native(), Some(&[])).unwrap();
    source.layers = d2_native::layers::LayerStack::with_mods(&native_root, &entries).unwrap();
    use d2_server::file_source::FileSource;
    assert!(source.mod_versions().is_empty());
    assert!(source.server_ui_documents().unwrap().is_empty());
    assert_eq!(source.ui_documents().unwrap().len(), 1);
    assert!(source.net_document().unwrap().is_none());
    put(&native_root, "plain-addons.toml", "approved=[]");
    assert!(d2_client::kit_mod::plain_layers(&native_root, &installed).is_err());
}

#[test]
fn addon_authoring_and_signed_packaging_do_not_grant_activation_permission() {
    let addon = scratch("plain-addon-authoring");
    put(&addon, "mod.toml", "id='plain-hud'\nname='Plain HUD'\nversion='1.0.0'\nauthor='Tester'\ndescription=''\napi=1\nkind='addon'\nfor='d2gamma'\nscreen_only=true");
    put(&addon, "files/ui/hud.toml", "version=1\nname='plain-hud'");
    let o = options(Some(addon.clone()), "plain-addon-authoring");
    let report = check_mod::check(&o).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(report
        .text()
        .contains("activation still requires owner approval"));
    let pack = scratch("plain-addon-authoring-pack").with_extension("d2pack");
    d2_client::app::package_mod::package_checked(
        &addon,
        native(),
        &pack,
        &scratch("plain-addon-authoring-key"),
        o.mods_dir.clone(),
        scratch("plain-addon-package-report"),
    )
    .unwrap();
    let report = check_mod::check(&options(Some(pack.clone()), "plain-addon-packed")).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    assert!(d2_native::package::open_mod(&pack, d2_modkit::api::Date::today()).is_ok());
    assert!(
        d2_native::layers::LayerStack::with_mods(native(), &[pack.display().to_string()])
            .unwrap_err()
            .to_string()
            .contains("owner approval")
    );
    put(&addon, "files/scripts/bad.luau", "return {}");
    assert_eq!(check_mod::check(&o).unwrap().exit_code, 1);
}

#[test]
fn addon_authoring_checks_target_maker_approval_without_enabling_it() {
    let installed = scratch("addon-maker-authoring");
    let base = installed.join("check-test");
    put(
        &base,
        "mod.toml",
        &format!("{}approved_addons=['test-addon']\n", manifest()),
    );
    let addon = scratch("mod-addon-authoring");
    put(&addon, "mod.toml", "id='test-addon'\nname='Test Add-on'\nversion='1.0.0'\nauthor='Tester'\ndescription=''\napi=1\nkind='addon'\nfor='check-test'");
    put(&addon, "files/ui/hud.toml", "version=1\nname='test-hud'");
    let mut o = options(Some(addon), "mod-addon-authoring");
    o.mods_dir = installed;
    let report = check_mod::check(&o).unwrap();
    assert_eq!(report.exit_code, 0, "{}", report.text());
    put(&base, "mod.toml", &manifest());
    assert!(check_mod::check(&o)
        .unwrap()
        .text()
        .contains("has not approved"));
}
#[test]
fn custom_monster_cofs_supply_missing_animation_timing_without_replacing_base_records() {
    use d2_data::bin::DataRules;
    use d2_server::game::client_tables::{client_monster_anims, client_unit_rows};
    use d2_server::game::{GameData, GameSource, Layers};
    let dir = folder("cof-animation");
    put(
        &dir,
        "files/patches/token.d2patch",
        &format!("d2patch 1\n{}", edit_row("monstats", 0, "Code", "ZQ")),
    );
    let cof = "native='cof'\nnative_version=1\nversion=20\nunknown='0x00000000'\nframes=4\ndirections=1\nx_min=-8\nx_max=8\ny_min=-16\ny_max=0\nanimation_rate=256\nevents=[0,1,0,0]\nevent_padding=[]\ndraw_order=[[1,1,1,1]]\n[[layer]]\ncomponent=1\nshadow=1\nselectable=1\noverride_translucency=0\nnew_translucency=0\nweapon_class='hth '\n";
    for weapon in ["hth", "1hs"] {
        put(
            &dir,
            &format!("files/data/global/monsters/zq/cof/zqa1{weapon}.cof.toml"),
            cof,
        );
    }
    let source = GameSource::Native(
        native().into(),
        Layers::List(vec![dir.display().to_string()]),
    );
    let GameData::Live(plain) = GameData::open_with(&source, DataRules::Original).unwrap();
    assert!(!plain.tables.anim.info(b"ZQA1HTH").unwrap().found);
    assert_eq!(
        plain.tables.anim.record(b"ZQA1HTH").unwrap(),
        &d2_formats::animdata::DEFAULT_RECORD
    );
    let GameData::Live(modded) = GameData::open_with(&source, DataRules::Mod).unwrap();
    use d2_server::local_seams::UnitAppearance;
    let names =
        d2_server::game::appearance::AnimationNames::load(modded.archives.as_ref()).unwrap();
    let key = names
        .anim_key(d2_sim::units::UnitType::Monster, 0, 4, 0)
        .unwrap();
    let record = modded.tables.anim.record(&key).unwrap();
    assert_eq!((record.frames, record.speed), (4, 256));
    assert_eq!(&record.events[..4], &[0, 1, 0, 0]);
    for bucket in &plain.tables.anim.buckets {
        for record in bucket {
            assert_eq!(modded.tables.anim.find(&record.name).unwrap(), Some(record));
        }
    }
    let mut rows = client_unit_rows(modded.archives.as_ref()).unwrap();
    client_monster_anims(modded.archives.as_ref(), &mut rows).unwrap();
    let monster = rows.monsters[0].as_ref().unwrap();
    assert_eq!(monster.anims[4], Some((4, 256)));
    assert_eq!(monster.anim_events[4].unwrap()[1], 1);
}

#[test]
fn packed_runtime_host_loads_the_same_declarations_as_the_folder() {
    let dir = folder("packed-runtime-host");
    put(
        &dir,
        "files/rules.toml",
        "[monsters.hit_recovery]\nenabled=false",
    );
    put(
        &dir,
        "files/stats.toml",
        "[[stat]]\nname='resonance'\nid=40000",
    );
    put(
        &dir,
        "files/states.toml",
        "[[state]]\nid=380\n[[state.effect]]\nstat='resonance'\nper_stack=7",
    );
    put(&dir, "files/acts.toml", "version=1\n[[acts]]\nname='Lantern'\nstart=137\nend=139\ntown=137\nwaypoints=[{id=0,level=137}]");
    let pack = scratch("packed-runtime-host").with_extension("d2pack");
    d2_native::package::package(&dir, native(), &pack, &scratch("packed-runtime-host-key"))
        .unwrap();
    for path in [dir, pack] {
        let stack = d2_modkit::resolve::resolve_with(
            &d2_modkit::ModSpec::Dir(path),
            &scratch("runtime-host-installed"),
            d2_modkit::api::Date::today(),
            d2_native::package::open_mod,
        )
        .unwrap();
        let host = d2_client::kit_mod::host(&stack).unwrap();
        assert!(!host.global().monsters.hit_recovery.enabled);
        assert_eq!(host.state_rules().get(380).unwrap().effects[0].stat, 40000);
        let rules = host.into_ruleset();
        assert_eq!(rules.stat_rules().unwrap().id("resonance"), Some(40000));
        assert_eq!(rules.acts().unwrap().town(0), Some(137));
    }
}

#[test]
fn generated_sweeps_use_configured_towns_fields_and_waypoints() {
    use d2_client::app::sweep;
    let dir = map_fixture("configured-sweep-world");
    put(&dir, "files/acts.toml", "version=1\n[[acts]]\nname='Reversed'\nstart=1\nend=3\ntown=2\nwaypoints=[{id=0,level=2},{id=1,level=1}]");
    let stack = d2_modkit::resolve::resolve_with(
        &d2_modkit::ModSpec::Dir(dir),
        &scratch("sweep-world-installed"),
        d2_modkit::api::Date::today(),
        d2_native::package::open_mod,
    )
    .unwrap();
    let rules = d2_client::kit_mod::host(&stack).unwrap().into_ruleset();
    let data = d2_server::game::GameData::open_with(
        &d2_server::game::GameSource::Native(
            native().into(),
            d2_server::game::Layers::List(stack.layer_entries()),
        ),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let d2_server::game::GameData::Live(live) = data;
    let cases = sweep::generate_with_rules(&live, "all", &rules)
        .unwrap()
        .cases;
    let named = |name: &str| &cases.iter().find(|c| c.name == name).expect(name).text;
    assert!(named("class-0-town-neutral").contains("char area 0 2"));
    assert!(named("monster-0").contains("char area 0 1"));
    assert!(named("waypoint-to-1").contains("char area 0 2"));
    assert!(cases
        .iter()
        .filter(|c| c.family == "skills")
        .any(|c| c.text.contains("char area 0 1")));
    assert!(!cases
        .iter()
        .filter(|c| c.family == "areas")
        .any(|c| c.name.starts_with("act-1-")));
    let plain = sweep::generate(&live, "anims").unwrap();
    assert!(plain
        .cases
        .iter()
        .find(|c| c.name == "class-0-town-neutral")
        .unwrap()
        .text
        .contains("char area 0 1"));
    assert!(plain
        .cases
        .iter()
        .find(|c| c.name == "monster-0")
        .unwrap()
        .text
        .contains("char area 0 2"));
}

#[test]
fn headless_clients_handshake_before_accepting_synced_values_and_survive_timeout() {
    use d2_client::app::{
        ai_arena,
        autoplay_host::{Host, HostArgs},
    };
    use d2_client::bridge::BridgeResource;
    use d2_server::game::{GameData, GameSource, Layers};
    let dir = map_fixture("headless-mod-network");
    put(&dir, "files/acts.toml", "version=1\n[[acts]]\nname='Network test'\nstart=1\nend=3\ntown=1\nwaypoints=[{id=0,level=1},{id=1,level=2}]");
    put(
        &dir,
        "files/net.toml",
        "version=1\n[[values]]\nid=1\nname='might'\nsource='stat.strength'",
    );
    let stack = d2_client::kit_mod::activate(&d2_modkit::ModSpec::Dir(dir)).unwrap();
    let mut host = Host::start(&HostArgs {
        native: Some(native().into()),
        new: Some(("amazon".into(), "Network".into())),
        seed: Some(42),
        ..Default::default()
    })
    .unwrap();
    host.step(400);
    assert_eq!(host.sent_counts().get(&0x71), Some(&1));
    let bridge = &host.app_mut().world().resource::<BridgeResource>().0;
    assert!(!bridge.world().rules.0.is_original());
    assert_eq!(bridge.world().synced_values.get("might"), Some(&15));
    let data = GameData::open_with(
        &GameSource::Native(native().into(), Layers::List(stack.layer_entries())),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let trace = ai_arena::run_on(
        &ai_arena::ArenaArgs {
            class: 0,
            ai: None,
            name: None,
            seed: 7,
            ticks: 400,
            distance: 15,
            level: 0,
            difficulty: 0,
            out: None,
            native: None,
            game_dir: None,
            hurt: Vec::new(),
        },
        data,
        d2_client::kit_mod::ruleset(),
        &d2_sim::rules::data_mod::AiRules::none(),
        None,
    )
    .unwrap();
    assert_eq!(trace.samples.len(), 400);
}

#[test]
fn plain_headless_client_ignores_mod_contract_and_sends_no_envelope() {
    use d2_client::app::autoplay_host::Host;
    use d2_client::bridge::BridgeResource;
    use d2_server::game::{GameData, GameSource, Layers};
    let dir = map_fixture("plain-headless-network");
    put(
        &dir,
        "files/net.toml",
        "version=1\n[[values]]\nid=1\nname='might'\nsource='stat.strength'",
    );
    let plain_data = GameData::open_with(
        &GameSource::Native(
            native().into(),
            Layers::List(vec![dir.display().to_string()]),
        ),
        d2_data::bin::DataRules::Original,
    )
    .unwrap();
    let mut plain = Host::start_with(
        plain_data,
        d2_client::app::single_player::new_character("amazon", "Plain").unwrap(),
        None,
        false,
        Some(42),
    )
    .unwrap();
    plain.step(400);
    assert!(!plain.sent_counts().contains_key(&0x71));
    let b = &plain.app_mut().world().resource::<BridgeResource>().0;
    assert_eq!(b.wire(), d2_proto::envelope::Wire::Original);
    assert!(b.world().synced_values.is_empty());
    drop(plain);
}

#[test]
fn explicit_repro_rules_configure_client_world_before_join() {
    use d2_client::app::repro::{RunOpts, Session};
    use d2_server::game::{GameData, GameSource, Layers};
    let dir = custom_act_fixture("explicit-client-world");
    put(
        &dir,
        "files/net.toml",
        "version=1\n[[values]]\nid=1\nname='might'\nsource='stat.strength'",
    );
    let stack = d2_modkit::resolve::resolve_with(
        &d2_modkit::ModSpec::Dir(dir),
        &scratch("explicit-world-installed"),
        d2_modkit::api::Date::today(),
        d2_native::package::open_mod,
    )
    .unwrap();
    let rules = d2_client::kit_mod::host(&stack).unwrap().into_ruleset();
    assert!(
        d2_client::kit_mod::ruleset().is_original(),
        "no active global mod"
    );
    let data = GameData::open_with(
        &GameSource::Native(native().into(), Layers::List(stack.layer_entries())),
        d2_data::bin::DataRules::Mod,
    )
    .unwrap();
    let mut session = Session::from_data(data);
    // A field join beyond the handshake timeout needs the explicit act on
    // both server and client; the process-wide default remains Original.
    let scenario = d2_client::scenario_script::Scenario::parse(
        "scenario 2\nname explicit-client-world\nend 400\nchar class 0\nchar area 0 138\nat 5 checkpoint\n",
    ).unwrap();
    let report = session
        .run(
            &scenario,
            &RunOpts {
                rules: Some(rules),
                out: scratch("explicit-client-world-report"),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(report.exit, 0, "{}", report.json);
}
