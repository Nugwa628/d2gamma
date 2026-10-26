//! The values a mod game's combat formulas (MS3,
//! [`super::FormulaHooks`]) get beyond the units: the attacking weapon
//! ([`WeaponView`], from [`super::FormulaHost::weapon`]) and the inputs of
//! the block and avoid sequence ([`AvoidTerms`]).

/// The base item columns of the weapon a unit attacks with, as damage
/// formulas read them (its damage is in the unit's damage stats).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponView {
    /// The base item is two-handed (`2handed`): D2RUST then rolls the
    /// two-handed damage stats.
    pub two_handed: bool,
    /// The base item may be wielded in one or two hands (`1or2handed`).
    pub one_or_two_handed: bool,
    /// `StrBonus`: percent of strength added to the damage percent.
    pub str_bonus: i32,
    /// `DexBonus`: percent of dexterity added to the damage percent.
    pub dex_bonus: i32,
}

/// The inputs of the block and avoid sequence of one hit
/// ([`super::FormulaHooks::block_or_avoid`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AvoidTerms {
    /// The defender's block chance in percent (D2RUST's formula or the
    /// [`super::FormulaHooks::block_chance`] hook); 0 when the attack
    /// can't be blocked.
    pub block: i32,
    /// D2RUST would block at a third of `block`: a moving player not
    /// walking.
    pub block_third: bool,
    /// The hit is a missile's: avoid instead of dodge.
    pub missile: bool,
}
