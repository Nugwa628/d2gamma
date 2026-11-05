//! Reversible map edits and conflict-checked project writes. Inherited assets are copied only after editing.
use super::{MapFiles, MapProject};
use anyhow::{bail, Context, Result};
use d2_formats::{
    ds1::Ds1,
    dt1::{Dt1, Dt1Block, Dt1Tile},
};
use d2_native::kind::NativeKind;
use d2_sim::drlg::{
    maps::{generation::Generation, Level, Maps},
    preset::Ds1Input,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Floor(usize),
    Wall(usize),
}
#[derive(Clone, Debug)]
enum Edit {
    Cell {
        path: Vec<u8>,
        layer: Layer,
        at: [u16; 2],
        value: u32,
        orientation: u32,
    },
    Registry(Maps),
    Asset {
        path: Vec<u8>,
        map: Box<Ds1>,
    },
    Tiles {
        path: String,
        tiles: Dt1,
    },
}
impl Edit {
    fn bytes(&self) -> usize {
        match self {
            Self::Cell { path, .. } => 64 + path.len(),
            Self::Registry(m) => m
                .level
                .iter()
                .map(|l| {
                    256 + l
                        .files
                        .iter()
                        .chain(&l.tiles)
                        .map(String::len)
                        .sum::<usize>()
                        + l.preset.len() * 256
                        + l.object.len() * 16
                        + l.spawn.len() * 16
                        + l.warp.len() * 16
                })
                .sum(),
            Self::Asset { map, .. } => map
                .walls
                .iter()
                .chain(&map.orientations)
                .chain(&map.floors)
                .chain(std::iter::once(&map.shadow))
                .map(|g| g.len() * 4)
                .sum(),
            Self::Tiles { tiles, .. } => tiles
                .tiles
                .iter()
                .map(|t| 256 + t.blocks.iter().map(|b| 64 + b.pixels.len()).sum::<usize>())
                .sum(),
        }
    }
}
pub struct Document {
    pub project: MapProject,
    baseline: BTreeMap<String, Option<Vec<u8>>>,
    assets: BTreeSet<Vec<u8>>,
    tiles: BTreeSet<String>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    pub dirty: bool,
}
fn map_path(p: &[u8]) -> Result<String> {
    Ok(format!(
        "files/data/global/tiles/{}.toml",
        std::str::from_utf8(p)?
    ))
}
impl Document {
    pub fn new(project: MapProject) -> Result<Self> {
        let mut baseline = BTreeMap::new();
        baseline.insert(
            "files/levels.toml".into(),
            read_project(&project.root, "files/levels.toml")?,
        );
        for p in project.files.decoded.keys() {
            let p = map_path(p)?;
            baseline.insert(p.clone(), read_project(&project.root, &p)?);
        }
        // Capture all DT1 sidecars/blocks too: safe saving must not overwrite edits by another tool.
        for (p, d) in &project.tiles {
            for f in d.write(
                &format!("data/global/tiles/{p}"),
                &d2_native::kind::grey_palette(),
            )? {
                let p = format!("files/{}", f.path);
                baseline.insert(p.clone(), read_project(&project.root, &p)?);
            }
        }
        let doc = Self {
            project,
            baseline,
            assets: BTreeSet::new(),
            tiles: BTreeSet::new(),
            undo: vec![],
            redo: vec![],
            dirty: false,
        };
        doc.validate()?;
        Ok(doc)
    }
    fn apply(&mut self, edit: Edit) -> Result<Edit> {
        let inverse = match edit {
            Edit::Registry(maps) => Edit::Registry(std::mem::replace(&mut self.project.maps, maps)),
            Edit::Asset { path, map } => {
                let map = *map;
                let old = self
                    .project
                    .files
                    .decoded
                    .insert(path.clone(), map.clone())
                    .context("Map disappeared")?;
                self.project
                    .files
                    .inputs
                    .insert(path.clone(), Ds1Input::from(&map));
                self.assets.insert(path.clone());
                Edit::Asset {
                    path,
                    map: Box::new(old),
                }
            }
            Edit::Tiles { path, tiles } => {
                let old = self
                    .project
                    .tiles
                    .insert(path.clone(), tiles)
                    .context("Tile library disappeared")?;
                self.tiles.insert(path.clone());
                Edit::Tiles { path, tiles: old }
            }
            Edit::Cell {
                path,
                layer,
                at: [x, y],
                value,
                orientation,
            } => {
                let d = self
                    .project
                    .files
                    .decoded
                    .get_mut(&path)
                    .context("Map disappeared")?;
                if u32::from(x) >= d.width - 1 || u32::from(y) >= d.height - 1 {
                    bail!("Brush outside map");
                }
                let i = usize::from(y) * d.width as usize + usize::from(x);
                let (values, orients) = match layer {
                    Layer::Floor(l) => (d.floors.get_mut(l).context("Floor layer missing")?, None),
                    Layer::Wall(l) => (
                        d.walls.get_mut(l).context("Wall layer missing")?,
                        Some(
                            d.orientations
                                .get_mut(l)
                                .context("Orientation layer missing")?,
                        ),
                    ),
                };
                let old = values[i];
                values[i] = value;
                let mut old_orientation = 0;
                if let Some(o) = orients {
                    old_orientation = o[i];
                    o[i] = orientation;
                }
                self.project
                    .files
                    .inputs
                    .insert(path.clone(), Ds1Input::from(&*d));
                self.assets.insert(path.clone());
                Edit::Cell {
                    path,
                    layer,
                    at: [x, y],
                    value: old,
                    orientation: old_orientation,
                }
            }
        };
        self.dirty = true;
        Ok(inverse)
    }
    fn record(&mut self, edit: Edit) -> Result<()> {
        let inverse = self.apply(edit)?;
        self.undo.push(inverse);
        while self.undo.len() > 64
            || self.undo.len() > 1
                && self.undo.iter().map(Edit::bytes).sum::<usize>() > 32 * 1024 * 1024
        {
            self.undo.remove(0);
        }
        self.redo.clear();
        Ok(())
    }
    pub fn paint(
        &mut self,
        path: &str,
        layer: Layer,
        at: [u16; 2],
        value: u32,
        orientation: u32,
    ) -> Result<()> {
        let d = self
            .project
            .files
            .decoded
            .get(path.as_bytes())
            .context("Map missing")?;
        if u32::from(at[0]) >= d.width - 1 || u32::from(at[1]) >= d.height - 1 {
            bail!("Brush outside map");
        }
        let i = usize::from(at[1]) * d.width as usize + usize::from(at[0]);
        let same = match layer {
            Layer::Floor(l) => d.floors.get(l).and_then(|v| v.get(i)).copied() == Some(value),
            Layer::Wall(l) => {
                d.walls.get(l).and_then(|v| v.get(i)).copied() == Some(value)
                    && d.orientations.get(l).and_then(|v| v.get(i)).copied() == Some(orientation)
            }
        };
        if !same {
            self.record(Edit::Cell {
                path: path.as_bytes().to_vec(),
                layer,
                at,
                value,
                orientation,
            })?;
        }
        Ok(())
    }
    pub fn update_level(&mut self, level: Level) -> Result<()> {
        let mut maps = self.project.maps.clone();
        let old = maps
            .level
            .iter_mut()
            .find(|l| l.id == level.id)
            .context("Level missing")?;
        if *old != level {
            *old = level;
            self.record(Edit::Registry(maps))?;
        }
        Ok(())
    }
    pub fn add_level(&mut self, source: u32, id: u32) -> Result<()> {
        if !(1..=255).contains(&id) || self.project.maps.level.iter().any(|l| l.id == id) {
            bail!("Choose an unused level id 1..255; add its table row separately when needed");
        }
        let mut maps = self.project.maps.clone();
        let mut level = maps
            .level
            .iter()
            .find(|l| l.id == source)
            .context("Source level missing")?
            .clone();
        level.id = id;
        level.warp.clear();
        level.offset[0] = (level.offset[0] + i32::from(level.size[0]) + 16).min(12000);
        maps.level.push(level);
        maps.validate().map_err(anyhow::Error::msg)?;
        self.record(Edit::Registry(maps))
    }
    pub fn add_layer(&mut self, path: &str, wall: bool) -> Result<()> {
        let mut map = self
            .project
            .files
            .decoded
            .get(path.as_bytes())
            .context("Map missing")?
            .clone();
        let n = (map.width * map.height) as usize;
        if wall {
            if map.walls.len() >= 4 {
                bail!("At most four wall layers");
            }
            map.walls.push(vec![0; n]);
            map.orientations.push(vec![0; n]);
        } else {
            if map.floors.len() >= 2 {
                bail!("At most two floor layers");
            }
            map.floors.push(vec![0; n]);
        }
        self.record(Edit::Asset {
            path: path.as_bytes().to_vec(),
            map: Box::new(map),
        })
    }
    pub fn import_map(&mut self, path: &str) -> Result<()> {
        if self.project.files.decoded.contains_key(path.as_bytes()) {
            return Ok(());
        }
        let rel = map_path(path.as_bytes())?;
        let bytes = read_project(&self.project.root, &rel)?.context("Project sidecar missing")?;
        let d = d2_native::toml_kinds::read_ds1(&rel, std::str::from_utf8(&bytes)?)?;
        if d.version < 7 || d.width < 2 || d.height < 2 || d.width > 257 || d.height > 257 {
            bail!("DS1 must use version >=7 and fit 256 tiles");
        }
        self.project
            .files
            .inputs
            .insert(path.as_bytes().to_vec(), Ds1Input::from(&d));
        self.project
            .files
            .decoded
            .insert(path.as_bytes().to_vec(), d);
        self.baseline.insert(rel, Some(bytes));
        Ok(())
    }
    pub fn update_tiles(&mut self, path: &str, tiles: Dt1) -> Result<()> {
        if self.project.tiles.get(path) != Some(&tiles) {
            self.record(Edit::Tiles {
                path: path.into(),
                tiles,
            })?;
        }
        Ok(())
    }
    pub fn undo(&mut self) -> Result<()> {
        if let Some(edit) = self.undo.pop() {
            let inverse = self.apply(edit)?;
            self.redo.push(inverse);
        }
        Ok(())
    }
    pub fn redo(&mut self) -> Result<()> {
        if let Some(edit) = self.redo.pop() {
            let inverse = self.apply(edit)?;
            self.undo.push(inverse);
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        self.project.maps.validate().map_err(anyhow::Error::msg)?;
        for l in &self.project.maps.level {
            for path in &l.tiles {
                if !self.project.tiles.contains_key(path) {
                    bail!("Level {}: tile library {path} is not loaded", l.id);
                }
            }
            for path in &l.files {
                let mut one = self.project.maps.clone();
                one.level.retain(|x| x.id == l.id);
                one.level[0].files = vec![path.clone()];
                one.preview(l.id, 0, &self.project.files)
                    .map_err(anyhow::Error::msg)?;
            }
        }
        Ok(())
    }
    pub fn save(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        self.validate()?;
        let registry = self.project.maps.to_toml().map_err(anyhow::Error::msg)?;
        // Apply the loader's text-size limit before replacing any project file.
        Maps::parse(&registry).map_err(anyhow::Error::msg)?;
        let mut writes = BTreeMap::from([("files/levels.toml".into(), registry.into_bytes())]);
        for p in &self.assets {
            let name = map_path(p)?;
            let d = &self.project.files.decoded[p];
            writes.insert(
                name.clone(),
                d2_native::toml_kinds::write_ds1(&name, d)?.into_bytes(),
            );
        }
        for p in &self.tiles {
            for f in self.project.tiles[p].write(
                &format!("data/global/tiles/{p}"),
                &d2_native::kind::grey_palette(),
            )? {
                writes.insert(format!("files/{}", f.path), f.bytes);
            }
        }
        for p in writes.keys() {
            if read_project(&self.project.root, p)? != self.baseline.get(p).cloned().flatten() {
                bail!("{p} changed on disk; reload before saving");
            }
        }
        replace_files(&self.project.root, &writes)?;
        for (p, b) in writes {
            self.baseline.insert(p, Some(b));
        }
        self.assets.clear();
        self.tiles.clear();
        self.dirty = false;
        Ok(())
    }
    /// Start a level from art already imported into this project.
    pub fn from_project_assets(root: &Path, id: u32, map: &str, tiles: &str) -> Result<Self> {
        anyhow::ensure!((1..=255).contains(&id), "Choose a level ID 1..255");
        anyhow::ensure!(
            !root.join("files/levels.toml").exists(),
            "This project already has levels; load them first"
        );
        let store = d2_native::kind::MemStore::from_files(&crate::asset_library::library(
            &root.join("files/data"),
        )?);
        let relative = format!("files/data/global/tiles/{map}.toml");
        let bytes = read_project(root, &relative)?.context("Project map art missing")?;
        let ds1 = d2_native::toml_kinds::read_ds1(&relative, std::str::from_utf8(&bytes)?)?;
        let dt1 = Dt1::read(&format!("data/global/tiles/{tiles}"), &store)?;
        let size = [
            u16::try_from(ds1.width.saturating_sub(1))?,
            u16::try_from(ds1.height.saturating_sub(1))?,
        ];
        let project = MapProject {
            root: root.into(),
            maps: Maps {
                version: 2,
                level: vec![Level {
                    id,
                    size,
                    offset: [100, 100],
                    entry: [2, 2],
                    files: vec![map.into()],
                    tiles: vec![tiles.into()],
                    preset: vec![],
                    warp: vec![],
                    object: vec![],
                    spawn: vec![],
                    generation: Generation::Preset,
                }],
            },
            files: MapFiles {
                inputs: BTreeMap::from([(map.as_bytes().to_vec(), Ds1Input::from(&ds1))]),
                decoded: BTreeMap::from([(map.as_bytes().to_vec(), ds1)]),
            },
            tiles: BTreeMap::from([(tiles.into(), dt1)]),
        };
        let mut doc = Self::new(project)?;
        doc.dirty = true;
        Ok(doc)
    }
    /// A distributable floor/wall library and a blank authored town, generated from numbers only.
    pub fn starter(root: &Path) -> Result<Self> {
        if read_project(root, "files/levels.toml")?.is_some() {
            bail!("This project already has levels; load them first");
        }
        let ds1 = blank(32, 32);
        let dt1 = starter_tiles();
        let maps = Maps {
            version: 2,
            level: vec![Level {
                id: 1,
                size: [32, 32],
                offset: [100, 100],
                entry: [2, 2],
                files: vec!["modkit/new.ds1".into()],
                tiles: vec!["modkit/generated.dt1".into()],
                preset: vec![],
                warp: vec![],
                object: vec![],
                spawn: vec![],
                generation: Generation::Preset,
            }],
        };
        let project = MapProject {
            root: root.into(),
            maps,
            files: MapFiles {
                inputs: BTreeMap::from([(b"modkit/new.ds1".to_vec(), Ds1Input::from(&ds1))]),
                decoded: BTreeMap::from([(b"modkit/new.ds1".to_vec(), ds1)]),
            },
            tiles: BTreeMap::from([("modkit/generated.dt1".into(), dt1)]),
        };
        let mut doc = Self::new(project)?;
        doc.assets.insert(b"modkit/new.ds1".to_vec());
        doc.tiles.insert("modkit/generated.dt1".into());
        doc.dirty = true;
        Ok(doc)
    }
}
fn checked(root: &Path, rel: &str) -> Result<PathBuf> {
    d2_modkit::layout::check_rel_path(rel).map_err(anyhow::Error::msg)?;
    let root = root.canonicalize()?;
    let mut p = root;
    for part in rel.split('/') {
        p.push(part);
        match fs::symlink_metadata(&p) {
            Ok(m) if m.file_type().is_symlink() => bail!("{} is a link", p.display()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(p)
}
fn read_project(root: &Path, rel: &str) -> Result<Option<Vec<u8>>> {
    match fs::read(checked(root, rel)?) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
/// Stage every file before replacing any. Roll back replaced files on rename failure.
fn replace_files(root: &Path, writes: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    use std::io::Write;
    let mut staged = Vec::new();
    let result = (|| -> Result<()> {
        for (i, (rel, bytes)) in writes.iter().enumerate() {
            let dest = checked(root, rel)?;
            fs::create_dir_all(dest.parent().context("Parent missing")?)?;
            let temp = dest.with_file_name(format!(".studio-stage-{}-{i}", std::process::id()));
            let backup = dest.with_file_name(format!(".studio-backup-{}-{i}", std::process::id()));
            if backup.exists() {
                bail!(
                    "A previous save backup remains at {}; recover it before saving",
                    backup.display()
                );
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            staged.push((dest, temp, backup, false, false));
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        for (dest, temp, backup, saved, installed) in &mut staged {
            if dest.exists() {
                fs::rename(&*dest, &*backup)?;
                *saved = true;
            }
            fs::rename(&*temp, &*dest)?;
            *installed = true;
        }
        Ok(())
    })();
    if let Err(error) = result {
        for (dest, temp, backup, saved, installed) in staged.iter().rev() {
            if *installed {
                let _ = fs::remove_file(dest);
            }
            if *saved {
                fs::rename(backup, dest).with_context(|| {
                    format!(
                        "Save failed ({error}); restore {} manually",
                        backup.display()
                    )
                })?;
            }
            let _ = fs::remove_file(temp);
        }
        return Err(error);
    }
    for (_, _, backup, saved, _) in staged {
        if saved {
            fs::remove_file(backup)?;
        }
    }
    Ok(())
}
fn blank(w: u16, h: u16) -> Ds1 {
    let n = (usize::from(w) + 1) * (usize::from(h) + 1);
    Ds1 {
        version: 18,
        width: u32::from(w) + 1,
        height: u32::from(h) + 1,
        act: 0,
        tag_type: 0,
        files: vec![b"data\\global\\tiles\\modkit\\generated.dt1".to_vec()],
        unknown_header: None,
        walls: vec![vec![0; n]],
        orientations: vec![vec![0; n]],
        floors: vec![vec![0x100001; n]],
        shadow: vec![0; n],
        tags: None,
        objects: vec![],
        unknown_groups: Some([0; 4]),
        groups: vec![],
        groups_truncated: false,
        paths: vec![],
        trailing: vec![],
    }
}
fn starter_tiles() -> Dt1 {
    let floor = |color: u8, main: u32| {
        let mut blocks = Vec::new();
        for r in 0..5 {
            for c in 0..5 {
                let mut pixels = vec![0; 32 * 15];
                for y in 0..15 {
                    let half = if y < 8 { 2 * (y + 1) } else { 2 * (15 - y) };
                    for x in 16 - half..16 + half {
                        pixels[y * 32 + x] = color;
                    }
                }
                blocks.push(Dt1Block {
                    x: (c - r) * 16 + 64,
                    y: (c + r) * 8,
                    unknown1: 0,
                    grid_x: c as u8,
                    grid_y: r as u8,
                    format: 1,
                    unknown2: 0,
                    pixels,
                });
            }
        }
        Dt1Tile {
            light_direction: 0,
            roof_height: 0,
            material_flags: 0,
            height: 80,
            width: 160,
            unknown_height: 0,
            orientation: 0,
            main_index: main,
            sub_index: 0,
            rarity: 1,
            unknown_color: 0,
            subtile_flags: [0; 25],
            unknown_58: 0,
            cache_index: 0,
            unknown_5c: 0,
            blocks,
        }
    };
    let mut wall = floor(180, 2);
    wall.orientation = 1;
    wall.subtile_flags = [1; 25];
    for b in &mut wall.blocks {
        b.y -= 32;
    }
    Dt1 {
        version: 7,
        minor_version: 6,
        tiles: vec![floor(72, 1), floor(110, 3), wall],
    }
}
/// Inverse of the shared renderer's cell origin, accounting for the diamond centre.
pub fn pick_tile(x: f32, y: f32) -> Option<[u16; 2]> {
    let x = (x - 80.0) / 80.0;
    let y = (y - 40.0) / 40.0;
    let tx = ((x + y) / 2.0).round();
    let ty = ((y - x) / 2.0).round();
    (tx >= 0.0 && ty >= 0.0 && tx < 256.0 && ty < 256.0).then_some([tx as u16, ty as u16])
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("studio-map-{}-{tag}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn oversized_registry_is_rejected_before_writing_project_files() {
        let root = dir("registry-limit");
        let mut d = Document::starter(&root).unwrap();
        let mut level = d.project.maps.level[0].clone();
        level.object = vec![
            d2_sim::drlg::maps::Object {
                class: 0,
                at: [3, 3],
                mode: 0
            };
            256
        ];
        d.project.maps.level = (1..=255)
            .map(|id| {
                let mut l = level.clone();
                l.id = id;
                l
            })
            .collect();
        assert!(d.save().unwrap_err().to_string().contains("1 MiB"));
        assert!(!root.join("files/levels.toml").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn paintings_placements_undo_and_generated_tiles_roundtrip() {
        let root = dir("roundtrip");
        let _ = fs::remove_file(root.join("files/levels.toml"));
        let mut d = Document::starter(&root).unwrap();
        let path = "modkit/new.ds1";
        d.paint(path, Layer::Wall(0), [4, 5], 0x200001, 1).unwrap();
        let i = 5 * 33 + 4;
        assert_eq!(
            d.project.files.inputs[path.as_bytes()].walls[0][i],
            0x200001
        );
        d.undo().unwrap();
        assert_eq!(d.project.files.inputs[path.as_bytes()].walls[0][i], 0);
        d.redo().unwrap();
        let mut level = d.project.maps.level[0].clone();
        level.object.push(d2_sim::drlg::maps::Object {
            class: 0,
            at: [7, 8],
            mode: 2,
        });
        level.spawn.push(d2_sim::drlg::maps::Spawn {
            monster: 0,
            rect: [10, 10, 3, 3],
            count: 4,
        });
        d.update_level(level).unwrap();
        let p = d.project.preview(1, 42, 1).unwrap();
        assert_eq!(p[0].realized.units.len(), 5);
        assert_eq!(p[0].realized.units[0].mode, 2);
        let original_tiles = d.project.tiles["modkit/generated.dt1"].clone();
        let mut changed = original_tiles.clone();
        changed.tiles[0].subtile_flags[0] = 1;
        changed.tiles[0].blocks[0].pixels[16] = 90;
        d.update_tiles("modkit/generated.dt1", changed.clone())
            .unwrap();
        d.undo().unwrap();
        assert_eq!(d.project.tiles["modkit/generated.dt1"], original_tiles);
        d.redo().unwrap();
        assert_eq!(d.project.tiles["modkit/generated.dt1"], changed);
        d.save().unwrap();
        assert!(!d.dirty);
        let text =
            fs::read_to_string(root.join("files/data/global/tiles/modkit/new.ds1.toml")).unwrap();
        let map = d2_native::toml_kinds::read_ds1("test", &text).unwrap();
        assert_eq!(map, d.project.files.decoded[path.as_bytes()]);
        let maps =
            Maps::parse(&fs::read_to_string(root.join("files/levels.toml")).unwrap()).unwrap();
        assert_eq!(maps, d.project.maps);
        let dt1 = &d.project.tiles["modkit/generated.dt1"];
        let native = dt1
            .write(
                "data/global/tiles/modkit/generated.dt1",
                &d2_native::kind::grey_palette(),
            )
            .unwrap();
        assert!(native.iter().any(|f| f.path.ends_with(".png")));
        for file in native {
            assert_eq!(
                fs::read(root.join("files").join(file.path)).unwrap(),
                file.bytes
            );
        }
        // The generated ISO blocks obey the native codec's diamond mask.
        let files = dt1
            .write("generated.dt1", &d2_native::kind::grey_palette())
            .unwrap();
        let decoded = Dt1::read(
            "generated.dt1",
            &d2_native::kind::MemStore::from_files(&files),
        )
        .unwrap();
        assert_eq!(*dt1, decoded);
        d.undo().unwrap();
        d.undo().unwrap();
        assert!(d.dirty);
        d.save().unwrap();
        assert!(d.project.maps.level[0].object.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn saves_reject_external_edits_and_paths_outside_the_project() {
        let root = dir("conflict");
        let _ = fs::remove_file(root.join("files/levels.toml"));
        let mut d = Document::starter(&root).unwrap();
        d.save().unwrap();
        d.paint("modkit/new.ds1", Layer::Floor(0), [2, 3], 0x300001, 0)
            .unwrap();
        let p = root.join("files/data/global/tiles/modkit/new.ds1.toml");
        fs::write(&p, "external change").unwrap();
        assert!(d
            .save()
            .unwrap_err()
            .to_string()
            .contains("changed on disk"));
        assert_eq!(fs::read_to_string(p).unwrap(), "external change");
        assert!(d.dirty);
        assert!(checked(&root, "../outside").is_err());
        assert!(checked(&root, "/outside").is_err());
        assert!(d.import_map("../../outside.ds1").is_err());
        assert!(d
            .paint("modkit/new.ds1", Layer::Wall(0), [32, 0], 1, 1)
            .is_err());
        assert!(d.add_level(1, 1).is_err());
        assert!(d.add_level(1, 256).is_err());
        let mut malformed = d.project.clone();
        malformed.maps.level[0].size = [16, 16];
        assert!(Document::new(malformed)
            .err()
            .unwrap()
            .to_string()
            .contains("size differs"));
        let mut missing = d.project.clone();
        missing.maps.level[0].tiles = vec!["missing.dt1".into()];
        assert!(Document::new(missing)
            .err()
            .unwrap()
            .to_string()
            .contains("tile library missing.dt1"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn picking_tracks_the_shared_isometric_tile_centres() {
        for x in [0, 1, 5, 255] {
            for y in [0, 1, 8, 255] {
                let (sx, sy) = d2_native::map::layout::cell_origin(x, y);
                assert_eq!(
                    pick_tile((sx + 80) as f32, (sy + 40) as f32),
                    Some([x as u16, y as u16])
                );
            }
        }
        assert_eq!(pick_tile(-10000.0, -10000.0), None);
    }
    #[test]
    fn failed_staging_preserves_all_files_and_cleans_temporary_output() {
        let root = dir("stage");
        fs::write(root.join("a"), "old-a").unwrap();
        fs::write(root.join("b"), "old-b").unwrap();
        let backup = root.join(format!(".studio-backup-{}-1", std::process::id()));
        fs::write(&backup, "recover me").unwrap();
        let writes = BTreeMap::from([
            ("a".into(), b"new-a".to_vec()),
            ("b".into(), b"new-b".to_vec()),
        ]);
        assert!(replace_files(&root, &writes).is_err());
        assert_eq!(fs::read(root.join("a")).unwrap(), b"old-a");
        assert_eq!(fs::read(root.join("b")).unwrap(), b"old-b");
        assert_eq!(fs::read(backup).unwrap(), b"recover me");
        assert!(!fs::read_dir(&root).unwrap().any(|f| {
            f.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".studio-stage-")
        }));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn project_links_cannot_redirect_asset_writes() {
        let root = dir("link");
        let outside = dir("outside");
        std::os::unix::fs::symlink(&outside, root.join("files")).unwrap();
        assert!(Document::starter(&root).is_err());
        assert!(!outside.join("levels.toml").exists());
        fs::remove_file(root.join("files")).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
