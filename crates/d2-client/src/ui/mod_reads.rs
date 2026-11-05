//! What a mod game's panels read of the game's rules, and the game's own
//! pop-ups they draw with (`crate::mods`), through their context:
//!
//! - [`UiCtx::defense`], [`UiCtx::resistance`], [`UiCtx::attack_rating`]
//!   and [`UiCtx::chance_to_hit`]: a unit's combat values as the character
//!   panel works them out;
//! - [`UiCtx::skill_point`]: what the next point in a skill costs the
//!   player and whether it meets the skill's requirements, by the skills
//!   tables' rules;
//! - [`UiCtx::draw_item_tip`], [`UiCtx::draw_skill_desc`] and
//!   [`UiCtx::draw_hover_text`]: the game's item tip, skill description and
//!   hover text boxes;
//! - [`UiCtx::quest_inputs`]: what the quest log derives its rows from (the
//!   server's status list, the quest flag records and counters);
//! - [`UiCtx::waypoint_open`]: the waypoint menu the server opened (the
//!   waypoint's GUID and the player's activation record).
//!
//! The reads that need the UI's tables answer `None` until the client has
//! them. D2Gamma's own panels read them directly and never call these.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use d2_sim::combat::hit::{hit_chance, HitTerms};
use d2_sim::skills::levels::{max_level, meets_attr_reqs, meets_skill_reqs, skill_point_cost};

use super::{hud_tips, skill_hover, skill_hover_lines, Shared, SharedRef};
use crate::bridge::world::{ClientWorld, UnitKey, PLAYER};
use crate::ui::char_desc::DescUnits;
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::Point;
use crate::ui::item_tip;
use crate::ui::panel::UiCtx;
use crate::ui::panels::char_inputs;
use crate::ui::panels::character::{self, CharacterView, ResistEffect};

thread_local! {
    static SHARED: RefCell<Weak<RefCell<Shared>>> = const { RefCell::new(Weak::new()) };
}

/// Makes the UI's tables readable from mod panels (the client, at
/// install).
pub(super) fn attach(sh: &SharedRef) {
    SHARED.with(|s| *s.borrow_mut() = Rc::downgrade(sh));
}

/// `f` of the UI's shared state; `None` before install or while the state
/// is being changed.
fn with_shared<R>(f: impl FnOnce(&Shared) -> Option<R>) -> Option<R> {
    let sh = SHARED.with(|s| s.borrow().upgrade())?;
    let guard = sh.try_borrow().ok()?;
    f(&guard)
}

/// A unit's resistance as the character panel shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resistance {
    /// The stat after the difficulty's penalty (at least −100), uncapped.
    pub value: i32,
    /// The cap: 75 plus the max stat, at most 95.
    pub cap: i32,
    /// The value shown: `value` up to `cap`.
    pub shown: i32,
    /// The panel's text colour of it (`ui::item_tip::color`): red below 0,
    /// gold at the cap, blue or red under a resist effect, else white.
    pub color: u16,
}

/// The player's next point in a skill, by the skills tables' rules (the
/// server checks the same before it adds the point).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillPoint {
    /// Hard points in the skill.
    pub level: i32,
    /// The skill's max level.
    pub max: i32,
    /// The points the next level costs (`skpoints`, else 1).
    pub cost: i32,
    /// Unspent skill points.
    pub free: i32,
    /// The required level and skills are met.
    pub reqs_met: bool,
    /// The skill is in game and its level and attribute requirements are
    /// met.
    pub attrs_met: bool,
}

impl SkillPoint {
    /// A point can be spent on the skill now.
    pub fn learnable(&self) -> bool {
        self.reqs_met && self.attrs_met && self.level < self.max && self.free >= self.cost
    }
}

/// The character panel's rules over any unit of the model.
struct UnitView<'a> {
    world: &'a ClientWorld,
    key: UnitKey,
    penalties: &'a [i32],
    chars: &'a crate::ui::char_feed::CharTables,
}

impl CharacterView for UnitView<'_> {
    fn stat(&self, id: u16) -> i32 {
        if id == 31 {
            return super::character_defense(self.world, self.key);
        }
        self.world.total(self.key, id, 0)
    }
    fn base(&self, id: u16) -> i32 {
        self.world.base(self.key, id, 0)
    }
    fn alive(&self) -> bool {
        self.world
            .units
            .get(&self.key)
            .is_some_and(|u| !u.is_dead())
    }
    fn language(&self) -> u8 {
        0
    }
    fn resist_penalty(&self) -> i32 {
        let w = self.world;
        -char_inputs::resist_value(
            0,
            w.expansion != 0,
            usize::from(w.difficulty),
            self.penalties,
        )
    }
    fn resist_effect(&self, id: u16) -> ResistEffect {
        match self.world.units.get(&self.key) {
            Some(u) => self.chars.resist_effect(&u.states, id),
            None => ResistEffect::None,
        }
    }
}

impl UiCtx<'_> {
    /// `unit`'s defense as the character panel works it out: armor class
    /// plus a quarter of dexterity, then the armor percents.
    pub fn defense(&self, unit: UnitKey) -> Option<i32> {
        self.world.units.get(&unit)?;
        Some(super::character_defense(self.world, unit))
    }

    /// `unit`'s resistance of stat `stat` (max stat `max_stat`) as the
    /// character panel shows it, after the difficulty's penalty.
    pub fn resistance(&self, unit: UnitKey, stat: u16, max_stat: u16) -> Option<Resistance> {
        self.world.units.get(&unit)?;
        with_shared(|sh| {
            let penalties = match (&sh.resist_penalties, self.world.expansion != 0) {
                (Some(p), _) => p.as_slice(),
                // A classic game's penalty is fixed.
                (None, false) => &[][..],
                (None, true) => return None,
            };
            let view = UnitView {
                world: self.world,
                key: unit,
                penalties,
                chars: &sh.char_tables,
            };
            let (value, cap) = character::resist_terms(&view, stat, max_stat);
            let (shown, color) = character::resist_value(&view, stat, max_stat);
            Some(Resistance {
                value,
                cap,
                shown,
                color,
            })
        })
    }

    /// `unit`'s attack rating before its percent bonus, as the character
    /// panel works it out: to-hit plus 5 per dexterity point over 7, plus
    /// the class's factor for a player.
    fn base_attack_rating(&self, unit: UnitKey) -> Option<i32> {
        let u = self.world.units.get(&unit)?;
        let factor = if unit.unit_type == PLAYER {
            with_shared(|sh| sh.char_tables.tohit_factor.get(u.class as usize).copied())
                .unwrap_or(0)
        } else {
            0
        };
        let t = |id| self.world.total(unit, id, 0);
        Some(t(19) + 5 * (t(2) - 7) + factor)
    }

    /// `unit`'s attack rating as the character panel shows it (the percent
    /// bonus, stat 119, included).
    pub fn attack_rating(&self, unit: UnitKey) -> Option<i32> {
        let ar = self.base_attack_rating(unit)?;
        Some(crate::ui::skill_desc::ar_value(
            ar,
            self.world.total(unit, 119, 0),
        ))
    }

    /// `unit`'s chance to hit (percent, 5 to 95) a target of `defense` at
    /// level `level`, by the game's hit rule; `None` when both levels are
    /// 0.
    pub fn chance_to_hit(&self, unit: UnitKey, defense: i32, level: i32) -> Option<i32> {
        let ar = self.base_attack_rating(unit)?;
        let alvl = self.world.total(unit, 12, 0);
        if alvl.wrapping_add(level) == 0 {
            return None;
        }
        Some(hit_chance(HitTerms {
            ar,
            pct_ar: self.world.total(unit, 119, 0),
            def: defense,
            alvl,
            dlvl: level,
        }))
    }

    /// The local player's next point in `skill` (cost, requirements,
    /// levels); `None` without the skills tables, a local player or a row
    /// for the skill.
    pub fn skill_point(&self, skill: u16) -> Option<SkillPoint> {
        let key = self.world.local_player?;
        let me = self.world.units.get(&key)?;
        let level = me
            .skills
            .as_ref()
            .and_then(|l| l.native(skill).map(|i| l.entries[i].base))
            .unwrap_or(0);
        with_shared(|sh| {
            let t = sh.char_tables.desc.skills.as_deref()?;
            let s = i32::from(skill);
            let rec = t.skill(s)?;
            let mut units = DescUnits::new(self.world, key, &sh.char_tables.desc);
            Some(SkillPoint {
                level,
                max: max_level(rec),
                cost: skill_point_cost(&mut units, t, key, s)?,
                free: self.world.base(key, 5, 0),
                reqs_met: meets_skill_reqs(&units, t, key, s),
                attrs_met: meets_attr_reqs(&units, t, key, s),
            })
        })
    }

    /// Draws the game's tip of item `item` as its inventory shows it, the
    /// box centred on `at.x` above `at` (below when there is no room);
    /// false when there is no tip (an unknown item, or no item tables).
    pub fn draw_item_tip(&self, item: UnitKey, at: Point, out: &mut dyn UiDrawSink) -> bool {
        use crate::ui::item_tip_world::{hover_ctx, model_bits, WorldUnit};
        with_shared(|sh| {
            let tips = sh.items.tips.as_ref()?;
            let it = crate::bridge::items::item(self.world, item)?;
            let me = WorldUnit::local(self.world);
            let ctx = hover_ctx(tips, self.world, me.as_ref(), &it);
            let lines = tips.tip_lines_of(&model_bits(tips, self.world, item)?, &ctx);
            if lines.is_empty() {
                return None;
            }
            let s = sh.config.screen;
            let anchor = item_tip::TipAnchor::at_point(at);
            item_tip::draw_tip_at(&lines, anchor, (s.w, s.h), sh.fonts.as_ref(), out);
            Some(())
        })
        .is_some()
    }

    /// Draws the game's description box of a skill icon (`hover`: the
    /// skill, the level shown, the icon's point, whether a point can be
    /// spent on it, [`UiCtx::skill_point`]) for the local player; false
    /// without the skills tables or a local player.
    pub fn draw_skill_desc(
        &self,
        hover: &skill_hover::SkillHover,
        out: &mut dyn UiDrawSink,
    ) -> bool {
        with_shared(|sh| {
            let t = sh.char_tables.desc.skills.as_deref()?;
            let key = self.world.local_player?;
            let mut inputs = skill_hover_lines::ModelInputs::new(
                self.world,
                key,
                &sh.char_tables.desc,
                t,
                self.strings,
                &sh.hud.tables.hover,
            );
            let s = sh.config.screen;
            skill_hover::draw(hover, &mut inputs, sh.fonts.as_ref(), (s.w, s.h), out);
            Some(())
        })
        .is_some()
    }

    /// Draws the game's hover text box: `text` (UTF-16, colour codes kept)
    /// in colour `color` with its bottom left at `at`, or its bottom centre
    /// when `centered`, kept on the screen.
    pub fn draw_hover_text(
        &self,
        text: Vec<u16>,
        at: Point,
        color: u16,
        centered: bool,
        out: &mut dyn UiDrawSink,
    ) {
        with_shared(|sh| {
            let s = sh.config.screen;
            hud_tips::push_popup(
                text,
                at,
                color,
                centered,
                (s.w, s.h),
                sh.fonts.as_ref(),
                out,
            );
            Some(())
        });
    }
}

impl UiCtx<'_> {
    /// What the quest log derives its rows from, as the server sent it
    /// (`quest_log_ui::rows` over it gives the game's own rows); `None`
    /// before install.
    pub fn quest_inputs(&self) -> Option<super::quest_log_ui::QuestInputs> {
        with_shared(|sh| Some(sh.quest.clone()))
    }

    /// The waypoint menu the server opened last (S→C 0x63): the waypoint's
    /// GUID, the player's activation record, the level the player stood
    /// in and the tab it chose; its `seq` changes with every opening.
    /// `None` before the first.
    pub fn waypoint_open(&self) -> Option<super::WaypointOpen> {
        with_shared(|sh| sh.waypoint_open)
    }
}

#[cfg(test)]
#[path = "mod_reads_tests.rs"]
mod tests;
