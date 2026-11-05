//! Installed-data dogfood for the showcase. Run explicitly on the CI game install.
use anyhow::{Context, Result};
use d2_client::app::{
    ai_arena,
    autoplay_host::{Host, HostArgs},
    control::protocol,
    poke,
};
use d2_client::bridge::BridgeResource;
use d2_sim::rules::data_mod::ai::trace::EventKind;
use std::path::PathBuf;

fn setup() -> Result<(PathBuf, PathBuf)> {
    let native = PathBuf::from(
        std::env::var("D2_SHOWCASE_NATIVE")
            .context("set D2_SHOWCASE_NATIVE to a converted install")?,
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let target = std::env::var_os("D2_SHOWCASE_PACKED")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("templates/showcase"));
    let stack = d2_client::kit_mod::activate(&d2_modkit::ModSpec::Dir(target))?;
    assert!(
        stack.mods.iter().any(|m| m.manifest.id == "showcase-notes"),
        "enable the showcase-notes add-on in the isolated dogfood home"
    );
    let out = root.join(".lane/dogfood-results");
    std::fs::create_dir_all(&out)?;
    Ok((native, out))
}
fn command(host: &mut Host, line: &str) -> Result<()> {
    host.run(protocol::parse_command(line).map_err(anyhow::Error::msg)?)
        .with_context(|| format!("showcase command {line}"))?;
    host.step(8);
    Ok(())
}
fn values(
    host: &mut Host,
) -> (
    std::collections::BTreeMap<String, i32>,
    std::collections::BTreeMap<String, i32>,
) {
    let world = host
        .app_mut()
        .world()
        .resource::<BridgeResource>()
        .0
        .world();
    (world.synced_values.clone(), world.ui_variables.clone())
}
fn area(host: &mut Host) -> Result<u64> {
    let s = host.state()?;
    s["snap"]["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["ut"] == 0 && u["g"] == s["local"])
        .unwrap()["lv"]
        .as_u64()
        .context("player level")
}
fn interact_exit(host: &mut Host, level: usize, slot: usize) -> Result<()> {
    let class = host.server().lock().unwrap().with(move |l| {
        l.host_mut().game.events.action.hooks().drlg.data.levels[level].warp[slot]
    })?;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/dogfood-results");
    std::fs::write(
        out.join(format!("before-exit-{level}-{slot}-map.json")),
        serde_json::to_vec_pretty(&host.run(d2_client::app::autoplay_host::Command::Map)?)?,
    )?;
    std::fs::write(
        out.join(format!("before-exit-{level}-{slot}-state.json")),
        serde_json::to_vec_pretty(&host.state()?)?,
    )?;
    command(host, &format!("interactunit 5 {class}"))?;
    // A tile's pending interaction waits for the walk to stop, then its
    // request reaches the server on a later tick.
    for _ in 0..100 {
        if area(host)? != level as u64 {
            return Ok(());
        }
        host.step(1);
    }
    std::fs::write(
        out.join(format!("stalled-exit-{level}-{slot}.json")),
        serde_json::to_vec_pretty(&(host.state()?, host.sent_counts()))?,
    )?;
    anyhow::bail!("exit {level}/{slot} did not finish")
}
#[test]
#[ignore = "requires D2_SHOWCASE_NATIVE and the installed CI game data"]
fn showcase_walk_quest_and_synced_hud() -> Result<()> {
    let (native, out) = setup()?;
    let mut host = Host::start(&HostArgs {
        native: Some(native.clone()),
        new: Some(("amazon".into(), "Lantern".into())),
        seed: Some(42),
        ..Default::default()
    })?;
    host.step(30);
    assert_eq!(area(&mut host)?, 137);
    command(&mut host, "interactunit 2 119")?;
    host.step(16);
    let waypoint_state = host.state()?;
    let rows = waypoint_state["client"]["waypoint_rows"]
        .as_array()
        .context("waypoint rows")?;
    assert_eq!(
        rows.iter()
            .map(|r| r["level"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [137, 138, 139]
    );
    std::fs::write(
        out.join("waypoint-state.json"),
        serde_json::to_vec_pretty(&waypoint_state)?,
    )?;
    assert_eq!(
        rows[0]["known"], false,
        "the current row is not a travel destination"
    );
    assert_eq!(rows[0]["current"], true);
    command(&mut host, "key Esc")?;
    command(&mut host, "click ui:mod.button(lantern_hud,charge)")?;
    assert_eq!(values(&mut host).0.get("resonance"), Some(&1));
    // Two requests inside the declared cooldown cannot change the server twice.
    command(&mut host, "click ui:mod.button(lantern_hud,charge)")?;
    assert_eq!(values(&mut host).0.get("resonance"), Some(&1));
    interact_exit(&mut host, 137, 0)?;
    assert_eq!(area(&mut host)?, 138);
    assert_eq!(values(&mut host).1.get("quests/q1"), Some(&2));
    interact_exit(&mut host, 138, 1)?;
    assert_eq!(area(&mut host)?, 139);
    command(&mut host, "interactunit 1 734")?;
    let select = poke::parse_poke_arg("1 msg 0x3C 357 0 4294967295")
        .map_err(anyhow::Error::msg)?
        .op;
    host.server()
        .lock()
        .unwrap()
        .with(move |l| poke::apply_on_host(l.host_mut(), &select))?;
    host.step(8);
    for _ in 0..30 {
        if values(&mut host).1.get("quests/q2") == Some(&2) {
            break;
        }
        command(&mut host, "rclickunit 1 734")?;
        host.step(24);
    }
    let (synced, vars) = values(&mut host);
    assert_eq!(vars.get("quests/q2"), Some(&2), "{vars:?}");
    assert_eq!(vars.get("quests/q2o0"), Some(&2));
    assert_eq!(vars.get("quests/q2o1"), Some(&1));
    assert!(synced["resonance"] > 1, "visual graph ran");
    assert!(
        vars.keys().any(|k| k.ends_with("/casts")),
        "Luau ran: {vars:?}"
    );
    host.step(300);
    assert_eq!(
        values(&mut host).1.get("ward/ward_active"),
        Some(&0),
        "state-end trigger called Luau"
    );
    command(&mut host, "key Q")?;
    std::fs::write(
        out.join("walk-quest-state.json"),
        serde_json::to_vec_pretty(&host.state()?)?,
    )?;
    std::fs::write(
        out.join("walk-quest-values.json"),
        serde_json::to_vec_pretty(&(synced.clone(), vars))?,
    )?;
    let before = host
        .server()
        .lock()
        .unwrap()
        .with(|l| d2_client::app::save::read_live(&mut l.host_mut().game))??;
    let data = d2_server::game::GameData::open_with(
        &d2_server::game::GameSource::Native(
            native.clone(),
            d2_server::game::Layers::List(d2_client::kit_mod::layers()),
        ),
        d2_data::bin::DataRules::Mod,
    )?;
    let d2_server::game::GameData::Live(live) = data;
    let mut item_tables = live.save.items.clone();
    item_tables.rules = d2_client::kit_mod::ruleset();
    let codes: Vec<_> = before
        .extra
        .items
        .as_ref()
        .context("inventory")?
        .iter()
        .map(|item| {
            d2_sim::items::bitstream::read::read_save_entry(&item.bytes, &item_tables)
                .unwrap()
                .item
                .item
                .code
        })
        .collect();
    assert_eq!(
        codes.iter().filter(|&&code| code == *b"seal").count(),
        1,
        "reward: {codes:?}"
    );
    let save = d2_server::game::character_save::apply_live(
        &d2_server::game::character_save::fresh(0, b"Lantern", 0),
        &before,
        0,
    );
    use d2_server::game::CharacterCodec;
    let codec = d2_server::game::ModCodec(d2_client::kit_mod::ruleset());
    let path = out.join("Lantern.d2m");
    std::fs::write(
        &path,
        codec.write(&save, &live.save).map_err(anyhow::Error::msg)?,
    )?;
    drop(host);
    let mut reloaded = Host::start(&HostArgs {
        native: Some(native.clone()),
        save: Some(path),
        seed: Some(42),
        ..Default::default()
    })?;
    reloaded.step(30);
    let (again, quests) = values(&mut reloaded);
    assert_eq!(again.get("resonance"), synced.get("resonance"));
    assert_eq!(quests.get("quests/q2"), Some(&2));
    drop(reloaded);
    let mut eclipse = Host::start(&HostArgs {
        native: Some(native),
        new: Some(("amazon".into(), "Eclipse".into())),
        difficulty: 3,
        seed: Some(42),
        ..Default::default()
    })?;
    eclipse.step(30);
    assert_eq!(area(&mut eclipse)?, 137);
    assert_ne!(values(&mut eclipse).1.get("quests/q2"), Some(&2));
    Ok(())
}
#[test]
#[ignore = "requires D2_SHOWCASE_NATIVE and the installed CI game data"]
fn showcase_arena_kills_the_two_phase_boss() -> Result<()> {
    let (native, out) = setup()?;
    let args = ai_arena::parse_args(
        &[
            "--class",
            "734",
            "--ai",
            "148",
            "--level",
            "139",
            "--distance",
            "4",
            "--ticks",
            "500",
            "--seed",
            "7",
        ]
        .map(str::to_owned),
    )?;
    let data = d2_server::game::GameData::open_with(
        &d2_server::game::GameSource::Native(
            native,
            d2_server::game::Layers::List(d2_client::kit_mod::layers()),
        ),
        d2_data::bin::DataRules::Mod,
    )?;
    let stack = d2_client::kit_mod::active().unwrap();
    let graphs = d2_client::kit_mod::host(stack)?.ai_rules().clone();
    let select = poke::parse_poke_arg("1 msg 0x3C 357 0 4294967295")
        .map_err(anyhow::Error::msg)?
        .op;
    let cast = poke::parse_poke_arg("1 msg 0x0C @x @y")
        .map_err(anyhow::Error::msg)?
        .op;
    let observer = Box::new(
        move |h: &mut poke::ServerHost<d2_client::app::state_dump::StepClock>| {
            let frame = h.game.game.frame;
            if frame == 20 {
                let r = poke::apply_on_host(h, &select);
                assert_eq!(r.code(), "ok");
            }
            if frame >= 75 && frame % 30 == 15 {
                let r = poke::apply_on_host(h, &cast);
                assert_eq!(r.code(), "ok");
            }
            if let Some(ai) = h.game.events.action.hooks().ai.as_mut() {
                ai.events.take();
            }
        },
    );
    let trace = ai_arena::run_with(
        &args,
        data,
        d2_client::kit_mod::ruleset(),
        &graphs,
        Some(std::path::Path::new("/work/game")),
        Some(observer),
    )?;
    std::fs::write(out.join("boss-arena.trace"), trace.write())?;
    assert!(trace
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::WindUp { .. })));
    let phase = trace
        .events
        .iter()
        .find(|e| matches!(e.kind, EventKind::Phase { .. }))
        .context("boss phase")?;
    assert!(trace.samples.iter().any(|s| s.state == Some(1)));
    let immune: Vec<_> = trace
        .samples
        .iter()
        .filter(|s| s.tick > phase.tick && s.tick < phase.tick + 100)
        .collect();
    assert!(immune.len() >= 90);
    assert!(
        immune.windows(2).all(|s| s[0].life == s[1].life),
        "immune window blocks repeated casts"
    );
    assert!(trace.summary().died_at.is_some(), "{:?}", trace.summary());
    Ok(())
}

#[test]
#[ignore = "requires D2_SHOWCASE_NATIVE and the installed CI game data"]
fn showcase_packages_and_rechecks_signed_mod_and_addon() -> Result<()> {
    let (native, out) = setup()?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    for name in ["showcase", "showcase-notes"] {
        let packed = out.join(format!("{name}.d2pack"));
        d2_client::app::package_mod::package_checked(
            &root.join("templates").join(name),
            &native,
            &packed,
            &out.join("private-author-key.pk8"),
            d2_client::user_dir::mods_dir(),
            out.join(format!("package-{name}")),
        )?;
        let report = d2_client::app::check_mod::check(&d2_client::app::check_mod::Options {
            target: Some(packed.clone()),
            native: native.clone(),
            mods_dir: d2_client::user_dir::mods_dir(),
            out: out.join(format!("packed-{name}")),
            jobs: 4,
            full: false,
        })?;
        d2_client::app::check_mod::write_report(&report, &out.join(format!("packed-{name}")))?;
        assert_eq!(report.exit_code, 0, "{}", report.text());
        d2_modkit::publish::verify(&d2_native::pack::Pack::open(&packed)?)?;
    }
    Ok(())
}

#[test]
#[ignore = "requires installed CI game data"]
fn showcase_renders_every_headless_frame() -> Result<()> {
    let (native, out) = setup()?;
    let mut host = Host::start(&HostArgs {
        native: Some(native),
        new: Some(("amazon".into(), "LanternFrame".into())),
        seed: Some(42),
        ..Default::default()
    })?;
    host.step(35);
    let state = host.state()?;
    std::fs::write(
        out.join("frame-diagnostic.json"),
        serde_json::to_vec_pretty(&state)?,
    )?;
    let view = &state["client"]["view"];
    assert_eq!(view["skipped"], 0, "{view}");
    assert!(view["tick"].as_u64().unwrap_or(0) >= 35, "{view}");
    assert!(view["tiles"].as_u64().unwrap_or(0) > 0, "{view}");
    assert!(
        !state["client"]["anim"].as_array().unwrap().is_empty(),
        "world frames must record drawn animation segments"
    );
    Ok(())
}

#[test]
#[ignore = "requires installed CI game data"]
fn showcase_class_animation_rates_are_measured() -> Result<()> {
    use d2_client::app::{
        single_player::{GameData, GameSource, Layers},
        sweep,
    };
    let (native, out) = setup()?;
    let data = GameData::open_with(
        &GameSource::Native(
            native.clone(),
            Layers::List(d2_client::kit_mod::layers_for(&native)?),
        ),
        d2_client::kit_mod::ruleset().data_rules(),
    )?;
    let report = sweep::run(
        data,
        &sweep::Options {
            family: "anims".into(),
            filter: Some("class-".into()),
            jobs: 4,
            out: out.join("animation-rates"),
            native: Some(native),
            ..Default::default()
        },
    )?;
    assert_eq!(report["totals"]["failed"], 0, "{report}");
    assert_eq!(report["totals"]["scenarios"], 48, "{report}");
    Ok(())
}
