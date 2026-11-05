//! The separately distributed Mod Studio executable.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use mod_studio::ui::Studio;
fn main() -> eframe::Result {
    let command: Vec<_> = std::env::args_os().skip(1).collect();
    if command.first().is_some_and(|s| s == "seed-preview") {
        if let Err(error) = mod_studio::maps::seed_preview(&command[1..]) {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let mut studio = Studio::default();
    let mut input_script = None;
    let mut captures = std::path::PathBuf::from("studio-captures");
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--render" {
            studio.render = args.next().map(Into::into);
        } else if arg == "--input-script" {
            input_script = args.next().map(std::path::PathBuf::from);
        } else if arg == "--captures" {
            captures = args.next().map(Into::into).unwrap_or(captures);
        } else {
            studio.open(std::path::Path::new(&arg));
        }
    }
    let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let app: Box<dyn eframe::App> = if let Some(path) = input_script {
        let runner = match mod_studio::input_script::Runner::load(&path, &captures) {
            Ok(runner) => runner,
            Err(error) => {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
        };
        Box::new(mod_studio::input_script::ScriptApp {
            studio,
            runner,
            failed: failed.clone(),
        })
    } else {
        Box::new(studio)
    };
    let result = eframe::run_native(
        "Mod Studio",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1100.0, 800.0]),
            ..Default::default()
        },
        Box::new(|_| Ok(app)),
    );
    if failed.load(std::sync::atomic::Ordering::Relaxed) {
        std::process::exit(1);
    }
    result
}
