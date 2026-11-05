//! A walk's end in the client, single player and on a realm's link
//! (`Rig::remote`: the same server and messages, no single-player server
//! copy): once the server's unit stands, the client draws it standing
//! within a few frames, for the local player and for the town's NPCs.

use std::collections::BTreeMap;

use crate::real_rig::Rig;

use d2_client::bridge::BridgeResource;
use d2_client::world_view::walk::PreviewWalk;

/// Frames (40 ms each) the client may lag the server's stand.
const LAG: usize = 4;

/// The local player's drawn mode, as the play app picks it
/// (`world_view::walk::publish_prediction`): the prediction's walk, its
/// standing pose, else the model's mode.
fn drawn_mode(r: &Rig) -> u32 {
    let w = r.app.world();
    let world = w.resource::<BridgeResource>().0.world();
    let walk = w.resource::<PreviewWalk>();
    walk.predict
        .mode()
        .or_else(|| walk.predict.stood_mode(world))
        .unwrap_or_else(|| world.local().expect("joined").mode)
}

/// A town walk a few sub-tiles east, then a second of frames: the
/// drawn mode of each frame after the server's player stood. At the end
/// the drawn player stands on the server's cell.
fn walk_and_stop(mut r: Rig) -> Vec<u32> {
    r.step(10);
    let (x, y) = r.pos();
    let mut m = vec![0x01];
    m.extend_from_slice(&((x + 6) as u16).to_le_bytes());
    m.extend_from_slice(&(y as u16).to_le_bytes());
    r.send(&m);
    let mut walked = false;
    for _ in 0..200 {
        r.step(1);
        let mode = r.mode();
        walked |= matches!(mode, 2 | 3 | 6);
        if walked && matches!(mode, 1 | 5) {
            break;
        }
    }
    assert!(walked, "the server's player never walked");
    assert!(matches!(r.mode(), 1 | 5), "the server's player still walks");
    let drawn = (0..25)
        .map(|_| {
            r.step(1);
            drawn_mode(&r)
        })
        .collect();
    let (x, y) = r.pos();
    let at = r.app.world().resource::<PreviewWalk>().predict.position();
    assert_eq!(
        at.map(|(px, py)| (px >> 16, py >> 16)),
        Some((x as u32, y as u32)),
        "the drawn player's cell"
    );
    drawn
}

fn assert_stands(drawn: &[u32]) {
    assert!(
        drawn[LAG..].iter().all(|m| matches!(m, 1 | 5)),
        "the player's drawn mode after the server's stand: {drawn:?}"
    );
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_local_player_stands_after_a_walk_in_single_player() {
    assert_stands(&walk_and_stop(Rig::new("barbarian", &[])));
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_local_player_stands_after_a_walk_on_a_realm_link() {
    assert_stands(&walk_and_stop(Rig::remote("barbarian", &[])));
}

/// The town's NPCs walk their map-AI paths: over `frames` frames, the
/// longest run of frames (per NPC) the client drew one walking while the
/// server's stood, with the NPC's class.
fn npc_walk_lag(mut r: Rig, frames: usize) -> (usize, Vec<(u32, u32, usize)>) {
    let mut run: BTreeMap<u32, usize> = BTreeMap::new();
    let mut worst: BTreeMap<u32, (u32, usize)> = BTreeMap::new();
    for _ in 0..frames {
        r.step(1);
        let server: Vec<(u32, u32, u32)> = r.with(|sim, _| {
            sim.events
                .action
                .sys
                .units
                .iter()
                .filter(|(_, u)| u.ty == d2_sim::units::UnitType::Monster)
                .map(|(_, u)| (u.guid, u.class, u.mode))
                .collect()
        });
        let world = r.app.world().resource::<BridgeResource>().0.world();
        for (guid, class, mode) in server {
            let key = d2_client::bridge::UnitKey::new(1, guid);
            let Some(c) = world.units.get(&key) else {
                run.remove(&guid);
                continue;
            };
            // Monster modes: NU 1, WL 2, RN 15.
            let stuck = mode == 1 && matches!(c.mode, 2 | 15);
            let n = run.entry(guid).or_default();
            *n = if stuck { *n + 1 } else { 0 };
            let w = worst.entry(guid).or_insert((class, 0));
            w.1 = w.1.max(*n);
        }
    }
    let max = worst.values().map(|w| w.1).max().unwrap_or(0);
    (
        max,
        worst
            .into_iter()
            .filter(|(_, w)| w.1 > LAG)
            .map(|(g, (c, n))| (g, c, n))
            .collect(),
    )
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn town_npcs_stand_when_the_server_stops_them_in_single_player() {
    let (max, stuck) = npc_walk_lag(Rig::new("barbarian", &[]), 400);
    assert!(max <= LAG, "NPCs drawn walking while standing: {stuck:?}");
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn town_npcs_stand_when_the_server_stops_them_on_a_realm_link() {
    let (max, stuck) = npc_walk_lag(Rig::remote("barbarian", &[]), 400);
    assert!(max <= LAG, "NPCs drawn walking while standing: {stuck:?}");
}
