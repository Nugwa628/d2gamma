//! `mp-load`: the realm lobby's load and soak driver (lobby plan L17; the
//! driver is [`mp_test::load`]).
//!
//! ```text
//! mp-load [--bots 50] [--secs 300] [--ramp 20] [--flap 10] [--games on|off]
//!         [--sample-ms 1000] [--window 300] [--out target/mp-load]
//!         [--game-dir DIR] [--setup split:1] [--set KEY=VALUE]..
//!         [--only chat,whisper,friends,guild,ladder,market,users,games,flap]
//! ```
//!
//! Starts a main server (`d2rust-realm`) and its game server
//! (`d2rust-node`), both next to this binary, with the game data of
//! `--game-dir` or `D2_GAME_DIR`; logs every bot in, lets them act for
//! `--secs`, then writes `load.md`, `load.json` and `samples.csv` to
//! `--out` and prints the markdown. `--set` adds a main server config key
//! (`net_threads=4`); `--only` keeps those groups of acts (to find which
//! one a cost comes from). Exit code: 0 when the run held (no line lost, every
//! stored DM delivered, no session left, every bot got in), 1 when not, 2
//! when it couldn't run.

use std::path::PathBuf;
use std::process::ExitCode;

use mp_test::load::{self, Opts};
use mp_test::server::Target;
use mp_test::topology::{NodeProgram, Nodes, Setup};

fn sibling(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let mut p = exe.with_file_name(name);
    if cfg!(windows) {
        p.set_extension("exe");
    }
    p
}

fn parse() -> Result<Opts, String> {
    let mut bots = 50;
    let mut secs = 300;
    let mut ramp_s = 20;
    let mut flap_pct = 10;
    let mut games = true;
    let mut sample_ms = 1000;
    let mut window_s = 300;
    let mut out = PathBuf::from("target/mp-load");
    let mut seed = 0x5eed_u64;
    let mut setup = Setup::parse("split:1")?;
    let mut game_dir = std::env::var_os("D2_GAME_DIR").map(PathBuf::from);
    let mut settings = Vec::new();
    let mut only = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} needs a value"));
        let number = |v: String| {
            v.parse::<u64>()
                .map_err(|_| format!("{a}: {v} is not a number"))
        };
        match a.as_str() {
            "--bots" => bots = number(value()?)? as usize,
            "--secs" => secs = number(value()?)?,
            "--ramp" => ramp_s = number(value()?)?,
            "--flap" => flap_pct = number(value()?)? as usize,
            "--games" => games = value()? != "off",
            "--sample-ms" => sample_ms = number(value()?)?,
            "--window" => window_s = number(value()?)?,
            "--out" => out = PathBuf::from(value()?),
            "--seed" => seed = number(value()?)?,
            "--setup" => setup = Setup::parse(&value()?)?,
            "--game-dir" => game_dir = Some(PathBuf::from(value()?)),
            "--set" => {
                let v = value()?;
                let (k, v) = v
                    .split_once('=')
                    .ok_or(format!("--set {v}: not KEY=VALUE"))?;
                settings.push((k.to_owned(), v.to_owned()));
            }
            "--only" => {
                for g in value()?.split(',') {
                    if !load::ACT_GROUPS.contains(&g) {
                        return Err(format!(
                            "--only: no group {g} ({})",
                            load::ACT_GROUPS.join(",")
                        ));
                    }
                    only.push(g.to_owned());
                }
            }
            "-h" | "--help" => {
                return Err(
                    "usage: mp-load [--bots N] [--secs N] [--ramp S] [--flap PCT] \
                            [--games on|off] [--sample-ms MS] [--window S] [--out DIR] \
                            [--game-dir DIR] [--setup split:1] [--set KEY=VALUE].. [--only GROUPS]"
                        .into(),
                )
            }
            _ => return Err(format!("unknown argument {a} (--help)")),
        }
    }
    if bots == 0 || bots > 1000 {
        return Err("--bots takes 1 to 1000".into());
    }
    let game_dir = game_dir.ok_or("the real servers need --game-dir or D2_GAME_DIR")?;
    let target = Target::Topology {
        realm: Box::new(Target::D2rustRealm {
            bin: sibling("d2rust-realm"),
            game_dir: game_dir.clone(),
        }),
        nodes: Nodes {
            program: NodeProgram::D2rustNode {
                bin: sibling("d2rust-node"),
                game_dir,
            },
            setup,
        },
    };
    Ok(Opts {
        target,
        bots,
        secs,
        ramp_s,
        flap_pct,
        games,
        sample_ms,
        window_s,
        out,
        seed,
        settings,
        only,
    })
}

fn main() -> ExitCode {
    let opts = match parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("mp-load: {e}");
            return ExitCode::from(2);
        }
    };
    match load::run(opts) {
        Ok(r) => {
            println!("{}", r.markdown);
            if r.held {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("mp-load: {e}");
            ExitCode::from(2)
        }
    }
}
