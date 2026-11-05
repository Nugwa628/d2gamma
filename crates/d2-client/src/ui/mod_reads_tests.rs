//! A mod game's sheet panel reads the combat values and skill points by
//! the game's rules and draws the game's pop-ups; D2Gamma's character
//! panel shows the same values.

use std::cell::RefCell;
use std::sync::Arc;

use super::super::character_bind_tests::tbl;
use super::super::{FontMeasure, ModelCharacter, OriginalUi, UiConfig, CHARACTER_FONTS};
use super::*;
use crate::assets::path::MemorySource;
use crate::bridge::world::{ClientUnit, ItemData, ItemRecord, KindData, PlayerData, MONSTER};
use crate::mods::{ClientMods, PanelFactory, PanelPlace, MOD_PANEL_IDS};
use crate::ui::char_desc::DescTables;
use crate::ui::char_feed::CharTables;
use crate::ui::draw::UiDraw;
use crate::ui::geom::Rect;
use crate::ui::layout::Screen;
use crate::ui::mod_io::{request_intent, ModRequest};
use crate::ui::panel::{ClientIntent, Panel, PanelId, UiEvent, UiResponse, WidgetId};
use crate::ui::panels::UiFiles;
use crate::ui::root::UiRoot;
use crate::ui::{NoPanelRules, NoStrings};
use d2_data::tables::{Missiles, Record, Skilldesc, Skills};
use d2_sim::skills::SkillTables;

const ME: UnitKey = UnitKey {
    unit_type: PLAYER,
    guid: 1,
};
const MERC: UnitKey = UnitKey {
    unit_type: MONSTER,
    guid: 7,
};
const CAP: UnitKey = UnitKey {
    unit_type: crate::bridge::items::ITEM,
    guid: 9,
};
const FIRE: (u16, u16) = (39, 40);
const COLD: (u16, u16) = (43, 44);

/// What the sheet read on its last draw.
#[derive(Clone, Debug, Default, PartialEq)]
struct Seen {
    defense: Option<i32>,
    merc_defense: Option<i32>,
    fire: Option<Resistance>,
    cold: Option<Resistance>,
    attack_rating: Option<i32>,
    hit: Option<i32>,
    no_levels_hit: Option<i32>,
    points: Vec<Option<SkillPoint>>,
    tip: bool,
    desc: bool,
}

thread_local! {
    static SEEN: RefCell<Seen> = RefCell::new(Seen::default());
}

/// A mod game's character sheet: reads the values, draws the item's tip,
/// a skill's description and a hover text.
#[derive(Clone)]
struct Sheet;

impl Panel for Sheet {
    fn id(&self) -> PanelId {
        PanelId(MOD_PANEL_IDS)
    }
    fn rect(&self) -> Rect {
        Rect::new(0, 0, 0, 0)
    }
    fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        let hover = skill_hover::SkillHover {
            skill: 0,
            level: 0,
            at: Point::new(100, 200),
            above: false,
            learnable: true,
        };
        let seen = Seen {
            defense: ctx.defense(ME),
            merc_defense: ctx.defense(MERC),
            fire: ctx.resistance(ME, FIRE.0, FIRE.1),
            cold: ctx.resistance(ME, COLD.0, COLD.1),
            attack_rating: ctx.attack_rating(ME),
            hit: ctx.chance_to_hit(ME, 265, 10),
            no_levels_hit: ctx.chance_to_hit(MERC, 0, 0),
            points: (0..3).map(|s| ctx.skill_point(s)).collect(),
            tip: ctx.draw_item_tip(CAP, Point::new(400, 300), out),
            desc: ctx.draw_skill_desc(&hover, out),
        };
        ctx.draw_hover_text(
            "Sheet".encode_utf16().collect(),
            Point::new(50, 60),
            4,
            false,
            out,
        );
        SEEN.with(|s| *s.borrow_mut() = seen);
    }
    fn hit(&self, _p: Point) -> Option<WidgetId> {
        None
    }
    fn event(&mut self, _e: UiEvent, _ctx: &UiCtx) -> UiResponse {
        UiResponse::Ignored
    }
}

impl PanelFactory for Sheet {
    fn make(&self, _files: &UiFiles) -> Box<dyn Panel> {
        Box::new(Sheet)
    }
}

fn fonts() -> FontMeasure {
    let mut src = MemorySource::default();
    for id in CHARACTER_FONTS {
        src.insert(crate::ui::font_info(id).unwrap().tbl_path, tbl(6));
    }
    FontMeasure::load(&src, &CHARACTER_FONTS).unwrap()
}

fn blank<T: Record>() -> T {
    T::decode(&vec![0u8; T::SIZE])
}

/// Skills 0 (a `skpoints` formula of 2, level 5), 1 (needs skill 0) and 2
/// (strength 50); no formulas otherwise.
fn skill_tables() -> SkillTables {
    let skill = |skpoints: u32, reqlevel: u16, reqskill1: u16, reqstr: u16| {
        let mut s: Skills = blank();
        s.skpoints = skpoints;
        (s.reqlevel, s.maxlvl, s.ingame, s.reqstr) = (reqlevel, 20, true, reqstr);
        (s.reqskill1, s.reqskill2, s.reqskill3) = (reqskill1, 0xFFFF, 0xFFFF);
        s.skilldesc = 0xFFFF;
        s
    };
    SkillTables {
        skills: vec![
            skill(0, 5, 0xFFFF, 0),
            skill(0xFFFF_FFFF, 1, 0, 0),
            skill(0xFFFF_FFFF, 1, 0xFFFF, 50),
        ],
        skilldesc: vec![blank::<Skilldesc>()],
        missiles: vec![blank::<Missiles>()],
        // `skpoints` at offset 0: the constant 2.
        skills_code: vec![0x07, 2, 0x00],
        miss_code: Vec::new(),
        level_cap: 99,
        stat_count: 359,
        data_rules: d2_data::bin::DataRules::Original,
    }
}

/// A compact, identified record of a cap, which the tip decodes.
fn cap_stream() -> Vec<u8> {
    use crate::bridge::items::mode;
    use d2_proto::item_bits::hflag;
    let bits: Vec<(u32, u32)> = vec![
        (hflag::COMPACT | hflag::IDENTIFIED, 32),
        (0x65, 10),
        (u32::from(mode::STORED), 3),
        (0, 4),
        (0, 4),
        (0, 4),
        (5, 3),
        (u32::from_le_bytes(*b"cap "), 32),
    ];
    let (mut out, mut acc, mut n) = (Vec::new(), 0u64, 0u32);
    for (v, w) in bits {
        acc |= u64::from(v) << n;
        n += w;
        while n >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            n -= 8;
        }
    }
    if n > 0 {
        out.push(acc as u8);
    }
    out
}

/// A standing level 10 player (to-hit 100, dexterity 40, armor class 100
/// +50 %, fire 80 / max +5, cold −20, 3 unspent points), a hireling with
/// armor class 50, and a cap; hell's predecessor difficulty (penalty 40).
fn world() -> ClientWorld {
    let mut w = ClientWorld::default();
    let mut p = ClientUnit::new(ME);
    p.mode = 1;
    p.kind = KindData::Player(PlayerData::default());
    for (s, v) in [
        (31, 100),
        (2, 40),
        (171, 50),
        (19, 100),
        (119, 20),
        (12, 10),
        (5, 3),
        (FIRE.0, 80),
        (FIRE.1, 5),
        (COLD.0, -20),
    ] {
        p.stats.insert(s, v);
    }
    w.units.insert(ME, p);
    let mut m = ClientUnit::new(MERC);
    m.mode = 1;
    m.stats.insert(31, 50);
    w.units.insert(MERC, m);
    let mut cap = ClientUnit::new(CAP);
    cap.kind = KindData::Item(ItemData {
        last: Some(ItemRecord {
            id: 0x9C,
            action: 0x04,
            category: 0,
            owner: None,
            seq: 0,
            stream: cap_stream(),
        }),
        ..ItemData::default()
    });
    w.units.insert(CAP, cap);
    w.local_player = Some(ME);
    w.expansion = 1;
    w.difficulty = 1;
    w
}

/// The client with `mods`, its tables loaded or not.
fn ui(mods: ClientMods, tables: bool) -> (OriginalUi, UiRoot) {
    let mut ui = OriginalUi::new(
        UiConfig {
            screen: Screen::R800,
            expansion_installed: true,
        },
        None,
    )
    .unwrap();
    ui.set_mods(mods);
    if tables {
        ui.set_fonts(fonts());
        ui.set_resist_penalties(vec![0, -40, -100]);
        ui.set_char_tables(CharTables {
            tohit_factor: vec![15],
            desc: DescTables {
                skills: Some(Arc::new(skill_tables())),
                ..DescTables::default()
            },
            ..CharTables::default()
        });
        ui.set_item_tips(crate::ui::item_tip::tests::tips());
    }
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    ui.install(&mut root).unwrap();
    (ui, root)
}

fn draw(root: &UiRoot, w: &ClientWorld) -> Vec<UiDraw> {
    let ctx = UiCtx {
        tick: 2,
        world: w,
        strings: &NoStrings,
    };
    let mut out: Vec<UiDraw> = Vec::new();
    root.draw(&ctx, &mut out);
    out
}

fn texts(out: &[UiDraw]) -> Vec<String> {
    out.iter()
        .filter_map(|d| match d {
            UiDraw::Text(t) => Some(String::from_utf16_lossy(&t.text)),
            _ => None,
        })
        .collect()
}

fn sheet() -> ClientMods {
    ClientMods::new().with_panel(PanelPlace::Add { open: true }, Sheet)
}

/// The sheet reads the player's and the hireling's combat values by the
/// character panel's rules and the game's hit rule.
#[test]
fn a_sheet_reads_combat_values_by_the_games_rules() {
    let (_ui, root) = ui(sheet(), true);
    let w = world();
    draw(&root, &w);
    let seen = SEEN.with(|s| s.borrow().clone());
    // 100 + 40 / 4 = 110, +50 %.
    assert_eq!(seen.defense, Some(165));
    assert_eq!(seen.merc_defense, Some(50));
    // 80 − 40, under the cap 75 + 5.
    assert_eq!(
        seen.fire,
        Some(Resistance {
            value: 40,
            cap: 80,
            shown: 40,
            color: 0,
        })
    );
    // −20 − 40: red.
    assert_eq!(
        seen.cold,
        Some(Resistance {
            value: -60,
            cap: 75,
            shown: -60,
            color: 1,
        })
    );
    // 100 + 5 · (40 − 7) + 15 = 280, +20 %.
    assert_eq!(seen.attack_rating, Some(336));
    let hit = hit_chance(HitTerms {
        ar: 280,
        pct_ar: 20,
        def: 265,
        alvl: 10,
        dlvl: 10,
    });
    assert_eq!(seen.hit, Some(hit));
    assert!((5..=95).contains(&hit));
    assert_eq!(seen.no_levels_hit, None);
}

/// The sheet reads each skill's next point: its formula's cost, the
/// level, skill and attribute requirements, and whether it is learnable.
#[test]
fn a_sheet_reads_skill_point_costs_and_requirements() {
    let (_ui, root) = ui(sheet(), true);
    let mut w = world();
    draw(&root, &w);
    let points = SEEN.with(|s| s.borrow().points.clone());
    let point = |level, cost, free, reqs_met, attrs_met| {
        Some(SkillPoint {
            level,
            max: 20,
            cost,
            free,
            reqs_met,
            attrs_met,
        })
    };
    assert_eq!(
        points,
        vec![
            point(0, 2, 3, true, true),
            // Skill 0 is not learned.
            point(0, 1, 3, false, true),
            // Strength 0 of 50.
            point(0, 1, 3, true, false),
        ]
    );
    assert!(points[0].unwrap().learnable());
    assert!(!points[1].unwrap().learnable());
    // One point left: skill 0's formula asks two.
    w.units.get_mut(&ME).unwrap().stats.insert(5, 1);
    draw(&root, &w);
    let p0 = SEEN.with(|s| s.borrow().points[0]).unwrap();
    assert_eq!((p0.free, p0.learnable()), (1, false));
}

/// The sheet draws the game's item tip, skill description and hover text.
#[test]
fn a_sheet_draws_the_games_pop_ups() {
    let (_ui, root) = ui(sheet(), true);
    let w = world();
    let out = draw(&root, &w);
    let seen = SEEN.with(|s| s.borrow().clone());
    assert!(seen.tip && seen.desc, "{seen:?}");
    let t = texts(&out);
    assert!(t.iter().any(|s| s.contains("Cap")), "{t:?}");
    assert!(t.iter().any(|s| s == "Sheet"), "{t:?}");
    // The pop-ups' backing boxes.
    let boxes = out.iter().filter(|d| matches!(d, UiDraw::Rect(_))).count();
    assert!(boxes >= 2, "{out:?}");
}

/// Without the client's tables the reads that need them answer `None`
/// and nothing is drawn but the plain hover text.
#[test]
fn reads_without_the_tables_answer_none() {
    let (_ui, root) = ui(sheet(), false);
    let w = world();
    let out = draw(&root, &w);
    let seen = SEEN.with(|s| s.borrow().clone());
    // The defense needs no tables; an expansion game's resistances need
    // the penalties; the AR goes without the class factor.
    assert_eq!(seen.defense, Some(165));
    assert_eq!(seen.fire, None);
    assert_eq!(seen.attack_rating, Some(318));
    assert_eq!(seen.points, vec![None, None, None]);
    assert!(!seen.tip && !seen.desc);
    assert_eq!(texts(&out), vec!["Sheet".to_string()]);
}

/// A hireling item request is the game's 0x61 with the body location.
#[test]
fn a_hireling_item_request_is_the_games_message() {
    assert_eq!(
        request_intent(&ModRequest::MercItem { bodyloc: 4 }),
        Some(ClientIntent(vec![0x61, 4, 0]))
    );
}

/// D2Gamma's character panel shows the values the reads give: one rule
/// for both.
#[test]
fn d2gamma_s_character_panel_shows_the_read_values() {
    let (_ui, _root) = ui(ClientMods::new(), true);
    let w = world();
    let ctx = UiCtx {
        tick: 2,
        world: &w,
        strings: &NoStrings,
    };
    let f = fonts();
    let chars = CharTables::default();
    let view = ModelCharacter {
        world: &w,
        key: ME,
        fonts: &f,
        penalties: &[0, -40, -100],
        chars: &chars,
        experience: &[],
    };
    assert_eq!(ctx.defense(ME), Some(view.stat(31)));
    for (stat, max) in [FIRE, COLD] {
        let r = ctx.resistance(ME, stat, max).unwrap();
        assert_eq!(
            (r.shown, r.color),
            character::resist_value(&view, stat, max)
        );
    }
}
