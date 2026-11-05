//! The form test of `use_state` test 7: a skill's `restrict` column and
//! its `State1`…`State3` against the states on the unit. One rule for
//! every caller (the server's skill use view and the client's use
//! state); each answers the unit questions through [`UnitForm`], and a
//! mod game's [`crate::rules::SkillProvider::form_ok`] answers first.

use d2_data::tables::Skills;

/// No state in a `State1`…`State3` cell.
pub const NO_STATE: u16 = 0xFFFF;

/// What the form test reads of the unit.
pub trait UnitForm {
    /// The unit is in a form: it has a state with the `states.txt`
    /// `restrict` flag.
    fn in_form(&self) -> bool;
    /// The unit has `state`.
    fn has_state(&self, state: u16) -> bool;
}

/// The skill row parts the test reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormSkill {
    pub skill: i32,
    /// skills.txt `restrict`.
    pub restrict: u8,
    /// skills.txt `State1`…`State3`, [`NO_STATE`] = none.
    pub states: [u16; 3],
}

impl FormSkill {
    /// The columns of a skills row.
    pub fn of(skill: i32, r: &Skills) -> Self {
        FormSkill {
            skill,
            restrict: r.restrict,
            states: [r.state1, r.state2, r.state3],
        }
    }
}

/// D2RUST's form test: `restrict` 0 passes only outside a form, 1
/// always, 2 only in a form that has one of the skill's states (the
/// first [`NO_STATE`] ends the list); other values pass.
pub fn form_ok(k: &FormSkill, u: &dyn UnitForm) -> bool {
    match k.restrict {
        0 => !u.in_form(),
        2 => {
            u.in_form()
                && k.states
                    .iter()
                    .take_while(|&&s| s != NO_STATE)
                    .any(|&s| u.has_state(s))
        }
        _ => true,
    }
}

/// The form test with the rules' answer first: a mod game's
/// [`crate::rules::SkillProvider::form_ok`], else [`form_ok`].
pub fn form_ok_with(rules: &crate::rules::Ruleset, k: &FormSkill, u: &dyn UnitForm) -> bool {
    rules
        .skills()
        .and_then(|p| p.form_ok(k, u))
        .unwrap_or_else(|| form_ok(k, u))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct U {
        form: bool,
        states: Vec<u16>,
    }
    impl UnitForm for U {
        fn in_form(&self) -> bool {
            self.form
        }
        fn has_state(&self, s: u16) -> bool {
            self.states.contains(&s)
        }
    }
    fn k(restrict: u8, states: [u16; 3]) -> FormSkill {
        FormSkill {
            skill: 1,
            restrict,
            states,
        }
    }

    #[test]
    fn form_ok_by_restrict() {
        let out = U {
            form: false,
            states: vec![],
        };
        let wolf = U {
            form: true,
            states: vec![139],
        };
        let bear = U {
            form: true,
            states: vec![140],
        };
        let none = [NO_STATE; 3];
        assert!(form_ok(&k(0, none), &out));
        assert!(!form_ok(&k(0, none), &wolf));
        assert!(form_ok(&k(1, none), &out) && form_ok(&k(1, none), &wolf));
        let s = [139, NO_STATE, NO_STATE];
        assert!(form_ok(&k(2, s), &wolf));
        assert!(!form_ok(&k(2, s), &bear));
        assert!(!form_ok(&k(2, s), &out));
        // The first empty cell ends the list.
        assert!(!form_ok(&k(2, [NO_STATE, 139, NO_STATE]), &wolf));
        assert!(form_ok(&k(9, none), &wolf));
    }
}
