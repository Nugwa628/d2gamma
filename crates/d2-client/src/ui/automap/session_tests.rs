//! Tests of the automap session.

use std::path::PathBuf;

use super::options::MemoryStore;
use super::persist::{table_entries, MaFile, TABLE_ENTRIES};
use super::picker::{CelPicker, PickRecord};
use super::session::{open_files_in, unit_pos, AutomapSession, AutomapSource, SaveFiles};
use super::{Automap, AutomapLevels, FrameFacts, UnitCels, SEED};
use crate::bridge::world::{ActLoad, ClientUnit, ClientWorld, UnitKey, MONSTER, OBJECT, PLAYER};
use crate::rules::camera::ClientPos;
use crate::rules::draw_order::{
    Dt1Facts, Fade, LevelFacts, NearRooms, Room, TileRecord, TileRect, REC_DRAWN,
};

struct Levels;

impl AutomapLevels for Levels {
    fn layer(&self, level: u32) -> Option<u32> {
        Some(level)
    }
    fn level_type(&self, _: u32) -> Option<u32> {
        Some(1)
    }
    fn act(&self, _: u32) -> Option<u8> {
        Some(0)
    }
}

/// Level 2 is on layer 249 (a mod's levels table), past the default table.
struct HighLayers;

impl AutomapLevels for HighLayers {
    fn layer(&self, level: u32) -> Option<u32> {
        Some(if level == 2 { 249 } else { level })
    }
    fn level_type(&self, _: u32) -> Option<u32> {
        Some(1)
    }
    fn act(&self, _: u32) -> Option<u8> {
        Some(0)
    }
    fn max_layer(&self) -> Option<u32> {
        Some(249)
    }
}

struct Cels;

impl UnitCels for Cels {
    fn monster_cel(&self, _: u32) -> Option<u32> {
        None
    }
    fn object_cel(&self, _: u32) -> Option<u32> {
        None
    }
}

/// LevelType 1, orientation 1: one record that always gives cel 40.
fn source() -> AutomapSource {
    let mut ranges = vec![(-1, -1); 36];
    ranges[1] = (0, 1);
    AutomapSource {
        picker: CelPicker {
            records: vec![PickRecord {
                level: 1,
                tile: 1,
                style: 0xFF,
                start: 0xFF,
                end: 0,
                cels: [40, -1, -1, -1],
                count: 1,
            }],
            ranges,
        },
        levels: Box::new(Levels),
        cels: Box::new(Cels),
    }
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("d2rust-automap-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn files(dir: &std::path::Path) -> SaveFiles {
    SaveFiles {
        dir: dir.to_path_buf(),
        sub: Some("sub".into()),
        name: "Hero".into(),
    }
}

#[test]
fn map_files_use_the_sub_dir_when_it_opens_else_the_save_dir() {
    let d = temp("sub");
    let f = files(&d);
    let (_, path, slot) = open_files_in(&f, 7, TABLE_ENTRIES).unwrap();
    assert_eq!((path, slot), (d.join("Hero.ma0"), 0));
    assert!(d.join("Hero.map").is_file());
    std::fs::create_dir_all(d.join("sub")).unwrap();
    let (_, path, _) = open_files_in(&f, 7, TABLE_ENTRIES).unwrap();
    assert_eq!(path, d.join("sub").join("Hero.ma0"));
    assert!(d.join("sub").join("Hero.map").is_file());
    // No sub dir named: the save dir.
    let f = SaveFiles { sub: None, ..f };
    let (_, path, _) = open_files_in(&f, 7, TABLE_ENTRIES).unwrap();
    assert_eq!(path, d.join("Hero.ma0"));
    let _ = std::fs::remove_dir_all(&d);
}

fn world(cell: (u16, u16)) -> ClientWorld {
    let mut w = ClientWorld::default();
    let p = UnitKey::new(PLAYER, 1);
    let mut u = ClientUnit::new(p);
    u.position = Some(cell);
    w.units.insert(p, u);
    w.local_player = Some(p);
    w.act = Some(ActLoad {
        act: 0,
        init_seed: 0x1234,
        town_level: 1,
        f8: 9,
    });
    w
}

fn near() -> NearRooms {
    let rec = TileRecord {
        tile: (0, 0),
        flags: REC_DRAWN,
        ty: 0,
        dt1: Dt1Facts {
            orientation: 1,
            sub: 5,
            ..Dt1Facts::default()
        },
        fade: Fade::OPAQUE,
        logical: None,
    };
    NearRooms {
        rooms: vec![Room {
            level: 2,
            tiles: TileRect {
                x: 10,
                y: 0,
                w: 8,
                h: 8,
            },
            floors: vec![rec],
            ..Room::default()
        }],
        player_tile: (0, 0),
        player_logical: 0,
        player_subtile: (0, 0),
        edge: None,
        level: LevelFacts {
            id: 2,
            ..LevelFacts::default()
        },
    }
}

#[test]
fn a_frame_opens_the_act_files_reveals_and_the_teardown_writes() {
    let d = temp("frame");
    let map = Automap::new(&MemoryStore::default(), SEED);
    let mut s = AutomapSession::new(map, source(), Some(files(&d)));
    // No act: nothing.
    let mut w = world((10, 10));
    let act = w.act.take();
    s.frame(&w, None).unwrap();
    assert!(s.data_path().is_none());
    w.act = act;
    let mut n = near();
    s.frame(&w, Some(&mut n)).unwrap();
    assert_eq!(s.data_path(), Some(d.join("Hero.ma0").as_path()));
    assert_eq!(s.map.current(), Some(2));
    assert_eq!(s.map.cells.tree(super::TreeKind::Floor).in_order().len(), 1);
    // The same act record does not reopen; the player did not move: no
    // second reveal.
    s.frame(&w, Some(&mut near())).unwrap();
    assert_eq!(s.map.current(), Some(2));
    s.teardown().unwrap();
    assert_eq!(s.map.current(), None);
    let bytes = std::fs::read(d.join("Hero.ma0")).unwrap();
    assert!(bytes.len() > 400, "table plus one record");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn files_that_cannot_open_keep_the_automap_in_memory() {
    // A file stands where the save directory goes: the index cannot be
    // written.
    let root = temp("missing");
    let blocker = root.join("in-the-way");
    std::fs::write(&blocker, b"x").unwrap();
    let d = blocker.join("gone");
    let map = Automap::new(&MemoryStore::default(), SEED);
    let mut s = AutomapSession::new(map, source(), Some(files(&d)));
    let w = world((10, 10));
    let mut n = near();
    s.frame(&w, Some(&mut n)).unwrap();
    assert!(s.data_path().is_none());
    assert_eq!(s.map.current(), Some(2));
    assert_eq!(s.map.cells.tree(super::TreeKind::Floor).in_order().len(), 1);
    // A new act record tears the memory-only act down without a file.
    let mut w2 = world((10, 10));
    if let Some(a) = w2.act.as_mut() {
        a.init_seed ^= 1;
    }
    s.frame(&w2, Some(&mut near())).unwrap();
    s.teardown().unwrap();
    assert!(!d.exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn toggle_flips_ui_state_0x0a() {
    let map = Automap::new(&MemoryStore::default(), SEED);
    let mut s = AutomapSession::new(map, source(), None);
    let f = FrameFacts {
        width: 800,
        height: 600,
        open_mode: 0,
        mini_down: false,
        unit_origin: ClientPos::default(),
    };
    s.toggle(&f);
    assert!(s.open);
    s.toggle(&f);
    assert!(!s.open);
}

/// An object's cell comes from its static path point, a monster's
/// from its cell centre, 8 pixels lower. The
/// Act IV stash is drawn one row
/// above the moving projection.
#[test]
fn unit_cell_static_and_moving_paths() {
    let o = unit_pos(OBJECT, (10, 11));
    assert_eq!((o.x, o.y), (-16, 168));
    let m = unit_pos(MONSTER, (10, 11));
    assert_eq!((m.x, m.y), (-16, 176));
    // y = py / 10 − 3: row 13 against row 14
    assert_eq!((o.y / 10 - 3, m.y / 10 - 3), (13, 14));
}

/// A DRLG callback's switch after the act load reads the layer's stored
/// cells (saved flag 1), so a reload writes nothing new: the `.ma0` keeps
/// its size (5022
/// bytes after both passes).
#[test]
fn a_reload_switch_after_the_act_load_writes_no_record_again() {
    let d = temp("reload");
    let w = world((10, 10));
    let map = Automap::new(&MemoryStore::default(), SEED);
    let mut s = AutomapSession::new(map, source(), Some(files(&d)));
    s.frame(&w, Some(&mut near())).unwrap();
    s.teardown().unwrap();
    let first = std::fs::read(d.join("Hero.ma0")).unwrap();
    let map = Automap::new(&MemoryStore::default(), SEED);
    let mut s = AutomapSession::new(map, source(), Some(files(&d)));
    s.map.limits = super::CelLimits {
        maxi: 64,
        town: [64; 4],
    };
    // The present order: the act first, then the callbacks' switch.
    s.act(&w).unwrap();
    s.map.switch(2).unwrap();
    assert_eq!(s.map.cells.tree(super::TreeKind::Floor).in_order().len(), 1);
    s.frame(&w, Some(&mut near())).unwrap();
    s.teardown().unwrap();
    let second = std::fs::read(d.join("Hero.ma0")).unwrap();
    assert_eq!(second.len(), first.len());
    let _ = std::fs::remove_dir_all(&d);
}

/// A unit placed since the last frame (any unit) holds the reveal for the next
/// two frames (ui-draws-automap-persist-ama: the warp's placement must not
/// reveal the Blood Moor's first room the same frame, or the town record saved
/// at the switch gains its unit cels).
#[test]
fn a_placement_holds_the_reveal_two_frames() {
    let map = Automap::new(&MemoryStore::default(), SEED);
    let mut s = AutomapSession::new(map, source(), None);
    let mut w = world((10, 10));
    w.placed_units = 1;
    let floor = |s: &AutomapSession| s.map.cells.tree(super::TreeKind::Floor).in_order().len();
    s.frame(&w, Some(&mut near())).unwrap();
    assert_eq!(floor(&s), 0);
    s.frame(&w, Some(&mut near())).unwrap();
    assert_eq!(floor(&s), 0);
    s.frame(&w, Some(&mut near())).unwrap();
    assert_eq!(floor(&s), 1);
}

#[test]
fn a_first_start_with_no_save_folder_makes_it_for_the_map_files() {
    let d = temp("fresh").join("saves");
    assert!(!d.exists());
    let f = SaveFiles {
        dir: d.clone(),
        sub: None,
        name: "Hero".into(),
    };
    let (file, path, _) = open_files_in(&f, 7, TABLE_ENTRIES).unwrap();
    assert!(d.join("Hero.map").is_file());
    assert_eq!(path, d.join("Hero.ma0"));
    // The teardown's write into a folder that vanished makes it again.
    std::fs::remove_dir_all(&d).unwrap();
    file.write_to(&path).unwrap();
    assert!(path.is_file());
    let _ = std::fs::remove_dir_all(d.parent().unwrap());
}

/// A layer id past the default 100-entry table (a mod's levels table) is
/// stored and restored: the table is sized from the level data, and the
/// vanilla size stays the floor.
#[test]
fn a_layer_past_the_default_table_is_saved_and_restored() {
    assert_eq!(table_entries(None), TABLE_ENTRIES);
    assert_eq!(table_entries(Some(99)), TABLE_ENTRIES);
    assert_eq!(table_entries(Some(100)), 101);
    assert_eq!(table_entries(Some(249)), 250);

    let d = temp("high-layer");
    let w = world((10, 10));
    let high = || AutomapSource {
        levels: Box::new(HighLayers),
        ..source()
    };
    let map = Automap::new(&MemoryStore::default(), SEED);
    let mut s = AutomapSession::new(map, high(), Some(files(&d)));
    s.frame(&w, Some(&mut near())).unwrap();
    let live = s.map.cells.tree(super::TreeKind::Floor).in_order().len();
    assert_eq!(live, 1);
    s.teardown().unwrap();
    let saved = std::fs::read(d.join("Hero.ma0")).unwrap();
    assert!(saved.len() > 250 * 4, "a 250-entry table plus the record");
    // The layer-249 table entry points at the record.
    let at = u32::from_le_bytes(saved[996..1000].try_into().unwrap());
    assert!(at as usize >= 250 * 4 && (at as usize) < saved.len());

    // A new game reads the cells back instead of starting empty.
    let map = Automap::new(&MemoryStore::default(), SEED);
    let mut s = AutomapSession::new(map, high(), Some(files(&d)));
    s.map.limits = super::CelLimits {
        maxi: 64,
        town: [64; 4],
    };
    s.act(&w).unwrap();
    s.map.switch(249).unwrap();
    assert_eq!(s.map.cells.tree(super::TreeKind::Floor).in_order().len(), 1);
    let _ = std::fs::remove_dir_all(&d);
}

/// Without level data that reaches past the default table, such a layer is
/// still refused (the file keeps the vanilla shape).
#[test]
fn a_layer_past_the_table_is_still_refused_in_a_default_file() {
    let mut f = MaFile::default();
    let rec = super::MaRecord {
        layer: 249,
        ..Default::default()
    };
    assert!(f.append(&rec).is_err());
    let mut big = MaFile::with_entries(250);
    assert!(big.append(&rec).is_ok());
    assert_eq!(big.entries(), 250);
    assert!(MaFile::from_bytes_with(vec![0; 40], 250).is_err());
}
