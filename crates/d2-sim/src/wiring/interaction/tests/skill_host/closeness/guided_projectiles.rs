//! Observe the actual homing mode and packed offset words.
use super::*;
#[test]
fn guided_projectile_report_observes_emission_and_homing_metadata() {
    let (_, all) = report();
    let (_, native, data) = all
        .iter()
        .find(|(n, _, _)| *n == "guided_projectile")
        .unwrap();
    assert_eq!(native, data);
    assert_eq!(native.results, [1, 1, 1]);
    assert_eq!(native.fired, 3);
    assert!(native.guided_modes > 0);
    assert_eq!(native.guided_offsets, native.guided_modes * 3);
}
