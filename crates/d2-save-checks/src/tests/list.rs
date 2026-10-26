//! The check list: standard names and modes, config lines, a mod's own
//! checks, records in and out.

use d2_formats::d2s::D2s;

use super::*;
use crate::{CheckFail, ListError, Mode, SaveCheck, SaveCtx, STANDARD};

fn rich() -> D2s {
    let mut s = at_level(2);
    set_stat(&mut s, stat::GOLD, 999_999);
    s
}

#[test]
fn standard_list_has_every_check_in_order() {
    let l = list();
    let names: Vec<&str> = l.modes().iter().map(|&(n, _)| n).collect();
    let want: Vec<&str> = STANDARD.iter().map(|&(n, _)| n).collect();
    assert_eq!(names, want);
}

#[test]
fn modes_decide_refuse_or_flag() {
    let mut l = list();
    let v = l.run(&character(), &rich(), SaveInfo::default());
    assert!(!v.accepted() && v.audit());
    assert!(v.refused.iter().any(|f| f.check == "gold"));
    assert!(l.configure("check.gold", "flag").expect("a check key"));
    let v = l.run(&character(), &rich(), SaveInfo::default());
    assert!(v.accepted() && v.audit());
    assert!(v.flagged.iter().any(|f| f.check == "gold"));
    l.set_mode("gold", Mode::Off).expect("a check");
    let v = l.run(&character(), &rich(), SaveInfo::default());
    assert!(v.accepted() && !v.audit());
}

#[test]
fn config_lines() {
    let mut l = list();
    assert_eq!(l.configure("listen", "0.0.0.0"), Ok(false));
    assert_eq!(
        l.configure("check.gold", "maybe"),
        Err(ListError::BadMode {
            name: "gold".into(),
            value: "maybe".into()
        })
    );
    assert_eq!(
        l.configure("check.nonesuch", "off"),
        Err(ListError::Unknown("nonesuch".into()))
    );
    for m in [Mode::Refuse, Mode::Flag, Mode::Off] {
        assert_eq!(Mode::parse(m.as_str()), Some(m));
    }
}

/// A mod's rule: no character may carry gold at all.
struct NoGold;

impl SaveCheck for NoGold {
    fn check(&self, _: &D2s, after: &D2s, _: &SaveCtx) -> Result<(), CheckFail> {
        match crate::decoded::char_stat(after, stat::GOLD) {
            0 => Ok(()),
            g => Err(CheckFail::new(format!("{g} gold"))),
        }
    }
}

#[test]
fn a_mod_adds_replaces_and_removes_checks() {
    let mut l = list();
    let mut some_gold = at_level(2);
    set_stat(&mut some_gold, stat::GOLD, 5);
    assert!(l
        .run(&character(), &some_gold, SaveInfo::default())
        .accepted());
    l.add("no-gold", Mode::Refuse, Box::new(NoGold))
        .expect("a new name");
    assert_eq!(
        l.add("no-gold", Mode::Refuse, Box::new(NoGold)),
        Err(ListError::Exists("no-gold".into()))
    );
    let v = l.run(&character(), &some_gold, SaveInfo::default());
    assert!(failed(&v, "no-gold") && !v.accepted());
    // Replacing the standard gold rule with the mod's keeps its place and mode.
    assert!(l.remove("no-gold"));
    l.replace("gold", Box::new(NoGold))
        .expect("a standard check");
    let v = l.run(&character(), &some_gold, SaveInfo::default());
    assert!(v.refused.iter().any(|f| f.check == "gold"));
    assert!(l.remove("gold"));
    assert!(!l.remove("gold"));
    assert!(l.run(&character(), &rich(), SaveInfo::default()).accepted());
}

#[test]
fn records_round_trip_and_bad_records_are_refused() {
    let l = list();
    let t = l.tables();
    let write = |s: &D2s| d2s::write(s, &t.reading(true)).expect("writes");
    let first = with_items(at_level(2), &[ring(1, 0, SUFFIX_ACCURACY, &[(TOHIT, 7)])]);
    let bytes = write(&first);
    // A character's first save is checked against its creation.
    let v = l.run_records(None, &bytes, SaveInfo::default());
    assert!(v.refused.is_empty() && v.flagged.is_empty(), "{v:?}");
    let v = l.run_records(Some(&bytes), &bytes, SaveInfo::default());
    assert!(v.refused.is_empty() && v.flagged.is_empty(), "{v:?}");
    let v = l.run_records(Some(&bytes), &write(&rich()), SaveInfo::default());
    assert!(failed(&v, "gold"));
    let v = l.run_records(Some(&bytes), b"not a save", SaveInfo::default());
    assert_eq!(v.refused.len(), 1);
    assert_eq!(v.refused[0].check, "decode");
}

/// Abuse (plan N26): a good record with bytes appended past the reader's
/// limit is refused, not checked on its first part and stored whole.
#[test]
fn an_oversized_record_is_refused() {
    let l = list();
    let mut bytes = d2s::write(&at_level(2), &l.tables().reading(true)).expect("writes");
    assert!(l.run_records(None, &bytes, SaveInfo::default()).accepted());
    for len in [d2s::MAX_FILE + 1, 16 << 20] {
        bytes.resize(len, 0);
        for v in [
            l.run_records(None, &bytes, SaveInfo::default()),
            l.run_records(Some(&bytes), &bytes[..d2s::MAX_FILE], SaveInfo::default()),
        ] {
            assert_eq!(v.refused.len(), 1, "{len} bytes: {v:?}");
            assert_eq!(v.refused[0].check, "decode");
            assert!(v.refused[0].fail.reason.contains("over the"), "{v:?}");
        }
    }
}
