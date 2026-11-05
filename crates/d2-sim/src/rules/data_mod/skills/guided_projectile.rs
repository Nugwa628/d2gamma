//! Homing projectile setup with explicit damage percentage and missile rows.
use super::Formula;
use crate::rules::{missile_init, MissileRequest, SkillHost};
use crate::units::UnitId;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuidedProjectilePart {
    pub missile: u16,
    #[serde(default)]
    pub non_bow: Option<u16>,
    pub damage_percent: Formula,
}
pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &GuidedProjectilePart,
) -> bool {
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    let target = w.target(u);
    let Some((tx, ty)) = w.target_position(u) else {
        return false;
    };
    let value = p.damage_percent.eval(w, u, skill, level);
    let row = if w.bow_hand(u) {
        p.missile
    } else {
        p.non_bow.unwrap_or(p.missile)
    };
    if !w.missile_row_exists(row) {
        return false;
    }
    let req = MissileRequest {
        flags: if target.is_some() { 0x20 } else { 0x420 },
        origin: Some(u),
        target,
        target_x: tx,
        target_y: ty,
        skill,
        level,
        init: (value != 0).then_some((missile_init::DAMAGE_PERCENT, value as u32)),
        ..MissileRequest::new(u, i32::from(row))
    };
    if let Some(m) = w.spawn_missile(req) {
        let (ux, uy) = w.position(u);
        let dx = i32::from(tx.wrapping_sub(ux) as i16);
        let dy = i32::from(ty.wrapping_sub(uy) as i16);
        w.set_missile_data(
            m,
            if target.is_some() { 1 } else { 2 },
            dy.wrapping_mul(0x10000).wrapping_add(dx),
        );
    }
    true
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn guided_projectile_is_opt_in_and_validates_rows() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='guided_projectile'\nmissile=0\ndamage_percent='lvl*3'\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::GuidedProjectile(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("missile=0", "missile=65535")).is_err());
    }
}
