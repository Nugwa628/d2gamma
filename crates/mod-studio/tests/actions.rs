//! Tool boundaries use invented reports and processes, without game assets.
mod support;
#[cfg(unix)]
use mod_studio::actions::{Event, Job};
use mod_studio::{
    actions::{Jump, Problem, Report, Severity},
    table_ui::TableEditor,
    tables::Tables,
    ToolCommand,
};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::Duration;

const REPORT: &str = r#"{"format_version":1,"target":"/stage","exit_code":1,"completed":["references"],"findings":[
{"severity":"info","category":"summary","location":"all tables","message":"data checked"},
{"severity":"warning","category":"Unverified","location":"skills row 1","message":"candidate"},
{"severity":"error","category":"references","location":"effective tables","message":"error[C01] mods/ember/files/patches/edit.d2patch:4:1: weapons eb1 (#0) type: unknown reference"}]}"#;

#[test]
fn commands_use_headless_game_and_preserve_paths_and_installed_mods() {
    let game = Path::new("C:/Game tools/D2Gamma-Game.exe");
    let root = Path::new("C:/My mods/Ember age");
    let native = Path::new("C:/Game data/native");
    let report = Path::new("C:/Reports/new check");
    let installed = Path::new("C:/Installed mods");
    let output = Path::new("C:/Packs/Ember age.d2pack");
    let check = ToolCommand::check(game, root, native, report, installed);
    assert_eq!(check.program, game);
    assert_eq!(
        check.args,
        [
            "check-mod",
            root.to_str().unwrap(),
            "--native",
            native.to_str().unwrap(),
            "--out",
            report.to_str().unwrap(),
            "--mods-dir",
            installed.to_str().unwrap()
        ]
        .map(std::ffi::OsString::from)
    );
    let package = ToolCommand::package_checked(game, root, native, output, report, installed);
    assert_eq!(package.program, game);
    assert_eq!(
        package.args,
        [
            "package-mod",
            root.to_str().unwrap(),
            "--base",
            native.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
            "--mods-dir",
            installed.to_str().unwrap()
        ]
        .map(std::ffi::OsString::from)
    );
    assert_eq!(
        ToolCommand::check(game, root, native, report, Path::new(""))
            .args
            .len(),
        6
    );
}

#[test]
fn json_problems_group_by_severity_and_reject_unknown_or_malformed_reports() {
    let report = Report::parse(REPORT).unwrap();
    assert_eq!(
        report
            .findings
            .iter()
            .map(|p| p.severity)
            .collect::<Vec<_>>(),
        [Severity::Error, Severity::Warning, Severity::Info]
    );
    assert!(
        Report::parse(&REPORT.replace("\"format_version\":1", "\"format_version\":2")).is_err()
    );
    assert!(Report::parse("{}").is_err());
    assert!(Report::parse(&REPORT.replace("\"warning\"", "\"fatal\"")).is_err());
    assert_eq!(
        Report::parse(&REPORT.replace("\"exit_code\":1", "\"exit_code\":0"))
            .unwrap()
            .exit_code,
        1
    );
    let multiple=REPORT.replace("unknown reference","unknown reference; error[C01] mods/ember/files/patches/edit.d2patch:5:1: weapons fs1 (#1) type: another reference");
    assert_eq!(Report::parse(&multiple).unwrap().findings.len(), 4);
}

#[test]
fn native_diagnostics_and_data_sweeps_jump_to_actual_rows_and_cells() {
    let report = Report::parse(REPORT).unwrap();
    assert_eq!(
        report.findings[0].jump(Path::new("/stage"), Path::new("/project")),
        Some(Jump::Table {
            table: "weapons".into(),
            row: 0,
            column: Some("type".into())
        })
    );
    let mut problem = report.findings[1].clone();
    assert_eq!(
        problem.jump(Path::new("/stage"), Path::new("/project")),
        Some(Jump::Table {
            table: "skills".into(),
            row: 1,
            column: None
        })
    );
    problem.location = "effective tables".into();
    problem.message =
        "weapons.txt row #2 (ss1) column `invfile` value `missing`: missing asset".into();
    assert_eq!(
        problem.jump(Path::new("/stage"), Path::new("/project")),
        Some(Jump::Table {
            table: "weapons".into(),
            row: 2,
            column: Some("invfile".into())
        })
    );
    let mut editor = TableEditor::default();
    editor.data = Some(Tables::from_data(support::data(), PathBuf::from("unused")));
    editor.jump("weapons", 2, Some("level")).unwrap();
    assert_eq!(editor.selected, 0);
    assert!(editor.jump("weapons", 100, Some("level")).is_err());
    assert!(editor.jump("absent", 0, None).is_err());
    problem.message="error[C01] mods/ember/files/patches/edit.d2patch:4:1: lvltypes Example (#2) File 1: missing asset".into();
    assert_eq!(
        problem.jump(Path::new("/stage"), Path::new("/project")),
        Some(Jump::Table {
            table: "lvltypes".into(),
            row: 2,
            column: Some("File 1".into())
        })
    );
    problem.message =
        "lvltypes.txt row #2 (Example) column `File 1` value `missing`: missing asset".into();
    assert_eq!(
        problem.jump(Path::new("/stage"), Path::new("/project")),
        Some(Jump::Table {
            table: "lvltypes".into(),
            row: 2,
            column: Some("File 1".into())
        })
    );
    editor.jump_target(&Jump::Item { index: 2 }).unwrap();
    assert!(editor.jump_target(&Jump::Item { index: 100 }).is_err());
    editor
        .jump_target(&Jump::Key {
            table: "weapons".into(),
            key: "eb1".into(),
        })
        .unwrap();
}

#[test]
fn generated_scenario_identities_select_their_table_rows() {
    for (location, target) in [
        ("item-2-equip", Jump::Item { index: 2 }),
        ("item 2 eb1", Jump::Item { index: 2 }),
        (
            "recipe-3",
            Jump::Table {
                table: "cubemain".into(),
                row: 3,
                column: None,
            },
        ),
        (
            "class-1-skill-4-cast",
            Jump::Table {
                table: "skills".into(),
                row: 4,
                column: None,
            },
        ),
        (
            "class-1-cast-4",
            Jump::Table {
                table: "skills".into(),
                row: 4,
                column: None,
            },
        ),
        (
            "monster-5-attack",
            Jump::Table {
                table: "monstats".into(),
                row: 5,
                column: None,
            },
        ),
        (
            "area-6-walk",
            Jump::Table {
                table: "levels".into(),
                row: 6,
                column: None,
            },
        ),
        (
            "treasure class 1 Ember Drops",
            Jump::Key {
                table: "treasureclassex".into(),
                key: "Ember Drops".into(),
            },
        ),
    ] {
        let p = Problem {
            severity: Severity::Warning,
            category: "Unverified".into(),
            location: location.into(),
            message: "candidate".into(),
        };
        assert_eq!(
            p.jump(Path::new("/stage"), Path::new("/project")),
            Some(target)
        );
    }
}

#[test]
fn snapshot_file_targets_map_back_to_project_and_unknown_scenarios_stay_unlinked() {
    let root = std::env::temp_dir().join(format!("studio-jump-{}", std::process::id()));
    std::fs::create_dir_all(root.join("files")).unwrap();
    std::fs::write(root.join("files/rules.toml"), "version = 1").unwrap();
    std::fs::create_dir_all(root.join("files/patches")).unwrap();
    std::fs::write(root.join("files/patches/edit.d2patch"), "invalid").unwrap();
    let mut p = Problem {
        severity: Severity::Error,
        category: "rules".into(),
        location: "/stage/files/rules.toml:3".into(),
        message: "invalid rules".into(),
    };
    assert_eq!(
        p.jump(Path::new("/stage"), &root),
        Some(Jump::File {
            path: root.join("files/rules.toml"),
            line: Some(3)
        })
    );
    p.location = "../outside".into();
    assert_eq!(p.jump(Path::new("/stage"), &root), None);
    p.location = r"\\?\C:\stage\files\rules.toml:3".into();
    assert_eq!(
        p.jump(Path::new(r"C:\stage"), &root),
        Some(Jump::File {
            path: root.join("files/rules.toml"),
            line: Some(3)
        })
    );
    p.location = "effective tables".into();
    p.message = "error[A01] mods/ember/files/patches/edit.d2patch:4:1: syntax error".into();
    assert_eq!(
        p.jump(Path::new("/stage"), &root),
        Some(Jump::File {
            path: root.join("files/patches/edit.d2patch"),
            line: Some(4)
        })
    );
    p.message.clear();
    p.location = "items scenario synthetic".into();
    assert_eq!(p.jump(Path::new("/stage"), &root), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn finish(job: &Job) -> (Result<(), String>, Option<(Report, String)>) {
    loop {
        if let Event::Finished { result, report } =
            job.events.recv_timeout(Duration::from_secs(10)).unwrap()
        {
            return (result, report);
        }
    }
}

#[cfg(unix)]
#[test]
fn worker_reads_json_after_failure_and_handles_large_output_without_pipe_deadlock() {
    let job = Job::start(
        |dir| ToolCommand {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "head -c 100000 /dev/zero; printf '%s' \"$1\" > \"$2/report.json\"; exit 1".into(),
                "fixture".into(),
                REPORT.into(),
                dir.into(),
            ],
        },
        None,
    );
    let (result, report) = finish(&job);
    assert!(result.is_err());
    assert_eq!(report.unwrap().0.findings[0].severity, Severity::Error);
}

#[cfg(unix)]
#[test]
fn cancel_terminates_worker_process_and_does_not_claim_success() {
    let job = Job::start(
        |_| ToolCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "exec sleep 30".into()],
        },
        None,
    );
    job.cancel();
    let (result, report) = finish(&job);
    assert_eq!(result.unwrap_err(), "Cancelled");
    assert!(report.is_none());
    assert!(job.started.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
#[test]
fn worker_accepts_a_valid_successful_check_with_warnings() {
    let report = r#"{"format_version":1,"target":"synthetic","exit_code":0,"findings":[{"severity":"warning","category":"Unverified","location":"weapons row 0","message":"candidate"}],"completed":["references"]}"#;
    let job = Job::start(
        move |dir| ToolCommand {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf '%s' \"$1\" > \"$2/report.json\"".into(),
                "fixture".into(),
                report.into(),
                dir.into(),
            ],
        },
        None,
    );
    let (result, report) = finish(&job);
    assert!(result.is_ok());
    assert_eq!(report.unwrap().0.findings[0].severity, Severity::Warning);
}

#[cfg(unix)]
#[test]
fn worker_refuses_success_without_valid_report_or_claimed_package() {
    for mode in ["missing", "invalid", "missing-pack"] {
        let mode = mode.to_owned();
        let job = Job::start(
            move |dir| {
                let script = match mode.as_str() {
                    "invalid" => "printf '{}' > \"$1/report.json\"",
                    _ => "true",
                };
                ToolCommand {
                    program: "/bin/sh".into(),
                    args: vec!["-c".into(), script.into(), "fixture".into(), dir.into()],
                }
            },
            Some(PathBuf::from("/missing-studio-test-pack")),
        );
        assert!(finish(&job).0.is_err());
    }
}

#[test]
fn graph_check_problem_targets_the_node_in_the_project() {
    let root = std::env::temp_dir().join(format!("studio-graph-jump-{}", std::process::id()));
    std::fs::create_dir_all(root.join("files/scripts")).unwrap();
    std::fs::write(root.join("files/scripts/main.graph.toml"), "version=1").unwrap();
    let mut problem = Problem {
        severity: Severity::Error,
        category: "Invalid scripts".into(),
        location: "files/scripts/main.graph.toml · node roll".into(),
        message: "unknown pin".into(),
    };
    assert_eq!(
        problem.jump(Path::new("/snapshot"), &root),
        Some(Jump::Graph {
            path: root.join("files/scripts/main.graph.toml"),
            node: "roll".into()
        })
    );
    problem.location = "files/scripts/../../main.graph.toml · node roll".into();
    assert_eq!(problem.jump(Path::new("/snapshot"), &root), None);
    std::fs::remove_dir_all(root).unwrap();
}
