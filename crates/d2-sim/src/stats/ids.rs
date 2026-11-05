//! Shared names for the itemstatcost ids used by D2Gamma.
//! These engine symbols share one assignment. Local modules re-export existing
//! spellings so callers and default game behavior keep the same ids.

macro_rules! stat_ids {
    ($($name:ident = $id:literal;)+) => {
        $(pub const $name: u16 = $id;)+
        /// Engine symbols and their ids, in numeric order.
        pub const NAMED_STATS: &[(&str, u16)] = &[$((stringify!($name), $name)),+];
    };
}

stat_ids! {
    STRENGTH = 0;
    ENERGY = 1;
    DEXTERITY = 2;
    VITALITY = 3;
    STATPTS = 4;
    NEWSKILLS = 5;
    HITPOINTS = 6;
    MAXHP = 7;
    MANA = 8;
    MAXMANA = 9;
    STAMINA = 10;
    MAXSTAMINA = 11;
    LEVEL = 12;
    EXPERIENCE = 13;
    GOLD = 14;
    GOLDBANK = 15;
    ARMOR_PERCENT = 16;
    MAXDAMAGE_PERCENT = 17;
    MINDAMAGE_PERCENT = 18;
    TOHIT = 19;
    TOBLOCK = 20;
    MINDAMAGE = 21;
    MAXDAMAGE = 22;
    SECONDARY_MINDAMAGE = 23;
    SECONDARY_MAXDAMAGE = 24;
    DAMAGEPERCENT = 25;
    MANARECOVERY = 26;
    MANARECOVERYBONUS = 27;
    STAMINARECOVERYBONUS = 28;
    LASTEXP = 29;
    NEXTEXP = 30;
    ARMORCLASS = 31;
    AC_VS_MISSILE = 32;
    AC_VS_HTH = 33;
    NORMAL_DR = 34;
    MAGIC_DR = 35;
    DAMAGERESIST = 36;
    MAGICRESIST = 37;
    FIRERESIST = 39;
    LIGHTRESIST = 41;
    COLDRESIST = 43;
    POISONRESIST = 45;
    FIREMINDAM = 48;
    FIREMAXDAM = 49;
    LIGHTMINDAM = 50;
    LIGHTMAXDAM = 51;
    MAGICMINDAM = 52;
    MAGICMAXDAM = 53;
    COLDMINDAM = 54;
    COLDMAXDAM = 55;
    COLDLENGTH = 56;
    POISONMINDAM = 57;
    POISONMAXDAM = 58;
    POISONLENGTH = 59;
    LIFEDRAINMINDAM = 60;
    LIFEDRAINMAXDAM = 61;
    MANADRAINMINDAM = 62;
    MANADRAINMAXDAM = 63;
    STAMDRAINMINDAM = 64;
    STAMDRAINMAXDAM = 65;
    STUNLENGTH = 66;
    VELOCITYPERCENT = 67;
    ATTACKRATE = 68;
    OTHER_ANIMRATE = 69;
    QUANTITY = 70;
    VALUE = 71;
    DURABILITY = 72;
    MAXDURABILITY = 73;
    HPREGEN = 74;
    ITEM_ADDCLASSSKILLS = 83;
    UNSENTPARAM1 = 84;
    ADDEXPERIENCE = 85;
    REDUCED_PRICES = 87;
    ITEM_REQ_PERCENT = 91;
    ITEM_NONCLASSSKILL = 97;
    STATE = 98;
    MONSTER_PLAYERCOUNT = 100;
    POISON_LENGTH_OVERRIDE = 101;
    BYPASS_UNDEAD = 103;
    BYPASS_DEMONS = 104;
    BYPASS_BEASTS = 106;
    ITEM_SINGLESKILL = 107;
    CURSE_RESISTANCE = 109;
    POISONLENGTHRESIST = 110;
    NORMALDAMAGE = 111;
    IGNORETARGETAC = 115;
    FRACTIONALTARGETAC = 116;
    PREVENTHEAL = 117;
    HALFFREEZE = 118;
    ITEM_TOHIT_PERCENT = 119;
    ITEM_DAMAGETARGETAC = 120;
    DEMONDAMAGE_PERCENT = 121;
    UNDEADDAMAGE_PERCENT = 122;
    DEMON_TOHIT = 123;
    UNDEAD_TOHIT = 124;
    ITEM_THROWABLE = 125;
    ITEM_ELEMSKILL = 126;
    ITEM_ALLSKILLS = 127;
    DEADLY_STRIKE = 141;
    ABSORB_FIRE_PCT = 142;
    ABSORB_LIGHT_PCT = 144;
    ABSORB_COLD_PCT = 148;
    ITEM_SLOW = 150;
    ITEM_AURA = 151;
    INDESTRUCTIBLE = 152;
    CANNOTBEFROZEN = 153;
    ITEM_PIERCE = 156;
    MAGICARROW = 157;
    EXPLOSIVEARROW = 158;
    THROW_MINDAMAGE = 159;
    THROW_MAXDAMAGE = 160;
    SKILL_HANDOFATHENA = 161;
    STAMINA_BONUS = 162;
    SKILL_PIERCE = 166;
    SKILL_FRENZY = 169;
    SKILL_ARMOR_PERCENT = 171;
    ALIGNMENT = 172;
    CONVERSION_LEVEL = 176;
    CONVERSION_MAXHP = 177;
    ATTACK_VS_MONTYPE = 179;
    DAMAGE_VS_MONTYPE = 180;
    ARMOR_OVERRIDE_PERCENT = 182;
    ITEM_ADDSKILL_TAB = 188;
    NUMSOCKETS = 194;
    ITEM_CHARGED_SKILL = 204;
    REPLENISH_DURABILITY = 252;
    REPLENISH_QUANTITY = 253;
    EXTRA_STACK = 254;
    BURNINGLENGTH = 315;
    BURNINGMINDAM = 316;
    BURNINGMAXDAM = 317;
    PROGRESSIVE_TOHIT = 325;
    POISON_COUNT = 326;
    DAMAGE_FRAMERATE = 327;
    PIERCE_IDX = 328;
    FIRE_MASTERY = 329;
    LIGHT_MASTERY = 330;
    COLD_MASTERY = 331;
    POISON_MASTERY = 332;
    PASSIVE_CRITICAL_STRIKE = 337;
    PASSIVE_DODGE = 338;
    PASSIVE_AVOID = 339;
    PASSIVE_EVADE = 340;
    PASSIVE_WEAPONBLOCK = 348;
    PASSIVE_SUMMON_RESIST = 349;
    MODIFIERLIST_SKILL = 350;
    MODIFIERLIST_LEVEL = 351;
    LAST_SENT_HP_PCT = 352;
    SOURCE_UNIT_TYPE = 353;
    SOURCE_UNIT_ID = 354;
    QUESTITEMDIFFICULTY = 356;
    MAGIC_MASTERY = 357;
}

// Existing engine spellings for the same stat.
pub use ARMOR_PERCENT as ITEM_ARMOR_PERCENT;
pub use BURNINGLENGTH as FIRELENGTH;
pub use BURNINGMAXDAM as BURNINGMAX;
pub use BURNINGMINDAM as BURNINGMIN;
pub use BYPASS_BEASTS as IGNORE_TARGET_DEFENSE;
pub use BYPASS_DEMONS as FRACTIONAL_TARGET_AC;
pub use BYPASS_UNDEAD as IGNORE_TARGET_AC;
pub use COLD_MASTERY as PASSIVE_COLD_MASTERY;
pub use DEADLY_STRIKE as DEADLYSTRIKE;
pub use DEXTERITY as DEX;
pub use FIRE_MASTERY as PASSIVE_FIRE_MASTERY;
pub use GOLDBANK as GOLD_BANK;
pub use HITPOINTS as LIFE;
pub use ITEM_CHARGED_SKILL as CHARGED_SKILL;
pub use ITEM_DAMAGETARGETAC as DAMAGETARGETAC;
pub use ITEM_TOHIT_PERCENT as TOHIT_PERCENT;
pub use LIGHT_MASTERY as PASSIVE_LTNG_MASTERY;
pub use MAXHP as MAX_LIFE;
pub use MAXMANA as MAX_MANA;
pub use MAXSTAMINA as MAX_STAMINA;
pub use MINDAMAGE as MIN_DAMAGE;
pub use NUMSOCKETS as ITEM_194;
pub use POISON_LENGTH_OVERRIDE as POISON_OVERRIDE_LENGTH;
pub use POISON_MASTERY as PASSIVE_POIS_MASTERY;
pub use SECONDARY_MINDAMAGE as MIN_DAMAGE_2H;
pub use STAMINARECOVERYBONUS as STAMINA_RECOVERY;
pub use STRENGTH as STR;
pub use THROW_MAXDAMAGE as ITEM_THROW_MAXDAMAGE;
pub use THROW_MINDAMAGE as ITEM_THROW_MINDAMAGE;
pub use TOHIT as TO_HIT;

/// Existing alternate engine spellings, for named mod references.
pub const ALIASES: &[(&str, u16)] = &[
    ("ITEM_ARMOR_PERCENT", ARMOR_PERCENT),
    ("FIRELENGTH", BURNINGLENGTH),
    ("BURNINGMAX", BURNINGMAXDAM),
    ("BURNINGMIN", BURNINGMINDAM),
    ("IGNORE_TARGET_DEFENSE", BYPASS_BEASTS),
    ("FRACTIONAL_TARGET_AC", BYPASS_DEMONS),
    ("IGNORE_TARGET_AC", BYPASS_UNDEAD),
    ("PASSIVE_COLD_MASTERY", COLD_MASTERY),
    ("DEADLYSTRIKE", DEADLY_STRIKE),
    ("DEX", DEXTERITY),
    ("PASSIVE_FIRE_MASTERY", FIRE_MASTERY),
    ("GOLD_BANK", GOLDBANK),
    ("LIFE", HITPOINTS),
    ("CHARGED_SKILL", ITEM_CHARGED_SKILL),
    ("DAMAGETARGETAC", ITEM_DAMAGETARGETAC),
    ("TOHIT_PERCENT", ITEM_TOHIT_PERCENT),
    ("PASSIVE_LTNG_MASTERY", LIGHT_MASTERY),
    ("MAX_LIFE", MAXHP),
    ("MAX_MANA", MAXMANA),
    ("MAX_STAMINA", MAXSTAMINA),
    ("MIN_DAMAGE", MINDAMAGE),
    ("ITEM_194", NUMSOCKETS),
    ("POISON_OVERRIDE_LENGTH", POISON_LENGTH_OVERRIDE),
    ("PASSIVE_POIS_MASTERY", POISON_MASTERY),
    ("MIN_DAMAGE_2H", SECONDARY_MINDAMAGE),
    ("STAMINA_RECOVERY", STAMINARECOVERYBONUS),
    ("STR", STRENGTH),
    ("ITEM_THROW_MAXDAMAGE", THROW_MAXDAMAGE),
    ("ITEM_THROW_MINDAMAGE", THROW_MINDAMAGE),
    ("TO_HIT", TOHIT),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_unique_names_and_increasing_ids() {
        for pair in NAMED_STATS.windows(2) {
            assert!(pair[0].1 < pair[1].1);
        }
        let names: std::collections::BTreeSet<_> =
            NAMED_STATS.iter().map(|&(name, _)| name).collect();
        assert_eq!(names.len(), NAMED_STATS.len());
        assert!(NAMED_STATS
            .iter()
            .all(|&(_, id)| id != super::super::NO_STAT));
    }

    #[test]
    fn default_stat_bindings_and_local_types_stay_compatible() {
        use crate::{combat, items, missiles, monsters, skills, world};

        let _: u16 = items::stat::ATTACKRATE;
        let _: i32 = combat::events::stat::ATTACKRATE;
        let _: i32 = skills::use_::bodies::helpers::stat::ATTACKRATE;
        assert_eq!(items::stat::ATTACKRATE, 68);
        assert_eq!(combat::events::stat::ATTACKRATE, 68);
        assert_eq!(crate::stats::stat::HITPOINTS, 6);
        assert_eq!(missiles::stat::MINDAMAGE, 21);
        assert_eq!(skills::stat_cb::stat_id::ITEM_AURA, 151);
        assert_eq!(world::stash::stat::GOLD, 14);
        // These pre-existing names denote different stats in the two callers.
        // Centralization must not silently change shrine or monster behavior.
        assert_eq!(world::objects::shrines::stat::ITEM_ARMOR_PERCENT, 25);
        assert_eq!(monsters::init::stat::ITEM_ARMOR_PERCENT, 16);
        assert_eq!(ITEM_ARMOR_PERCENT, 16);
        assert_eq!(DAMAGEPERCENT, 25);
    }
}
