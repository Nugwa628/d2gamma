//! Map preflight streams the real room/tile pipeline, then searches its subtile collision grid.
use super::{Report, Severity};
use d2_native::source::NativeSource;
use d2_server::game::{LevelSource, LiveData};
use d2_sim::drlg::maps::generation::Generation;
use d2_sim::drlg::{Drlg, Services, TownLevels};
use d2_sim::rules::acts::Acts;
use std::collections::{BTreeSet, VecDeque};

pub fn check(live: &LiveData, source: &NativeSource, report: &mut Report) -> anyhow::Result<()> {
    if live.maps.level.is_empty() {
        return Ok(());
    }
    let mut acts = None;
    for layer in source.layers.layers.iter().skip(1).rev() {
        if let Some(bytes) = layer.read("acts.toml") {
            let bytes = bytes?;
            acts = Some(Acts::from_toml(std::str::from_utf8(&bytes)?)?);
            break;
        }
    }
    let registry = acts.clone().unwrap_or_default();
    let owner = |id| registry.find_act(id);
    for l in &live.maps.level {
        if owner(l.id).is_none() {
            report.finding(
                Severity::Error,
                "map connectivity",
                &format!("files/levels.toml: level {}", l.id),
                "level belongs to no configured act",
            );
        }
        for w in &l.warp {
            let at = format!("files/levels.toml: level {}, warp slot {}", l.id, w.slot);
            if owner(w.to) != owner(l.id) || owner(w.to).is_none() {
                report.finding(Severity::Error,"map connectivity",&at,"warp destination must belong to the same act (use act travel for inter-act links)");
                continue;
            }
            let target = live.levels.drlg.levels.get(w.to as usize);
            if !target.is_some_and(|r| r.vis.contains(&l.id)) {
                report.finding(
                    Severity::Error,
                    "map connectivity",
                    &at,
                    format!("destination {} has no return link", w.to),
                );
            }
        }
        let Some(act) = owner(l.id) else {
            continue;
        };
        let a = &registry.rows()[usize::from(act)];
        let mut seen = BTreeSet::new();
        let mut todo = VecDeque::from([a.town]);
        todo.extend(a.waypoints.iter().map(|w| w.level));
        todo.extend(
            registry
                .rows()
                .iter()
                .flat_map(|a| &a.travel)
                .map(|t| t.to)
                .filter(|&v| owner(v) == Some(act)),
        );
        while let Some(id) = todo.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(row) = live.levels.drlg.levels.get(id as usize) {
                todo.extend(
                    row.vis
                        .iter()
                        .copied()
                        .filter(|&v| v != 0 && owner(v) == Some(act)),
                );
            }
        }
        if !seen.contains(&l.id) {
            report.finding(
                Severity::Error,
                "map connectivity",
                &format!("files/levels.toml: level {}", l.id),
                "not reachable from the act town, declared waypoints or act travel",
            );
        }
    }
    for (i, a) in live.maps.level.iter().enumerate() {
        for b in live.maps.level.iter().skip(i + 1) {
            if owner(a.id) == owner(b.id)
                && a.offset[0] < b.offset[0] + i32::from(b.size[0])
                && b.offset[0] < a.offset[0] + i32::from(a.size[0])
                && a.offset[1] < b.offset[1] + i32::from(b.size[1])
                && b.offset[1] < a.offset[1] + i32::from(a.size[1])
            {
                report.finding(
                    Severity::Error,
                    "map connectivity",
                    &format!("files/levels.toml: level {}", a.id),
                    format!("world rectangle overlaps level {} in the same act", b.id),
                );
            }
        }
    }
    if report.exit_code != 0 {
        return Ok(());
    }
    let random = live
        .maps
        .level
        .iter()
        .any(|l| l.files.len() > 1 || !matches!(l.generation, Generation::Preset));
    let seeds = if random { 32 } else { 1 };
    let towns = acts.as_ref().map_or_else(TownLevels::default, Acts::towns);
    let groups: BTreeSet<u8> = live.maps.level.iter().filter_map(|l| owner(l.id)).collect();
    for seed in 0..seeds {
        let src = LevelSource::live_with_acts(live, seed, acts.as_ref());
        for &act in &groups {
            let mut types = src.shared.clone();
            let mut drlg = match Drlg::create_with_towns(
                act, seed, 0, 0, false, &towns, &src.data, &mut types,
            ) {
                Ok(d) => d,
                Err(e) => {
                    report.finding(
                        Severity::Error,
                        "map generation",
                        &format!("files/levels.toml: act {act}, seed {seed}"),
                        e,
                    );
                    continue;
                }
            };
            let mut game = d2_sim::game::Game::new();
            game.lists.set_act_count(registry.count());
            game.lists.ensure_act(act)?;
            for l in live.maps.level.iter().filter(|l| owner(l.id) == Some(act)) {
                let at = format!("files/levels.toml: level {}, seed {seed}", l.id);
                let run = (|| -> anyhow::Result<()> {
                    let idx = drlg.get_or_alloc_level(&src.data, &mut types, l.id)?;
                    drlg.generate_level(&src.data, &mut types, idx)?;
                    let rooms = drlg.level_rooms(idx);
                    for &r in &rooms {
                        let mut svc = Services {
                            data: &src.data,
                            tiles: &live.files.dt1,
                            types: &mut types,
                            rooms: &mut game.lists,
                        };
                        drlg.stream_room(&mut svc, r)?;
                    }
                    if !types.borrow().errors.is_empty() {
                        anyhow::bail!("level pipeline errors: {:?}", types.borrow().errors);
                    }
                    let rect = drlg.level(idx).rect;
                    let w = rect.w as usize * 5;
                    let h = rect.h as usize * 5;
                    let start = [
                        usize::from(l.entry[0]) * 5 + 2,
                        usize::from(l.entry[1]) * 5 + 2,
                    ];
                    // Flatten actual room grids once; point lookup would scan every room for every subtile.
                    let mut flat = FlatGrid(d2_sim::drlg::CollisionGrid {
                        rect: d2_sim::drlg::TileRect {
                            x: 0,
                            y: 0,
                            w: w as i32,
                            h: h as i32,
                        },
                        masks: vec![d2_sim::path::collision::MISSING_ROOM; w * h],
                    });
                    for &r in &rooms {
                        if let Some(active) = drlg.room(r).active() {
                            let grid = &active.collision;
                            for y in 0..grid.rect.h {
                                for x in 0..grid.rect.w {
                                    let lx = grid.rect.x + x - rect.x * 5;
                                    let ly = grid.rect.y + y - rect.y * 5;
                                    if lx >= 0 && ly >= 0 && (lx as usize) < w && (ly as usize) < h
                                    {
                                        flat.0.masks[ly as usize * w + lx as usize] =
                                            grid.masks[(y * grid.rect.w + x) as usize];
                                    }
                                }
                            }
                        }
                    }
                    let reached = flood(w, h, start, |x, y| flat.player_clear(x, y));
                    if !reached[start[1] * w + start[0]] {
                        anyhow::bail!("entry is blocked by movement collision");
                    }
                    for warp in &l.warp {
                        let [x, y] = warp.at;
                        let (x, y) = (usize::from(x) * 5 + 2, usize::from(y) * 5 + 2);
                        if !reached[y * w + x] {
                            anyhow::bail!(
                                "warp slot {} to level {} is unreachable from the entry",
                                warp.slot,
                                warp.to
                            );
                        }
                        let class = src.data.levels[l.id as usize].warp[usize::from(warp.slot)];
                        let has_unit = rooms.iter().any(|r| {
                            types.borrow().presets.values().any(|p| {
                                p.room_units(*r)
                                    .iter()
                                    .any(|u| u.unit_type == 5 && u.class == class)
                            })
                        });
                        if !has_unit {
                            anyhow::bail!("warp slot {} produced no runtime warp unit", warp.slot);
                        }
                        if !rooms.iter().any(|r| {
                            drlg.room(*r)
                                .warp_links
                                .iter()
                                .any(|w| drlg.level(drlg.room(w.target).level).id == warp.to)
                        }) {
                            anyhow::bail!("warp slot {} produced no room link", warp.slot);
                        }
                    }
                    let realized = live
                        .maps
                        .preview(l.id, seed, &live.files.ds1)
                        .map_err(anyhow::Error::msg)?;
                    for unit in realized.units.iter().filter(|u| u.unit_type == 1) {
                        if !reached[unit.y as usize * w + unit.x as usize] {
                            anyhow::bail!(
                                "spawned monster {} at {},{} is unreachable from the entry",
                                unit.class,
                                unit.x,
                                unit.y
                            );
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = run {
                    report.finding(Severity::Error, "unreachable map warp or spawn", &at, e);
                }
            }
        }
    }
    report.completed.push(format!(
        "map connectivity and reachable warps ({seeds} seeds, static movement collision)"
    ));
    Ok(())
}
// One contiguous view avoids room scans while using the engine's player footprint query.
struct FlatGrid(d2_sim::drlg::CollisionGrid);
impl FlatGrid {
    fn player_clear(&self, x: usize, y: usize) -> bool {
        d2_sim::path::collision::size_value(
            self,
            Some(d2_sim::units::RoomId(0)),
            x as i32,
            y as i32,
            d2_sim::path::UnitShape::Player.size(),
            d2_sim::path::collision::masks::PLAYER_MOVE,
        ) == 0
    }
}
impl d2_sim::path::collision::CollisionRooms for FlatGrid {
    fn subtile_rect(&self, r: d2_sim::units::RoomId) -> Option<d2_sim::drlg::TileRect> {
        (r.0 == 0).then_some(self.0.rect)
    }
    fn adjacent_count(&self, _: d2_sim::units::RoomId) -> usize {
        0
    }
    fn adjacent(&self, _: d2_sim::units::RoomId, _: usize) -> Option<d2_sim::units::RoomId> {
        None
    }
    fn grid(&self, r: d2_sim::units::RoomId) -> Option<&d2_sim::drlg::CollisionGrid> {
        (r.0 == 0).then_some(&self.0)
    }
    fn grid_mut(&mut self, r: d2_sim::units::RoomId) -> Option<&mut d2_sim::drlg::CollisionGrid> {
        (r.0 == 0).then_some(&mut self.0)
    }
}
fn flood(
    w: usize,
    h: usize,
    start: [usize; 2],
    mut walkable: impl FnMut(usize, usize) -> bool,
) -> Vec<bool> {
    let mut seen = vec![false; w * h];
    let mut tested = vec![false; w * h];
    let mut queue = VecDeque::new();
    if start[0] < w && start[1] < h && walkable(start[0], start[1]) {
        tested[start[1] * w + start[0]] = true;
        seen[start[1] * w + start[0]] = true;
        queue.push_back((start[0], start[1]));
    }
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [
            (0isize, -1isize),
            (1, -1),
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
        ] {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            let i = ny * w + nx;
            if !tested[i] {
                tested[i] = true;
                if walkable(nx, ny) {
                    seen[i] = true;
                    queue.push_back((nx, ny));
                }
            }
        }
    }
    seen
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flood_respects_subtile_barriers_and_blocked_entry() {
        let r = flood(5, 5, [0, 0], |x, _| x != 2);
        assert!(r[4 * 5 + 1]);
        assert!(!r[3]);
        assert!(!flood(5, 5, [0, 0], |x, y| x != 0 || y != 0)
            .into_iter()
            .any(|r| r));
    }
    #[test]
    fn movement_uses_player_footprint_and_all_eight_path_directions() {
        let mut grid = FlatGrid(d2_sim::drlg::CollisionGrid {
            rect: d2_sim::drlg::TileRect {
                x: 0,
                y: 0,
                w: 7,
                h: 7,
            },
            masks: vec![1; 49],
        });
        for y in 0..7 {
            grid.0.masks[y * 7 + 3] = 0;
        }
        assert!(
            !grid.player_clear(3, 3),
            "a one-subtile corridor cannot fit a player"
        );
        for y in 0..7 {
            grid.0.masks[y * 7 + 2] = 0;
            grid.0.masks[y * 7 + 4] = 0;
        }
        assert!(grid.player_clear(3, 3));
        assert!(!grid.player_clear(3, 0));
        grid.0.masks.fill(1);
        for (x, y) in [
            (2, 2),
            (1, 2),
            (3, 2),
            (2, 1),
            (2, 3),
            (3, 3),
            (4, 3),
            (3, 4),
        ] {
            grid.0.masks[y * 7 + x] = 0;
        }
        assert!(grid.player_clear(2, 2));
        assert!(grid.player_clear(3, 3));
        assert!(!grid.player_clear(3, 2));
        assert!(!grid.player_clear(2, 3));
        assert!(flood(7, 7, [2, 2], |x, y| grid.player_clear(x, y))[3 * 7 + 3]);
    }
}
