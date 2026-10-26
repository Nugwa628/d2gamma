//! The character rules: name and class unchanged, the status bits (a
//! hardcore death can't be undone), level and experience, experience per
//! game-minute, stat and skill points, and the gold caps.

use d2_formats::d2s::{status, D2s};
use d2_sim::world::stash::{GOLD_PER_LEVEL, STASH_CAP};

use crate::decoded::{char_stat, has_char_stat, stat};
use crate::{CheckFail, SaveCheck, SaveCtx};

/// Stat points a quest gives in each difficulty (the Act 3 tome).
pub const QUEST_STAT_POINTS: i64 = 5;
/// Skill points quests give in each difficulty (the Act 1 den, the Act 2
/// book, the Act 4 reward of two).
pub const QUEST_SKILL_POINTS: i64 = 4;
/// Skill points per character level (the level-up rule's).
pub const SKILL_POINTS_PER_LEVEL: i64 = 1;
/// The experience-rate check's default cap per game-minute (plan C1; a
/// proposed default, generous on purpose: well above a full party's fast
/// runs at the end of the game).
pub const EXP_PER_MINUTE: u64 = 25_000_000;

fn fail(reason: String) -> Result<(), CheckFail> {
    Err(CheckFail::new(reason))
}

/// How many difficulties the status word opens.
fn open_difficulties(status_word: u16) -> i64 {
    i64::from(status::difficulties(status_word).count_ones())
}

/// The name and the class never change.
#[derive(Clone, Copy, Debug, Default)]
pub struct NameClass;

impl SaveCheck for NameClass {
    fn check(&self, before: &D2s, after: &D2s, _: &SaveCtx) -> Result<(), CheckFail> {
        let (b, a) = (&before.header, &after.header);
        if b.name_bytes() != a.name_bytes() {
            return fail(format!(
                "name changed from `{}` to `{}`",
                String::from_utf8_lossy(b.name_bytes()),
                String::from_utf8_lossy(a.name_bytes())
            ));
        }
        if b.class != a.class {
            return fail(format!("class changed from {} to {}", b.class, a.class));
        }
        Ok(())
    }
}

/// The status bits that describe the character's economy and fate:
/// hardcore and ladder never change, expansion is never taken away, and a
/// dead hardcore character stays dead.
#[derive(Clone, Copy, Debug, Default)]
pub struct Status;

impl SaveCheck for Status {
    fn check(&self, before: &D2s, after: &D2s, _: &SaveCtx) -> Result<(), CheckFail> {
        let (b, a) = (before.header.status, after.header.status);
        for (bit, name) in [(status::HARDCORE, "hardcore"), (status::LADDER, "ladder")] {
            if b & bit != a & bit {
                return fail(format!("the {name} bit changed ({b:#06x} to {a:#06x})"));
            }
        }
        if b & status::EXPANSION != 0 && a & status::EXPANSION == 0 {
            return fail("the expansion bit was taken away".into());
        }
        if b & status::HARDCORE != 0 && b & status::DEAD != 0 && a & status::DEAD == 0 {
            return fail("a dead hardcore character came back to life".into());
        }
        Ok(())
    }
}

/// The level is the level the experience reaches (the experience table of
/// the realm's rules), in the header and in the stats.
#[derive(Clone, Copy, Debug, Default)]
pub struct LevelExperience;

impl SaveCheck for LevelExperience {
    fn check(&self, _: &D2s, after: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        let Some(_) = after.body else {
            return Ok(());
        };
        let h = &after.header;
        let level = i64::from(h.level);
        if has_char_stat(after, stat::LEVEL) && char_stat(after, stat::LEVEL) != level {
            return fail(format!(
                "header level {level} but stats level {}",
                char_stat(after, stat::LEVEL)
            ));
        }
        let exp = char_stat(after, stat::EXPERIENCE);
        let class = i32::from(h.class);
        let v = &ctx.tables.vitals;
        if v.experience.is_empty() {
            return Ok(());
        }
        let rules = &ctx.tables.items.rules;
        let expected = i64::from(v.level_from_exp_in(rules, class, exp as u32));
        if level != expected {
            return fail(format!(
                "level {level} with {exp} experience (that experience is level {expected})"
            ));
        }
        Ok(())
    }
}

/// The experience gained since the stored record stays under a cap per
/// game-minute played ([`EXP_PER_MINUTE`] by default); it runs only when
/// the main server knows the time played ([`crate::SaveInfo::played_ms`]).
#[derive(Clone, Copy, Debug)]
pub struct ExperienceRate {
    /// The cap per started game-minute.
    pub per_minute: u64,
}

impl Default for ExperienceRate {
    fn default() -> Self {
        Self {
            per_minute: EXP_PER_MINUTE,
        }
    }
}

impl SaveCheck for ExperienceRate {
    fn check(&self, before: &D2s, after: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        let Some(ms) = ctx.info.played_ms else {
            return Ok(());
        };
        let gain = char_stat(after, stat::EXPERIENCE) - char_stat(before, stat::EXPERIENCE);
        let minutes = ms / 60_000 + 1;
        let cap = self.per_minute.saturating_mul(minutes);
        if gain > 0 && gain as u64 > cap {
            return fail(format!(
                "{gain} experience in {ms} ms of play (cap {cap} for {minutes} started minutes)"
            ));
        }
        Ok(())
    }
}

/// The attributes are at least the class's start values, and the points
/// spent on them plus the unspent ones are not above what the levels
/// (`charstats` `StatPerLevel`) and the quests of the open difficulties
/// give.
#[derive(Clone, Copy, Debug, Default)]
pub struct StatPoints;

impl SaveCheck for StatPoints {
    fn check(&self, _: &D2s, after: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        if after.body.is_none() {
            return Ok(());
        }
        let h = &after.header;
        let Some(cs) = ctx.tables.vitals.charstats(i32::from(h.class)) else {
            return fail(format!("class {} has no charstats row", h.class));
        };
        let attrs = [
            ("strength", stat::STRENGTH, cs.str),
            ("energy", stat::ENERGY, cs.int),
            ("dexterity", stat::DEXTERITY, cs.dex),
            ("vitality", stat::VITALITY, cs.vit),
        ];
        let mut spent = 0i64;
        for (name, id, base) in attrs {
            let v = char_stat(after, id);
            let base = i64::from(base);
            if v < base {
                return fail(format!("{name} {v} is below the class's {base}"));
            }
            spent += v - base;
        }
        let unspent = char_stat(after, stat::STATPTS);
        if unspent < 0 {
            return fail(format!("{unspent} unspent stat points"));
        }
        let level = i64::from(h.level).max(1);
        let per_level = i64::from(cs.statperlevel as i8);
        let allowed = (level - 1) * per_level + QUEST_STAT_POINTS * open_difficulties(h.status);
        if spent + unspent > allowed {
            return fail(format!(
                "{spent} stat points spent and {unspent} unspent, but level {level} and the quests give {allowed}"
            ));
        }
        Ok(())
    }
}

/// The skill points in the skills plus the unspent ones are not above what
/// the levels and the quests of the open difficulties give; no skill is
/// above its maximum or learned below its required level.
#[derive(Clone, Copy, Debug, Default)]
pub struct SkillPoints;

impl SaveCheck for SkillPoints {
    fn check(&self, _: &D2s, after: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        let Some(body) = &after.body else {
            return Ok(());
        };
        let h = &after.header;
        let level = i64::from(h.level).max(1);
        let t = &ctx.tables.items;
        let sl = &t.skill_lists;
        let class = usize::from(h.class);
        for (i, &points) in body.skills.iter().enumerate() {
            if points == 0 {
                continue;
            }
            let skill = sl
                .lists
                .get(class * sl.max + i)
                .filter(|_| i < sl.counts.get(class).map_or(0, |&n| n as usize));
            let Some(rec) = skill.and_then(|&s| t.skills.get(usize::from(s))) else {
                continue;
            };
            let max = if rec.maxlvl > 0 { rec.maxlvl } else { 20 };
            if i32::from(points) > max {
                return fail(format!(
                    "skill {} has {points} points (most {max})",
                    skill.copied().unwrap_or(0)
                ));
            }
            if i64::from(rec.reqlevel) > level {
                return fail(format!(
                    "skill {} learned at level {level} (needs level {})",
                    skill.copied().unwrap_or(0),
                    rec.reqlevel
                ));
            }
        }
        let spent: i64 = body.skills.iter().map(|&b| i64::from(b)).sum();
        let unspent = char_stat(after, stat::NEWSKILLS);
        if unspent < 0 {
            return fail(format!("{unspent} unspent skill points"));
        }
        let allowed =
            (level - 1) * SKILL_POINTS_PER_LEVEL + QUEST_SKILL_POINTS * open_difficulties(h.status);
        if spent + unspent > allowed {
            return fail(format!(
                "{spent} skill points spent and {unspent} unspent, but level {level} and the quests give {allowed}"
            ));
        }
        Ok(())
    }
}

/// Carried gold is at most level × 10,000 and stash gold at most
/// 2,500,000 (the game's own caps), neither negative.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gold;

impl SaveCheck for Gold {
    fn check(&self, _: &D2s, after: &D2s, _: &SaveCtx) -> Result<(), CheckFail> {
        let level = i64::from(after.header.level).max(1);
        let gold = char_stat(after, stat::GOLD);
        let cap = level * i64::from(GOLD_PER_LEVEL);
        if !(0..=cap).contains(&gold) {
            return fail(format!("{gold} gold carried at level {level} (cap {cap})"));
        }
        let bank = char_stat(after, stat::GOLDBANK);
        if !(0..=i64::from(STASH_CAP)).contains(&bank) {
            return fail(format!("{bank} gold in the stash (cap {STASH_CAP})"));
        }
        Ok(())
    }
}
