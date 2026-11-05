//! Editable spread and jitter fans through the shared missile request path.
use super::{Formula, MAX_MISSILES};
use crate::rules::{missile_init, MissileRequest, SkillHost};
use crate::units::UnitId;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectileFanPart {
    pub missile: u16,
    #[serde(default)]
    pub non_bow: Option<u16>,
    pub count: Formula,
    #[serde(default = "zero")]
    pub central: Formula,
    #[serde(default = "zero")]
    pub activate: Formula,
    #[serde(default)]
    pub jitter: bool,
    /// A bolt fan accepts an empty count; a tornado fan refuses it.
    #[serde(default)]
    pub empty_ok: bool,
}
fn zero() -> Formula {
    Formula::constant(0)
}
pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &ProjectileFanPart,
) -> bool {
    let missile = if !p.jitter && !w.bow_hand(u) {
        p.non_bow.unwrap_or(p.missile)
    } else {
        p.missile
    };
    if !p.jitter || p.empty_ok {
        w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    }
    if !w.missile_row_exists(missile) {
        return false;
    }
    if p.jitter && !p.empty_ok {
        w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    }
    let n = p.count.eval(w, u, skill, level).clamp(0, MAX_MISSILES);
    if p.jitter && n == 0 && !p.empty_ok {
        return false;
    }
    let Some((tx, ty)) = w.target_position(u) else {
        return true;
    };
    if p.jitter {
        let (x, y) = w.position(u);
        let mut req = MissileRequest {
            flags: 0x21,
            x,
            y,
            target_x: tx,
            target_y: ty,
            skill,
            level,
            ..MissileRequest::new(u, i32::from(missile))
        };
        for i in 0..n {
            req.init = Some((missile_init::JITTER, i as u32));
            w.spawn_missile(req);
        }
        return true;
    }
    let (ux, uy) = w.position(u);
    let (px, py) =
        crate::skills::use_::bodies::dos2::side_step((tx.wrapping_sub(ux), ty.wrapping_sub(uy)));
    let (mut tx, mut ty) = (
        tx.wrapping_sub(px.wrapping_mul(n) / 2),
        ty.wrapping_sub(py.wrapping_mul(n) / 2),
    );
    let c = p.central.eval(w, u, skill, level);
    let c = if c == 0 { n } else { c.clamp(0, n) };
    let k = n.wrapping_sub(c) / 2;
    let mut req = MissileRequest {
        flags: 0x820 | 0x1_0000,
        origin: Some(u),
        skill,
        level,
        activate: p.activate.eval(w, u, skill, level),
        ..MissileRequest::new(u, i32::from(missile))
    };
    for i in 0..n {
        if i >= k && i < k + c {
            req.flags &= !0x1_0000;
        } else {
            req.flags |= 0x1_0000;
        }
        req.target_x = tx;
        req.target_y = ty;
        w.spawn_missile(req);
        tx = tx.wrapping_add(px);
        ty = ty.wrapping_add(py);
    }
    true
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn projectile_fan_is_opt_in_and_validates_rows() {
        let text =
            "[[skill]]\nid=7\n[[skill.part]]\nkind='projectile_fan'\nmissile=0\ncount='lvl+2'\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::ProjectileFan(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("missile=0", "missile=65535")).is_err());
    }
}
