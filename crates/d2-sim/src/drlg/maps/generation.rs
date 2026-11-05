//! Seeded mod generators. Outputs feed the same preset room/tile pipeline as authored maps.
use super::{bad, Level};
use crate::drlg::preset::Ds1Input;
use crate::rng::Seed;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Generation {
    #[default]
    Preset,
    Maze {
        cell: u16,
        floor: u32,
        wall: u32,
        orientation: u8,
    },
    Outdoor {
        density: u8,
        floor: u32,
        wall: u32,
        orientation: u8,
    },
}
impl Generation {
    pub fn validate(&self, id: u32) -> Result<(), String> {
        match self {
            Self::Preset => Ok(()),
            Self::Maze {
                cell,
                floor,
                wall,
                orientation,
            } if (2..=16).contains(cell) && valid_brushes(*floor, *wall, *orientation) => Ok(()),
            Self::Outdoor {
                density,
                floor,
                wall,
                orientation,
            } if *density <= 80 && valid_brushes(*floor, *wall, *orientation) => Ok(()),
            _ => Err(bad(
                id,
                "generation needs maze cell 2..16 / outdoor density 0..80, nonzero brushes, and wall orientation 1..9",
            )),
        }
    }
}
fn valid_brushes(f: u32, w: u32, o: u8) -> bool {
    f & 0xff != 0
        && w & 0xff != 0
        && (1..=9).contains(&o)
        && f & 0x8000_0000 == 0
        && w & 0x8000_0000 == 0
}
pub fn generate(l: &Level, map: &mut Ds1Input, seed: &mut Seed) {
    let (floor, wall, orientation) = match l.generation {
        Generation::Preset => return,
        Generation::Maze {
            floor,
            wall,
            orientation,
            ..
        }
        | Generation::Outdoor {
            floor,
            wall,
            orientation,
            ..
        } => (floor, wall, u32::from(orientation)),
    };
    let w = usize::from(l.size[0]);
    let h = usize::from(l.size[1]);
    let stride = w + 1;
    let n = stride * (h + 1);
    map.walls = vec![vec![wall; n]];
    map.orientations = vec![vec![orientation; n]];
    map.floors = vec![vec![floor; n]];
    map.shadow = vec![0; n];
    let open = |m: &mut Ds1Input, x: usize, y: usize| {
        if x < w && y < h {
            let i = y * stride + x;
            m.walls[0][i] = 0;
            m.orientations[0][i] = 0;
        }
    };
    match l.generation {
        Generation::Maze { cell, .. } => {
            let cell = usize::from(cell);
            let cw = (w - 2) / cell;
            let ch = (h - 2) / cell;
            if cw > 0 && ch > 0 {
                let mut seen = vec![false; cw * ch];
                let mut stack = vec![(0usize, 0usize)];
                seen[0] = true;
                open(map, 1, 1);
                while let Some(&(x, y)) = stack.last() {
                    let mut choices = Vec::new();
                    for (dx, dy) in [(0isize, -1isize), (1, 0), (0, 1), (-1, 0)] {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        if nx >= 0
                            && ny >= 0
                            && (nx as usize) < cw
                            && (ny as usize) < ch
                            && !seen[ny as usize * cw + nx as usize]
                        {
                            choices.push((nx as usize, ny as usize));
                        }
                    }
                    if choices.is_empty() {
                        stack.pop();
                        continue;
                    }
                    let (nx, ny) = choices[seed.roll(choices.len() as i32) as usize];
                    seen[ny * cw + nx] = true;
                    for yy in (1 + y * cell).min(1 + ny * cell)..=(1 + y * cell).max(1 + ny * cell)
                    {
                        for xx in
                            (1 + x * cell).min(1 + nx * cell)..=(1 + x * cell).max(1 + nx * cell)
                        {
                            open(map, xx, yy);
                        }
                    }
                    stack.push((nx, ny));
                }
            }
        }
        Generation::Outdoor { density, .. } => {
            for y in 1..h - 1 {
                for x in 1..w - 1 {
                    if seed.roll(100) >= u32::from(density) {
                        open(map, x, y);
                    }
                }
            }
        }
        Generation::Preset => {}
    }
    // Author-declared points always have a corridor to the entry. Collision is still checked from DT1 data.
    let mut points = vec![[1, 1]];
    points.extend(l.warp.iter().map(|w| w.at));
    points.extend(l.object.iter().map(|o| o.at));
    for s in &l.spawn {
        let [x, y, w, h] = s.rect;
        points.push([x + w / 2, y + h / 2]);
        for yy in y..y + h {
            for xx in x..x + w {
                open(map, usize::from(xx), usize::from(yy));
            }
        }
    }
    for [x, y] in points {
        let [ex, ey] = l.entry;
        for xx in ex.min(x)..=ex.max(x) {
            open(map, usize::from(xx), usize::from(ey));
        }
        for yy in ey.min(y)..=ey.max(y) {
            open(map, usize::from(x), usize::from(yy));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    fn level(generation: Generation) -> Level {
        Level {
            id: 1,
            size: [32, 32],
            offset: [100, 100],
            entry: [2, 2],
            files: vec!["test.ds1".into()],
            tiles: vec!["test.dt1".into()],
            preset: vec![],
            warp: vec![super::super::Warp {
                slot: 0,
                to: 2,
                at: [29, 29],
            }],
            object: vec![],
            spawn: vec![super::super::Spawn {
                monster: 1,
                rect: [10, 10, 4, 4],
                count: 2,
            }],
            generation,
        }
    }
    fn grid(l: &Level, seed: u32) -> Ds1Input {
        let n = 33 * 33;
        let mut m = Ds1Input {
            version: 18,
            width: 32,
            height: 32,
            walls: vec![vec![0; n]],
            orientations: vec![vec![0; n]],
            floors: vec![vec![7; n]],
            shadow: vec![0; n],
            ..Default::default()
        };
        generate(l, &mut m, &mut Seed::init_low(seed));
        m
    }
    #[test]
    fn presets_leave_grids_and_rng_unchanged() {
        let l = level(Generation::Preset);
        let mut m = grid(&l, 0);
        let old = m.clone();
        let mut seed = Seed::init_low(42);
        let old_seed = seed;
        generate(&l, &mut m, &mut seed);
        assert_eq!(m, old);
        assert_eq!(seed, old_seed);
    }
    #[test]
    fn dozens_of_seeds_vary_both_generators_and_reproduce_anchors() {
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
            generation.validate(1).unwrap();
            let l = level(generation);
            let mut layouts = BTreeSet::new();
            for seed in 1..=32 {
                let m = grid(&l, seed);
                assert_eq!(m, grid(&l, seed));
                layouts.insert(m.walls[0].clone());
                for [x, y] in [[2, 2], [29, 29], [10, 10], [13, 13]] {
                    assert_eq!(m.walls[0][y * 33 + x], 0);
                }
            }
            assert!(layouts.len() > 24, "only {} layouts", layouts.len());
        }
    }
    #[test]
    fn generator_parameters_are_bounded_and_hidden_markers_are_not_brushes() {
        for generation in [
            Generation::Maze {
                cell: 1,
                floor: 1,
                wall: 1,
                orientation: 1,
            },
            Generation::Outdoor {
                density: 81,
                floor: 1,
                wall: 1,
                orientation: 1,
            },
            Generation::Maze {
                cell: 3,
                floor: 0,
                wall: 1,
                orientation: 1,
            },
            Generation::Maze {
                cell: 3,
                floor: 1,
                wall: 0x80000001,
                orientation: 1,
            },
            Generation::Outdoor {
                density: 20,
                floor: 1,
                wall: 1,
                orientation: 10,
            },
        ] {
            assert!(generation.validate(1).is_err());
        }
    }
}
