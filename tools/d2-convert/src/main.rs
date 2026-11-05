//! `d2-convert convert --install <D2 dir> --out <dir>`,
//! `d2-convert verify --out <dir> [--deep --install <D2 dir>]`,
//! `d2-convert pack --layer <mod folder> --out <file.d2pack>` and
//! `d2-convert verify-pack --pack <file.d2pack> [--against <mod folder>]`.
//!
//! Exit codes: 0 pass, 1 failures, 2 not a D2 LoD install or bad usage.

use std::path::PathBuf;
use std::process::ExitCode;

use d2_convert::{convert, kinds, verify_full, ConvertError, Options, VerifyOptions};

const USAGE: &str = "usage:
  d2-convert convert --install <D2 dir> --out <dir> [--lang ENG] [--force] [--threads N] [--quiet]
  d2-convert verify  --out <dir> [--deep --install <D2 dir>] [--quiet]
  d2-convert pack    --layer <mod folder> --out <file.d2pack> [--threads N] [--quiet]
  d2-convert verify-pack --pack <file.d2pack> [--against <mod folder>] [--quiet]
  d2-convert package-mod <folder> --base <converted install> [--out <file.d2pack>] [--game <D2Gamma-Game>] [--report DIR]
(--game is a synonym of --install, --native of --out)";

struct Args {
    install: Option<PathBuf>,
    out: Option<PathBuf>,
    layer: Option<PathBuf>,
    pack: Option<PathBuf>,
    against: Option<PathBuf>,
    lang: String,
    force: bool,
    deep: bool,
    quiet: bool,
    threads: usize,
    stop_after: Option<usize>,
}

fn parse(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut a = Args {
        install: None,
        out: None,
        layer: None,
        pack: None,
        against: None,
        lang: "ENG".into(),
        force: false,
        deep: false,
        quiet: false,
        threads: 0,
        stop_after: None,
    };
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--install" | "--game" => a.install = Some(value(&arg)?.into()),
            "--out" | "--native" => a.out = Some(value(&arg)?.into()),
            "--lang" => a.lang = value(&arg)?,
            "--layer" => a.layer = Some(value(&arg)?.into()),
            "--pack" => a.pack = Some(value(&arg)?.into()),
            "--against" => a.against = Some(value(&arg)?.into()),
            "--threads" => {
                a.threads = value(&arg)?
                    .parse()
                    .map_err(|_| "--threads: not a number")?
            }
            "--stop-after" => {
                a.stop_after = Some(
                    value(&arg)?
                        .parse()
                        .map_err(|_| "--stop-after: not a number")?,
                )
            }
            "--force" => a.force = true,
            "--deep" => a.deep = true,
            "--quiet" => a.quiet = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(a)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if cmd == "package-mod" {
        return package_mod(args);
    }
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match cmd.as_str() {
        "pack" => return pack(&a),
        "verify-pack" => return verify_pack(&a),
        _ => {}
    }
    let Some(out) = a.out.clone() else {
        eprintln!("--out is required\n{USAGE}");
        return ExitCode::from(2);
    };
    let kinds = kinds::builtin();
    match cmd.as_str() {
        "convert" => {
            let Some(install) = a.install else {
                eprintln!("--install is required\n{USAGE}");
                return ExitCode::from(2);
            };
            let mut opts = Options::new(install, out);
            opts.language = a.lang;
            opts.force = a.force;
            opts.threads = a.threads;
            opts.progress = !a.quiet;
            opts.stop_after = a.stop_after;
            match convert(&opts, &kinds) {
                Ok(s) => {
                    for (path, check, detail) in &s.failures {
                        eprintln!("FAILED {path}: {check}: {detail}");
                    }
                    ExitCode::from(s.exit_code() as u8)
                }
                Err(ConvertError::NotAnInstall(m)) => {
                    eprintln!("error: {m}");
                    ExitCode::from(2)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        "verify" => {
            if a.deep && a.install.is_none() {
                eprintln!("--deep needs --install\n{USAGE}");
                return ExitCode::from(2);
            }
            let opts = VerifyOptions {
                out,
                deep_install: if a.deep { a.install } else { None },
                progress: !a.quiet,
            };
            let outcome = verify_full(&opts, &kinds);
            let problems = outcome.problems;
            for p in &problems {
                eprintln!("FAILED {p}");
            }
            if problems.is_empty() && !a.quiet {
                eprintln!("verify: all files pass");
                if let Some(t) = &outcome.tables {
                    eprintln!("verify: C-TABLE {}", t.line());
                }
            }
            ExitCode::from(u8::from(!problems.is_empty()))
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// The game owns the complete checker. Forward publication to its headless
/// entry point rather than linking the renderer into the converter.
fn package_mod(args: impl Iterator<Item = String>) -> ExitCode {
    let mut forwarded = Vec::new();
    let mut game = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        if arg == "--game" {
            let Some(value) = args.next() else {
                eprintln!("--game needs the D2Gamma-Game executable path");
                return ExitCode::from(2);
            };
            game = Some(PathBuf::from(value));
        } else {
            forwarded.push(arg);
        }
    }
    let game = game.or_else(|| {
        std::env::current_exe().ok().map(|p| {
            p.with_file_name(if cfg!(windows) {
                "D2Gamma-Game.exe"
            } else {
                "D2Gamma-Game"
            })
        })
    });
    let Some(game) = game else {
        return ExitCode::from(2);
    };
    match std::process::Command::new(game)
        .arg("package-mod")
        .args(forwarded)
        .status()
    {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8),
        Err(e) => {
            eprintln!("package-mod: could not start the full checker: {e}; use --game PATH");
            ExitCode::from(1)
        }
    }
}

/// `pack`: builds a data pack from a mod's layer folder (`mod.toml` and
/// `files/`). Exit 0 written, 1 failed, 2 bad usage.
fn pack(a: &Args) -> ExitCode {
    let (Some(layer), Some(out)) = (&a.layer, &a.out) else {
        eprintln!(
            "pack needs --layer and --out
{USAGE}"
        );
        return ExitCode::from(2);
    };
    if !layer.join("mod.toml").is_file() {
        eprintln!(
            "error: {}: not a mod layer folder (no mod.toml)",
            layer.display()
        );
        return ExitCode::from(2);
    }
    let start = std::time::Instant::now();
    match d2_native::pack::build(layer, out, a.threads) {
        Ok(s) => {
            if !a.quiet {
                eprintln!(
                    "pack: {} files ({} compressed), {} bytes -> {} bytes in {:.1}s: {}",
                    s.files,
                    s.compressed,
                    s.raw_bytes,
                    s.pack_bytes,
                    start.elapsed().as_secs_f64(),
                    out.display()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

/// `verify-pack`: reads every file of a data pack and, with `--against`,
/// compares it with the layer folder. Exit 0 sound, 1 problems, 2 bad usage.
fn verify_pack(a: &Args) -> ExitCode {
    let Some(pack) = &a.pack else {
        eprintln!(
            "verify-pack needs --pack
{USAGE}"
        );
        return ExitCode::from(2);
    };
    let start = std::time::Instant::now();
    match d2_native::pack::verify(pack, a.against.as_deref()) {
        Ok(problems) => {
            for p in &problems {
                eprintln!("FAILED {p}");
            }
            if problems.is_empty() && !a.quiet {
                eprintln!(
                    "verify-pack: every file passes ({:.1}s)",
                    start.elapsed().as_secs_f64()
                );
            }
            ExitCode::from(u8::from(!problems.is_empty()))
        }
        Err(e) => {
            eprintln!("FAILED {e}");
            ExitCode::from(1)
        }
    }
}
