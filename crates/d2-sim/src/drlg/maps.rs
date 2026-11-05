//! Versioned, opt-in level definitions. Coordinates are tiles; units use tile centres.
pub mod generation;
use super::preset::{Ds1Input, Ds1Source, PresetData, PresetDef, PresetUnit};
use super::{DrlgData, WarpDef};
use crate::rng::Seed;
use generation::Generation;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Maps {
    pub version: u32,
    #[serde(default)]
    pub level: Vec<Level>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub id: u32,
    pub size: [u16; 2],
    #[serde(default)]
    pub offset: [i32; 2],
    pub entry: [u16; 2],
    pub files: Vec<String>,
    pub tiles: Vec<String>,
    #[serde(default)]
    pub generation: Generation,
    #[serde(default)]
    pub preset: Vec<Stamp>,
    #[serde(default)]
    pub warp: Vec<Warp>,
    #[serde(default)]
    pub object: Vec<Object>,
    #[serde(default)]
    pub spawn: Vec<Spawn>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    pub path: String,
    pub at: [u16; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warp {
    pub slot: u8,
    pub to: u32,
    pub at: [u16; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub class: u16,
    pub at: [u16; 2],
    #[serde(default)]
    pub mode: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spawn {
    pub monster: u16,
    pub rect: [u16; 4],
    pub count: u16,
}

fn bad(id: u32, detail: impl std::fmt::Display) -> String {
    format!("files/levels.toml: level {id}: {detail}")
}
fn path_ok(p: &str, suffix: &str) -> bool {
    !p.is_empty()
        && p.len() <= 180
        && p.ends_with(suffix)
        && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
        && p.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"/_-.".contains(&b))
        && !p.starts_with("__modkit/")
}
impl Maps {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 1024 * 1024 {
            return Err("files/levels.toml exceeds 1 MiB".into());
        }
        let mut maps: Self = toml::from_str(text).map_err(|e| format!("files/levels.toml: {e}"))?;
        if maps.version == 1 {
            if maps
                .level
                .iter()
                .any(|l| l.generation != Generation::Preset)
            {
                return Err("files/levels.toml: generation parameters require version 2".into());
            }
            maps.version = 2;
        }
        maps.validate()?;
        Ok(maps)
    }
    /// Canonical authoring output; keep the format writer with the validated schema.
    pub fn to_toml(&self) -> Result<String, String> {
        self.validate()?;
        toml::to_string_pretty(self).map_err(|e| format!("files/levels.toml: {e}"))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 2 || self.level.is_empty() || self.level.len() > 255 {
            return Err("files/levels.toml: version 2 requires 1..255 levels".into());
        }
        let mut ids = BTreeSet::new();
        let mut total = 0usize;
        for l in &self.level {
            let [w, h] = l.size;
            l.generation.validate(l.id)?;
            if let Generation::Maze { cell, .. } = l.generation {
                if w < 2 * cell + 2 || h < 2 * cell + 2 {
                    return Err(bad(
                        l.id,
                        "maze must fit at least two cells along each dimension",
                    ));
                }
            }
            if !(1..=255).contains(&l.id)
                || !ids.insert(l.id)
                || !(8..=256).contains(&w)
                || !(8..=256).contains(&h)
            {
                return Err(bad(l.id, "unique id 1..255 and size 8..256 required"));
            }
            if l.offset.iter().any(|v| !(0..=12000).contains(v))
                || l.offset[0] + i32::from(w) > 13000
                || l.offset[1] + i32::from(h) > 13000
            {
                return Err(bad(l.id, "offset exceeds the movement coordinate bound"));
            }
            let inside = |[x, y]: [u16; 2]| x < w && y < h;
            if !inside(l.entry)
                || !(1..=6).contains(&l.files.len())
                || !(1..=32).contains(&l.tiles.len())
                || l.files.iter().any(|p| !path_ok(p, ".ds1"))
                || l.tiles.iter().any(|p| !path_ok(p, ".dt1"))
            {
                return Err(bad(
                    l.id,
                    "entry, files or tile paths invalid (paths are relative to data/global/tiles)",
                ));
            }
            total += (usize::from(w) + 1) * (usize::from(h) + 1) * l.files.len();
            if l.warp.len() > 8
                || l.preset.len() > 64
                || l.object.len() > 256
                || l.spawn.len() > 64
                || l.spawn.iter().map(|r| usize::from(r.count)).sum::<usize>() > 256
            {
                return Err(bad(l.id, "placement limit exceeded"));
            }
            let mut slots = BTreeSet::new();
            let mut markers = BTreeSet::from([l.entry]);
            for warp in &l.warp {
                if warp.slot > 7
                    || !slots.insert(warp.slot)
                    || !(1..=255).contains(&warp.to)
                    || warp.to == l.id
                    || !inside(warp.at)
                    || !markers.insert(warp.at)
                {
                    return Err(bad(
                        l.id,
                        "warp slot, destination or position invalid/duplicated",
                    ));
                }
            }
            for p in &l.preset {
                if !inside(p.at) || !path_ok(&p.path, ".ds1") {
                    return Err(bad(l.id, "preset path/position invalid"));
                }
            }
            for o in &l.object {
                if !inside(o.at) || o.mode > 7 {
                    return Err(bad(l.id, "object outside level or mode outside 0..7"));
                }
            }
            for s in &l.spawn {
                let [x, y, w0, h0] = s.rect;
                if w0 == 0
                    || h0 == 0
                    || u32::from(x) + u32::from(w0) > u32::from(w)
                    || u32::from(y) + u32::from(h0) > u32::from(h)
                    || s.count == 0
                {
                    return Err(bad(l.id, "spawn region/count invalid"));
                }
            }
        }
        if total > 2_000_000 {
            return Err("files/levels.toml: total map cells exceed 2000000".into());
        }
        Ok(())
    }
    /// Apply to reduced views only; the patched levels table still owns names/population columns.
    pub fn apply(&self, data: &mut DrlgData, presets: &mut PresetData) -> Result<(), String> {
        self.validate()?;
        let needed: BTreeSet<u8> = self
            .level
            .iter()
            .flat_map(|l| l.warp.iter().map(|w| w.slot))
            .collect();
        let used: BTreeSet<i32> = data.warps.iter().map(|w| w.id).collect();
        let free: Vec<i32> = (0..=255)
            .rev()
            .filter(|id| !used.contains(id))
            .take(needed.len())
            .collect();
        if free.len() != needed.len() {
            return Err("files/levels.toml: not enough free byte-sized warp classes".into());
        }
        let classes: BTreeMap<u8, i32> = needed.into_iter().zip(free).collect();
        for &id in classes.values() {
            data.warps.push(WarpDef {
                id,
                direction: b'b',
                offset_x: 2,
                offset_y: 2,
                select: (-32, -48, 64, 64),
                ..Default::default()
            });
            data.warp_exits.resize(data.warps.len(), (0, 0));
        }
        for l in &self.level {
            let row = data.levels.get_mut(l.id as usize).ok_or_else(|| {
                bad(
                    l.id,
                    "no levels/leveldefs table row; add it through a table patch first",
                )
            })?;
            row.drlg_type = 2;
            row.size = [(i32::from(l.size[0]), i32::from(l.size[1])); 3];
            row.offset = (l.offset[0], l.offset[1]);
            row.depend = 0;
            row.position = 0;
            row.vis = [0; 8];
            row.warp = [-1; 8];
            for w in &l.warp {
                row.vis[usize::from(w.slot)] = w.to;
                row.warp[usize::from(w.slot)] = classes[&w.slot];
            }
            row.level_type = data.lvltypes.len() as u32;
            data.lvltypes
                .push(l.tiles.iter().map(|s| s.as_bytes().to_vec()).collect());
            for p in &mut presets.defs {
                if p.level_id == l.id {
                    p.level_id = 0;
                }
            }
            let mut file: [Vec<u8>; 6] = Default::default();
            for (slot, path) in file.iter_mut().zip(&l.files) {
                *slot = path.as_bytes().to_vec();
            }
            presets.defs.push(PresetDef {
                def: presets.defs.len() as u32,
                level_id: l.id,
                populate: 1,
                scan: 1,
                size_x: u32::from(l.size[0]),
                size_y: u32::from(l.size[1]),
                files: l.files.len() as i32,
                file,
                dt1_mask: if l.tiles.len() == 32 {
                    u32::MAX
                } else {
                    (1 << l.tiles.len()) - 1
                },
                ..Default::default()
            });
        }
        Ok(())
    }
    pub fn alias(id: u32) -> Vec<u8> {
        format!("__modkit/level-{id}.ds1").into_bytes()
    }
    pub fn aliases(&self, presets: &mut PresetData) {
        for l in &self.level {
            if let Some(i) = presets.def_for_level(l.id) {
                let p = &mut presets.defs[i as usize];
                p.files = 1;
                p.file = Default::default();
                p.file[0] = Self::alias(l.id);
            }
        }
    }
    /// Shared by runtime and authoring tools. `init_seed` is the game's public map seed.
    pub fn preview(
        &self,
        id: u32,
        init_seed: u32,
        src: &dyn Ds1Source,
    ) -> Result<Realized, String> {
        let start = Seed::init_low(init_seed).step();
        self.realize(id, start, src)
    }
    pub fn realize(
        &self,
        id: u32,
        start_seed: u32,
        src: &dyn Ds1Source,
    ) -> Result<Realized, String> {
        self.validate()?;
        let l = self
            .level
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| bad(id, "not declared"))?;
        let mut seed = Seed::init_low(start_seed.wrapping_add(id));
        let path = &l.files[seed.roll(l.files.len() as i32) as usize];
        let mut map = src
            .ds1(path.as_bytes())
            .cloned()
            .ok_or_else(|| bad(id, format!("missing {path}")))?;
        if map.version < 7
            || map.width != u32::from(l.size[0])
            || map.height != u32::from(l.size[1])
        {
            return Err(bad(
                id,
                "DS1 version/size differs from declared size (stored dimensions, without border)",
            ));
        }
        check_grid(id, &map)?;
        generation::generate(l, &mut map, &mut seed);
        for p in &l.preset {
            let piece = src
                .ds1(p.path.as_bytes())
                .ok_or_else(|| bad(id, format!("missing preset {}", p.path)))?;
            stamp(id, &mut map, piece, p.at)?;
        }
        // The level file owns entry/warp markers, including those inherited from stamped DS1s.
        for (walls, orientations) in map.walls.iter_mut().zip(&mut map.orientations) {
            for (v, o) in walls.iter_mut().zip(orientations) {
                let style = (*v >> 20) & 0x3f;
                if matches!(*o, 10 | 11) && (style <= 7 || (30..=33).contains(&style)) {
                    *v = 0;
                    *o = 0;
                }
            }
        }
        marker(id, &mut map, l.entry, 30)?;
        for warp in &l.warp {
            marker(id, &mut map, warp.at, u32::from(warp.slot))?;
        }
        let mut units = Vec::new();
        for o in &l.object {
            units.push(unit(2, o.class, o.at, u32::from(o.mode)));
        }
        for s in &l.spawn {
            for _ in 0..s.count {
                let [x, y, w, h] = s.rect;
                let at = [
                    x + seed.roll(i32::from(w)) as u16,
                    y + seed.roll(i32::from(h)) as u16,
                ];
                units.push(unit(1, s.monster, at, 1));
            }
        }
        Ok(Realized { map, units })
    }
}
fn unit(kind: u32, class: u16, [x, y]: [u16; 2], mode: u32) -> PresetUnit {
    PresetUnit {
        unit_type: kind,
        class: i32::from(class),
        mode,
        x: i32::from(x) * 5 + 2,
        y: i32::from(y) * 5 + 2,
        flags: 0,
        path: None,
    }
}
fn check_grid(id: u32, map: &Ds1Input) -> Result<(), String> {
    let n = (map.width as usize + 1) * (map.height as usize + 1);
    if map.walls.len() != map.orientations.len()
        || map.walls.len() > 4
        || map.floors.len() > 2
        || map
            .walls
            .iter()
            .chain(&map.orientations)
            .chain(&map.floors)
            .chain(std::iter::once(&map.shadow))
            .any(|g| g.len() != n)
    {
        return Err(bad(id, "DS1 grid/layer counts invalid"));
    }
    if map.objects.len() > 4096
        || map.paths.len() > 4096
        || map.paths.iter().map(|p| p.points.len()).sum::<usize>() > 65536
        || map
            .objects
            .iter()
            .any(|o| o.x > map.width * 5 || o.y > map.height * 5)
        || map.paths.iter().any(|p| {
            p.x > map.width * 5
                || p.y > map.height * 5
                || p.points.len() > 4096
                || p.points
                    .iter()
                    .any(|&(x, y, _)| x > map.width * 5 || y > map.height * 5)
        })
    {
        return Err(bad(
            id,
            "DS1 unit/path count or position exceeds map bounds",
        ));
    }
    Ok(())
}
fn marker(id: u32, map: &mut Ds1Input, [x, y]: [u16; 2], style: u32) -> Result<(), String> {
    let n = (map.width as usize + 1) * (map.height as usize + 1);
    let i = usize::from(y) * (map.width as usize + 1) + usize::from(x);
    let layer = match map
        .walls
        .iter()
        .zip(&map.orientations)
        .position(|(w, o)| w[i] == 0 && o[i] == 0)
    {
        Some(i) => i,
        None if map.walls.len() < 4 => {
            map.walls.push(vec![0; n]);
            map.orientations.push(vec![0; n]);
            map.walls.len() - 1
        }
        None => return Err(bad(id, "no free wall layer for entry/warp marker")),
    };
    map.walls[layer][i] = 0x8000_0000 | (style << 20);
    map.orientations[layer][i] = 10;
    Ok(())
}
fn stamp(id: u32, dst: &mut Ds1Input, src: &Ds1Input, [x, y]: [u16; 2]) -> Result<(), String> {
    if src.width > 256 || src.height > 256 {
        return Err(bad(id, "preset exceeds 256 tiles"));
    }
    check_grid(id, src)?;
    if u32::from(x) + src.width > dst.width
        || u32::from(y) + src.height > dst.height
        || src.version < 7
    {
        return Err(bad(
            id,
            "preset exceeds map bounds or uses DS1 version below 7",
        ));
    }
    let n = (dst.width as usize + 1) * (dst.height as usize + 1);
    fn layers(
        dst: &mut Vec<Vec<u32>>,
        src: &[Vec<u32>],
        n: usize,
        stride: usize,
        [sw, sh]: [usize; 2],
        [x, y]: [usize; 2],
    ) {
        dst.resize_with(dst.len().max(src.len()), || vec![0; n]);
        for (d, s) in dst.iter_mut().zip(src) {
            for yy in 0..sh {
                for xx in 0..sw {
                    let v = s[yy * sw + xx];
                    if v != 0 {
                        d[(y + yy) * stride + x + xx] = v;
                    }
                }
            }
        }
    }
    let stride = dst.width as usize + 1;
    let sw = src.width as usize + 1;
    let sh = src.height as usize + 1;
    layers(
        &mut dst.walls,
        &src.walls,
        n,
        stride,
        [sw, sh],
        [usize::from(x), usize::from(y)],
    );
    layers(
        &mut dst.orientations,
        &src.orientations,
        n,
        stride,
        [sw, sh],
        [usize::from(x), usize::from(y)],
    );
    layers(
        &mut dst.floors,
        &src.floors,
        n,
        stride,
        [sw, sh],
        [usize::from(x), usize::from(y)],
    );
    for yy in 0..sh {
        for xx in 0..sw {
            let v = src.shadow[yy * sw + xx];
            if v != 0 {
                dst.shadow[(usize::from(y) + yy) * stride + usize::from(x) + xx] = v;
            }
        }
    }
    for o in &src.objects {
        let mut o = o.clone();
        o.x += u32::from(x) * 5;
        o.y += u32::from(y) * 5;
        dst.objects.push(o);
    }
    for p in &src.paths {
        let mut p = p.clone();
        p.x += u32::from(x) * 5;
        p.y += u32::from(y) * 5;
        for (px, py, _) in &mut p.points {
            *px += u32::from(x) * 5;
            *py += u32::from(y) * 5;
        }
        dst.paths.push(p);
    }
    check_grid(id, dst)
}
#[derive(Clone, Debug)]
pub struct Realized {
    pub map: Ds1Input,
    pub units: Vec<PresetUnit>,
}
pub struct MapSource {
    pub base: Box<dyn Ds1Source + Send + Sync>,
    pub maps: BTreeMap<Vec<u8>, Realized>,
}
impl Ds1Source for MapSource {
    fn ds1(&self, path: &[u8]) -> Option<&Ds1Input> {
        self.maps
            .get(path)
            .map(|m| &m.map)
            .or_else(|| self.base.ds1(path))
    }
    fn extra_units(&self, path: &[u8]) -> &[PresetUnit] {
        self.maps
            .get(path)
            .map_or_else(|| self.base.extra_units(path), |m| m.units.as_slice())
    }
}
pub struct EmptySource;
impl Ds1Source for EmptySource {
    fn ds1(&self, _: &[u8]) -> Option<&Ds1Input> {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::drlg::preset::{Ds1Cache, PresetTables};
    use d2_data::fixup::maps::ActRanges;
    struct Files(BTreeMap<Vec<u8>, Ds1Input>);
    impl Ds1Source for Files {
        fn ds1(&self, p: &[u8]) -> Option<&Ds1Input> {
            self.0.get(p)
        }
    }
    fn blank(w: u32, h: u32) -> Ds1Input {
        let n = ((w + 1) * (h + 1)) as usize;
        Ds1Input {
            version: 18,
            width: w,
            height: h,
            walls: vec![vec![0; n]],
            orientations: vec![vec![0; n]],
            floors: vec![vec![0x100001; n]],
            shadow: vec![0; n],
            ..Default::default()
        }
    }
    fn maps() -> Maps {
        Maps::parse("version=1\n[[level]]\nid=1\nsize=[16,16]\nentry=[2,2]\nfiles=['test.ds1']\ntiles=['test.dt1']\n[[level.object]]\nclass=3\nat=[4,4]\n[[level.spawn]]\nmonster=2\nrect=[5,5,6,6]\ncount=12\n").unwrap()
    }
    fn tables() -> PresetData {
        PresetData {
            defs: vec![PresetDef::default()],
            monpreset_acts: ActRanges::default(),
            monpreset: vec![],
            monstats_count: 10,
            superuniques_count: 0,
            hdm_item: -1,
            tables: PresetTables::spec().unwrap(),
        }
    }
    #[test]
    fn authored_units_use_runtime_classes_and_cache_aliases() {
        let maps = maps();
        let files = Files(BTreeMap::from([(b"test.ds1".to_vec(), blank(16, 16))]));
        let a = maps.preview(1, 42, &files).unwrap();
        let b = maps.preview(1, 42, &files).unwrap();
        assert_eq!(a.map, b.map);
        assert_eq!(a.units, b.units);
        assert_eq!(a.units.len(), 13);
        assert_eq!(
            (
                a.units[0].unit_type,
                a.units[0].class,
                a.units[0].x,
                a.units[0].y
            ),
            (2, 3, 22, 22)
        );
        assert!(a.units[1..].iter().all(|u| u.class == 2
            && u.mode == 1
            && (25..55).contains(&u.x)
            && (25..55).contains(&u.y)));
        assert_ne!(a.units, maps.preview(1, 43, &files).unwrap().units);
        assert_eq!(a.map.orientations[0][2 * 17 + 2], 10);
        let src = MapSource {
            base: Box::new(files),
            maps: BTreeMap::from([(Maps::alias(1), a)]),
        };
        let mut cache = Ds1Cache::default();
        let id = cache.load(&Maps::alias(1), &src, &tables()).unwrap();
        assert_eq!(cache.file(id).units.len(), 13);
        assert_eq!(cache.file(id).units[0].class, 3);
        let native = cache.load(b"test.ds1", &src, &tables()).unwrap();
        assert!(cache.file(native).units.is_empty());
    }
    #[test]
    fn level_views_are_opt_in_and_keep_existing_rows_and_byte_warps() {
        let mut data = DrlgData {
            levels: vec![Default::default(); 3],
            ..Default::default()
        };
        let native = data.clone();
        let mut pd = tables();
        let old = pd.clone();
        Maps::default().aliases(&mut pd);
        assert_eq!(pd.defs, old.defs);
        assert_eq!(data, native);
        let mut maps = maps();
        maps.level[0].warp.push(Warp {
            slot: 7,
            to: 2,
            at: [14, 14],
        });
        maps.apply(&mut data, &mut pd).unwrap();
        assert_eq!(data.levels[2], native.levels[2]);
        assert_eq!(data.levels[1].size, [(16, 16); 3]);
        assert_eq!(data.levels[1].vis[7], 2);
        assert!((0..=255).contains(&data.levels[1].warp[7]));
        assert_eq!(pd.defs[1].file[0], b"test.ds1");
        maps.aliases(&mut pd);
        assert_eq!(pd.defs[1].file[0], Maps::alias(1));
    }
    #[test]
    fn presets_copy_layers_objects_and_paths_with_offsets() {
        let mut dst = blank(16, 16);
        let mut src = blank(2, 2);
        src.floors[0][0] = 0x300001;
        src.objects.push(super::super::preset::Ds1ObjectInput {
            kind: 2,
            id: 153,
            x: 2,
            y: 3,
            flags: 0,
        });
        src.paths.push(super::super::preset::Ds1PathInput {
            x: 2,
            y: 3,
            points: vec![(3, 4, 5)],
        });
        stamp(1, &mut dst, &src, [4, 6]).unwrap();
        assert_eq!(dst.floors[0][6 * 17 + 4], 0x300001);
        assert_eq!(dst.objects[0].x, 22);
        assert_eq!(dst.paths[0].points, [(23, 34, 5)]);
        assert!(stamp(1, &mut dst, &src, [16, 16]).is_err());
    }
    #[test]
    fn schema_rejects_unknowns_traversal_duplicates_explosive_work_and_bad_placements() {
        let base = toml::to_string(&maps()).unwrap();
        assert!(Maps::parse(&base.replace("version = 2", "version = 3")).is_err());
        assert!(Maps::parse(&base.replace("test.ds1", "../test.ds1")).is_err());
        assert!(Maps::parse(&(base.clone() + "\n[unknown]\nx=1\n")).is_err());
        let mut maps = maps();
        maps.level.push(maps.level[0].clone());
        assert!(maps.validate().is_err());
        maps.level.pop();
        maps.level[0].entry = [16, 0];
        assert!(maps.validate().is_err());
        maps.level[0].entry = [1, 1];
        maps.level[0].spawn[0].count = 257;
        assert!(maps.validate().is_err());
        maps.level[0].spawn[0].count = 1;
        maps.level[0].object[0].mode = 8;
        assert!(maps.validate().is_err());
        maps.level[0].object[0].mode = 0;
        maps.level[0].warp = vec![Warp {
            slot: 0,
            to: 2,
            at: maps.level[0].entry,
        }];
        assert!(maps.validate().is_err());
        maps.level[0].warp.clear();
        maps.level[0].size = [256, 256];
        maps.level[0].files = vec!["test.ds1".into(); 6];
        for id in 2..=7 {
            let mut l = maps.level[0].clone();
            l.id = id;
            maps.level.push(l);
        }
        assert!(maps.validate().unwrap_err().contains("total map cells"));
    }
    #[test]
    fn version_one_migrates_to_preset_without_changing_map_or_rng_output() {
        let one = maps();
        let two = Maps::parse(&one.to_toml().unwrap()).unwrap();
        assert_eq!(one.version, 2);
        assert_eq!(one, two);
        assert_eq!(one.level[0].generation, Generation::Preset);
        let files = Files(BTreeMap::from([(b"test.ds1".to_vec(), blank(16, 16))]));
        assert_eq!(
            one.preview(1, 42, &files).unwrap().map,
            two.preview(1, 42, &files).unwrap().map
        );
        assert_eq!(
            one.preview(1, 42, &files).unwrap().units,
            two.preview(1, 42, &files).unwrap().units
        );
        let mut generated = one;
        generated.level[0].generation = Generation::Outdoor {
            density: 20,
            floor: 1,
            wall: 1,
            orientation: 1,
        };
        assert!(Maps::parse(
            &toml::to_string(&generated)
                .unwrap()
                .replace("version = 2", "version = 1")
        )
        .unwrap_err()
        .contains("require version 2"));
        generated.level[0].generation = Generation::Maze {
            cell: 16,
            floor: 1,
            wall: 1,
            orientation: 1,
        };
        assert!(generated.validate().is_err());
    }
}
