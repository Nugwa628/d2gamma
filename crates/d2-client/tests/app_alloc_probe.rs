//! The allocator probe: a long session of games in one app, with the
//! process memory and the frame time measured per game, under the
//! allocator the test binary was built with (the `mimalloc` feature, or
//! the system's with `--no-default-features`). It asserts nothing: it prints one
//! `ALLOCPROBE` line per game and a total, for the numbers in the notes of
//! the allocator choice. Run it with `D2_PROBE_GAMES=10 --nocapture`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use d2_client::app::play::{install_live_client, register_live_client, LiveClient};
use d2_client::app::session::{self, GameEnder, GameStarter, Screen};
use d2_client::app::single_player::{self, GameData};
use d2_client::bridge::BridgeResource;
use d2_server::seams::Clock;

use crate::app_support;

#[cfg(all(feature = "mimalloc", not(feature = "dylib")))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

struct StepClock(Arc<AtomicU32>);

impl Clock for StepClock {
    fn now_ms(&mut self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

/// `VmRSS` / `VmHWM` of this process in KiB: `/proc/self/status` on Linux,
/// the working set and its peak from PowerShell on Windows (there is no
/// safe std call for it).
fn status_kib(field: &str) -> u64 {
    #[cfg(windows)]
    {
        let prop = if field == "VmHWM" {
            "PeakWorkingSet64"
        } else {
            "WorkingSet64"
        };
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {}).{prop}", std::process::id()),
            ])
            .output()
            .ok();
        return out
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|t| t.trim().parse::<u64>().ok())
            .map_or(0, |b| b / 1024);
    }
    #[cfg(not(windows))]
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| l.strip_prefix(field)).and_then(|v| {
                v.trim_start_matches(':')
                    .trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse()
                    .ok()
            })
        })
        .unwrap_or(0)
}

fn in_game(app: &App) -> bool {
    app.world()
        .get_resource::<BridgeResource>()
        .is_some_and(|b| b.0.world().in_game && b.0.world().local().is_some())
}

fn pct(sorted: &[u64], p: f64) -> u64 {
    sorted
        .get(((sorted.len() as f64 - 1.0) * p).round() as usize)
        .copied()
        .unwrap_or(0)
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR; measurement only"]
fn many_games_alloc_probe() {
    let games: u32 = std::env::var("D2_PROBE_GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let data = app_support::game_data();
    let GameData::Live(live) = data.clone();
    let ms = Arc::new(AtomicU32::new(1000));
    let mut app = App::new();
    app.insert_resource(d2_client::bridge::mirror::ScriptedClock(ms.clone()));
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
        .init_asset::<Image>()
        .init_resource::<ButtonInput<MouseButton>>();
    register_live_client(&mut app, false);
    session::add_session(&mut app);
    let game = {
        let ms = ms.clone();
        GameStarter(Box::new(move |world: &mut World| {
            let character = single_player::Character::New;
            let seed = single_player::game_seed(&character, None);
            let (link, started) = single_player::start_with(
                data.clone(),
                seed,
                character.clone(),
                StepClock(ms.clone()),
            )?;
            let speeds = single_player::walk_speeds(&data, &character)?;
            install_live_client(
                world,
                Box::new(link),
                LiveClient {
                    data: &live,
                    request: &character,
                    start_flags: None,
                    prices: started.prices,
                    speeds,
                    hardcore: false,
                    automap_files: None,
                    gpu: false,
                },
            )?;
            Ok(())
        }))
    };
    app.insert_non_send(game)
        .insert_non_send(GameEnder(Box::new(|_| Ok(false))));
    let mut times: Vec<u64> = Vec::new();
    let step = |app: &mut App, times: &mut Vec<u64>| {
        let t = Instant::now();
        app.update();
        times.push(t.elapsed().as_micros() as u64);
        ms.fetch_add(40, Ordering::SeqCst);
    };
    for _ in 0..5 {
        step(&mut app, &mut times);
    }
    let start = Instant::now();
    let mut per_game = Vec::new();
    for round in 1..=games {
        let from = times.len();
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Game);
        let mut frames = 0;
        while !in_game(&app) {
            step(&mut app, &mut times);
            frames += 1;
            assert!(frames < 2000, "game {round}: the join finishes");
        }
        // The game's own frames: walk-free idle ticks, as the session test.
        for _ in 0..200 {
            step(&mut app, &mut times);
        }
        app.world_mut()
            .resource_mut::<BridgeResource>()
            .0
            .save_and_exit()
            .unwrap();
        let mut frames = 0;
        while *app.world().resource::<State<Screen>>().get() != Screen::Front {
            step(&mut app, &mut times);
            frames += 1;
            assert!(frames < 500, "game {round}: the leave finishes");
        }
        step(&mut app, &mut times);
        let mut t: Vec<u64> = times[from..].to_vec();
        t.sort_unstable();
        let mean = t.iter().sum::<u64>() / t.len().max(1) as u64;
        let rss = status_kib("VmRSS");
        per_game.push(rss);
        println!(
            "ALLOCPROBE game={round} rss_kib={rss} hwm_kib={} upd_n={} upd_mean_us={mean} upd_p50_us={} upd_p99_us={} upd_max_us={}",
            status_kib("VmHWM"),
            t.len(),
            pct(&t, 0.5),
            pct(&t, 0.99),
            t.last().copied().unwrap_or(0),
        );
    }
    let mut all = times.clone();
    all.sort_unstable();
    println!(
        "ALLOCPROBE total games={games} wall_ms={} rss_first_kib={} rss_last_kib={} growth_kib={} hwm_kib={} upd_mean_us={} upd_p99_us={}",
        start.elapsed().as_millis(),
        per_game.first().copied().unwrap_or(0),
        per_game.last().copied().unwrap_or(0),
        per_game
            .last()
            .copied()
            .unwrap_or(0)
            .saturating_sub(per_game.first().copied().unwrap_or(0)),
        status_kib("VmHWM"),
        all.iter().sum::<u64>() / all.len().max(1) as u64,
        pct(&all, 0.99),
    );
}
