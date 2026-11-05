//! Seed previews use the simulation's map realization and the runtime native layer reader.
pub mod editor;
use anyhow::{bail, Context, Result};
use d2_formats::{ds1::Ds1, dt1::Dt1};
use d2_native::source::{NativeAsset, NativeSource};
use d2_sim::drlg::maps::{Maps, Realized};
use d2_sim::drlg::preset::{Ds1Input, Ds1Source};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Default)]
pub struct MapFiles {
    pub decoded: BTreeMap<Vec<u8>, Ds1>,
    pub inputs: BTreeMap<Vec<u8>, Ds1Input>,
}
impl Ds1Source for MapFiles {
    fn ds1(&self, p: &[u8]) -> Option<&Ds1Input> {
        self.inputs.get(p)
    }
}
#[derive(Clone, Debug)]
pub struct MapProject {
    pub root: PathBuf,
    pub maps: Maps,
    pub files: MapFiles,
    pub tiles: BTreeMap<String, Dt1>,
}
#[derive(Clone, Debug)]
pub struct SeedPreview {
    pub seed: u32,
    pub level: u32,
    pub realized: Realized,
}
impl MapProject {
    pub fn load(project: &Path, native: &Path, installed: &Path) -> Result<Self> {
        let today = d2_modkit::api::Date::today();
        let path = project.canonicalize()?;
        let picked = d2_native::package::open_mod(&path, today)?;
        if picked.manifest.screen_only {
            bail!("Screen-only add-ons cannot edit maps");
        }
        let resolve = |spec: &d2_modkit::ModSpec| {
            d2_modkit::resolve::resolve_with(spec, installed, today, d2_native::package::open_mod)
        };
        let stack = if picked.manifest.kind == d2_modkit::Kind::Full {
            resolve(&d2_modkit::ModSpec::Dir(path))?
        } else {
            let target = picked
                .manifest
                .attaches_to
                .as_ref()
                .context("Add-on needs a target mod")?;
            let mut stack = resolve(&d2_modkit::ModSpec::Id(target.clone()))?;
            picked
                .manifest
                .check_addon_for(Some(&stack.picked().manifest))
                .map_err(anyhow::Error::msg)?;
            stack.mods.push(picked);
            stack
        };
        let layers = d2_native::layers::LayerStack::with_mods(native, &stack.layer_entries())?;
        let mut source = NativeSource::open_with(native, Some(&[]))?;
        source.layers = layers;
        let layer = source
            .layers
            .layers
            .iter()
            .skip(1)
            .rev()
            .find(|l| l.has("levels.toml"))
            .context("No files/levels.toml in the mod stack")?;
        let bytes = layer
            .read("levels.toml")
            .context("levels.toml disappeared")??;
        let maps = Maps::parse(std::str::from_utf8(&bytes)?).map_err(anyhow::Error::msg)?;
        let mut files = MapFiles::default();
        let mut tiles = BTreeMap::new();
        for l in &maps.level {
            for p in l.files.iter().chain(l.preset.iter().map(|s| &s.path)) {
                if files.inputs.contains_key(p.as_bytes()) {
                    continue;
                }
                let path = format!("data/global/tiles/{p}");
                let NativeAsset::Ds1(d) = source
                    .read_native(&path)
                    .with_context(|| format!("{path}: missing"))?
                    .map_err(anyhow::Error::msg)?
                else {
                    bail!("{path}: wrong asset kind");
                };
                if d.version < 7 {
                    bail!("{path}: version below 7");
                }
                files
                    .inputs
                    .insert(p.as_bytes().to_vec(), Ds1Input::from(&d));
                files.decoded.insert(p.as_bytes().to_vec(), d);
            }
            for p in &l.tiles {
                if tiles.contains_key(p) {
                    continue;
                }
                let path = format!("data/global/tiles/{p}");
                let NativeAsset::Dt1(d) = source
                    .read_native(&path)
                    .with_context(|| format!("{path}: missing"))?
                    .map_err(anyhow::Error::msg)?
                else {
                    bail!("{path}: wrong asset kind");
                };
                tiles.insert(p.clone(), d);
            }
        }
        Ok(Self {
            root: project.into(),
            maps,
            files,
            tiles,
        })
    }
    pub fn preview(&self, level: u32, seed: u32, count: usize) -> Result<Vec<SeedPreview>> {
        if !(1..=64).contains(&count) {
            bail!("Preview count must be 1..64");
        }
        (0..count)
            .map(|i| {
                let seed = seed.wrapping_add(i as u32);
                Ok(SeedPreview {
                    seed,
                    level,
                    realized: self
                        .maps
                        .preview(level, seed, &self.files)
                        .map_err(anyhow::Error::msg)?,
                })
            })
            .collect()
    }
    /// Keep source metadata; only realized grid/placements replace the source fields.
    pub fn decoded_preview(&self, p: &SeedPreview) -> Result<Ds1> {
        use d2_sim::rng::Seed;
        let l = self
            .maps
            .level
            .iter()
            .find(|l| l.id == p.level)
            .context("Level disappeared")?;
        let start = Seed::init_low(p.seed).step();
        let mut rng = Seed::init_low(start.wrapping_add(p.level));
        let path = &l.files[rng.roll(l.files.len() as i32) as usize];
        let mut ds1 = self
            .files
            .decoded
            .get(path.as_bytes())
            .context("Source map missing")?
            .clone();
        ds1.files = l
            .tiles
            .iter()
            .map(|p| {
                format!("data/global/tiles/{p}")
                    .replace('/', "\\")
                    .into_bytes()
            })
            .collect();
        let m = &p.realized.map;
        ds1.width = m.width + 1;
        ds1.height = m.height + 1;
        ds1.version = m.version;
        ds1.act = m.act as u32;
        ds1.tag_type = m.tag_type;
        ds1.walls = m.walls.clone();
        ds1.orientations = m.orientations.clone();
        ds1.floors = m.floors.clone();
        ds1.shadow = m.shadow.clone();
        ds1.objects = m
            .objects
            .iter()
            .map(|o| d2_formats::ds1::Ds1Object {
                kind: o.kind,
                id: o.id,
                x: o.x,
                y: o.y,
                flags: o.flags,
            })
            .collect();
        ds1.paths = m
            .paths
            .iter()
            .map(|p| d2_formats::ds1::Ds1Path {
                x: p.x,
                y: p.y,
                points: p
                    .points
                    .iter()
                    .map(|&(x, y, action)| d2_formats::ds1::Ds1PathPoint { x, y, action })
                    .collect(),
            })
            .collect();
        Ok(ds1)
    }
    pub fn export(&self, level: u32, seed: u32, count: usize, out: &Path) -> Result<()> {
        let previews = self.preview(level, seed, count)?;
        fs::create_dir_all(out)?;
        let mut rows = Vec::new();
        for p in &previews {
            let filename = format!("level-{level}-seed-{}.ds1.toml", p.seed);
            let ds1 = self.decoded_preview(p)?;
            fs::write(
                out.join(&filename),
                d2_native::toml_kinds::write_ds1(&filename, &ds1)?,
            )?;
            rows.push(serde_json::json!({"level":level,"seed":p.seed,"file":filename,"units":p.realized.units.iter().map(|u|serde_json::json!({"kind":u.unit_type,"class":u.class,"x":u.x,"y":u.y,"mode":u.mode})).collect::<Vec<_>>()}));
        }
        fs::write(
            out.join("seeds.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"version":1,"seeds":rows}))?,
        )?;
        Ok(())
    }
}
/// Headless authoring command, dispatched before opening a GUI/window.
pub fn seed_preview(args: &[std::ffi::OsString]) -> Result<()> {
    if args.len() != 7 {
        bail!(
            "Usage: Mod-Studio seed-preview <project> <native> <installed-mods> <level> <seed> <count> <output>"
        );
    }
    let number = |i: usize| {
        args[i]
            .to_str()
            .context("Expected UTF-8 integer")?
            .parse::<u32>()
            .context("Expected an unsigned integer")
    };
    let project = MapProject::load(
        Path::new(&args[0]),
        Path::new(&args[1]),
        Path::new(&args[2]),
    )?;
    project.export(
        number(3)?,
        number(4)?,
        number(5)? as usize,
        Path::new(&args[6]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> MapProject {
        let maps=Maps::parse("version=2\n[[level]]\nid=1\nsize=[32,32]\nentry=[2,2]\nfiles=['test.ds1']\ntiles=['test.dt1']\n[level.generation]\nkind='maze'\ncell=3\nfloor=1048577\nwall=2097153\norientation=1\n").unwrap();
        let d = Ds1 {
            version: 18,
            width: 33,
            height: 33,
            act: 0,
            tag_type: 0,
            files: vec![],
            unknown_header: None,
            walls: vec![vec![0; 1089]],
            orientations: vec![vec![0; 1089]],
            floors: vec![vec![0x100001; 1089]],
            shadow: vec![0; 1089],
            tags: None,
            objects: vec![],
            unknown_groups: Some([0; 4]),
            groups: vec![],
            groups_truncated: false,
            paths: vec![],
            trailing: vec![],
        };
        MapProject {
            root: PathBuf::new(),
            maps,
            files: MapFiles {
                inputs: BTreeMap::from([(b"test.ds1".to_vec(), Ds1Input::from(&d))]),
                decoded: BTreeMap::from([(b"test.ds1".to_vec(), d)]),
            },
            tiles: BTreeMap::new(),
        }
    }
    #[test]
    fn seed_tool_exports_reproducible_native_maps_and_seed_metadata() {
        let project = project();
        let previews = project.preview(1, u32::MAX - 15, 32).unwrap();
        assert_eq!(previews[16].seed, 0);
        let layouts: std::collections::BTreeSet<_> = previews
            .iter()
            .map(|p| p.realized.map.walls.clone())
            .collect();
        assert!(layouts.len() > 24);
        for p in &previews {
            assert_eq!(
                p.realized.map,
                project.maps.preview(1, p.seed, &project.files).unwrap().map
            );
        }
        let dir = std::env::temp_dir().join(format!("mod-studio-seeds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        project.export(1, 42, 32, &dir).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("seeds.json")).unwrap()).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["seeds"].as_array().unwrap().len(), 32);
        let text = std::fs::read_to_string(dir.join("level-1-seed-42.ds1.toml")).unwrap();
        let decoded = d2_native::toml_kinds::read_ds1("preview", &text).unwrap();
        assert_eq!(
            Ds1Input::from(&decoded),
            project.preview(1, 42, 1).unwrap()[0].realized.map
        );
        assert!(project.preview(1, 0, 0).is_err());
        assert!(project.preview(1, 0, 65).is_err());
        assert!(project.preview(999, 0, 1).is_err());
        assert!(seed_preview(&[])
            .unwrap_err()
            .to_string()
            .contains("Usage:"));
    }
    #[test]
    fn seed_tool_loads_folders_signed_packs_and_approved_inherited_maps() {
        use d2_native::kind::NativeKind;
        let root = std::env::temp_dir().join(format!("studio-map-load-{}", std::process::id()));
        let native = root.join("native");
        let installed = root.join("installed");
        let folder = root.join("source");
        fs::create_dir_all(native.join("base")).unwrap();
        fs::create_dir_all(&installed).unwrap();
        fs::create_dir_all(&folder).unwrap();
        let tsv = d2_native::manifest::write_files_tsv(&[]);
        let manifest = d2_native::manifest::Manifest {
            converter_version: d2_native::manifest::MIN_CONVERTER_VERSION.into(),
            converter_commit: "generated-test".into(),
            kinds: BTreeMap::new(),
            language: "ENG".into(),
            lod: true,
            archives: vec![],
            counts: BTreeMap::from([("ds1".into(), d2_native::manifest::KindCounts::default())]),
            unnamed_blocks: BTreeMap::new(),
            complete: true,
            files_sha256: d2_native::source::sha256_hex(tsv.as_bytes()),
        };
        fs::write(native.join("manifest.toml"), manifest.to_toml()).unwrap();
        fs::write(native.join("files.tsv"), tsv).unwrap();
        let mut identity = crate::default_manifest();
        identity.id = "map-base".into();
        identity.allow_derived = true;
        identity.approved_addons = vec!["map-addon".into()];
        fs::write(folder.join("mod.toml"), identity.to_toml()).unwrap();
        let project = project();
        fs::create_dir_all(folder.join("files/data/global/tiles")).unwrap();
        fs::write(
            folder.join("files/levels.toml"),
            project.maps.to_toml().unwrap(),
        )
        .unwrap();
        let ds1 = &project.files.decoded[b"test.ds1".as_slice()];
        fs::write(
            folder.join("files/data/global/tiles/test.ds1.toml"),
            d2_native::toml_kinds::write_ds1("test", ds1).unwrap(),
        )
        .unwrap();
        let tile = d2_formats::dt1::Dt1Tile {
            light_direction: 0,
            roof_height: 0,
            material_flags: 0,
            height: 32,
            width: 32,
            unknown_height: 0,
            orientation: 0,
            main_index: 1,
            sub_index: 0,
            rarity: 1,
            unknown_color: 0,
            subtile_flags: [0; 25],
            unknown_58: 0,
            cache_index: 0,
            unknown_5c: 0,
            blocks: vec![d2_formats::dt1::Dt1Block {
                x: 0,
                y: 0,
                unknown1: 0,
                grid_x: 0,
                grid_y: 0,
                format: 0x2005,
                unknown2: 0,
                pixels: vec![72; 1024],
            }],
        };
        let tiles = Dt1 {
            version: 7,
            minor_version: 6,
            tiles: vec![tile],
        };
        for f in tiles
            .write(
                "data/global/tiles/test.dt1",
                &d2_native::kind::grey_palette(),
            )
            .unwrap()
        {
            let p = folder.join("files").join(f.path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, f.bytes).unwrap();
        }
        let loaded = MapProject::load(&folder, &native, &installed).unwrap();
        assert_eq!(loaded.maps, project.maps);
        assert_eq!(loaded.files.decoded, project.files.decoded);
        assert_eq!(loaded.tiles["test.dt1"], tiles);
        let mut snapshot =
            d2_modkit::MemoryPack::from_folder(&d2_modkit::ModFolder::scan(&folder).unwrap())
                .unwrap();
        let key = d2_modkit::publish::author_key(&root.join("key")).unwrap();
        d2_modkit::publish::seal(&mut snapshot, &key).unwrap();
        fs::write(
            folder.join(d2_modkit::publish::RECORD),
            &snapshot.files[d2_modkit::publish::RECORD],
        )
        .unwrap();
        let pack = installed.join("map-base.d2pack");
        d2_native::pack::build(&folder, &pack, 1).unwrap();
        let packed = MapProject::load(&pack, &native, &installed).unwrap();
        assert_eq!(packed.maps, loaded.maps);
        assert_eq!(packed.files.decoded, loaded.files.decoded);
        assert_eq!(packed.tiles, loaded.tiles);
        let derived = root.join("derived");
        fs::create_dir_all(derived.join("files")).unwrap();
        let mut child = crate::default_manifest();
        child.id = "map-child".into();
        crate::select_base(&mut child, Some(&identity)).unwrap();
        fs::write(derived.join("mod.toml"), child.to_toml()).unwrap();
        let inherited = MapProject::load(&derived, &native, &installed).unwrap();
        assert_eq!(inherited.maps, project.maps);
        assert_eq!(inherited.tiles, loaded.tiles);
        child.id = "map-addon".into();
        child.kind = d2_modkit::Kind::Addon;
        crate::select_base(&mut child, Some(&identity)).unwrap();
        fs::write(derived.join("mod.toml"), child.to_toml()).unwrap();
        assert_eq!(
            MapProject::load(&derived, &native, &installed)
                .unwrap()
                .maps,
            project.maps
        );
        child.id = "unapproved-map-addon".into();
        fs::write(derived.join("mod.toml"), child.to_toml()).unwrap();
        assert!(MapProject::load(&derived, &native, &installed)
            .unwrap_err()
            .to_string()
            .contains("has not approved"));
        child.screen_only = true;
        child.attaches_to = Some("d2gamma".into());
        fs::write(derived.join("mod.toml"), child.to_toml()).unwrap();
        assert!(MapProject::load(&derived, &native, &installed)
            .unwrap_err()
            .to_string()
            .contains("Screen-only"));
        let out = root.join("preview");
        seed_preview(&[
            folder.into_os_string(),
            native.into_os_string(),
            installed.into_os_string(),
            "1".into(),
            "42".into(),
            "2".into(),
            out.clone().into_os_string(),
        ])
        .unwrap();
        assert!(out.join("level-1-seed-42.ds1.toml").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
