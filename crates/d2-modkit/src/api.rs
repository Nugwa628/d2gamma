//! The modding API version and which versions D2Gamma still runs.
//!
//! A mod says in its manifest which modding API version it was made for
//! (`api = 1`). D2Gamma has one current version, and keeps older ones
//! working for a fixed window: a version stays supported until
//! [`SUPPORT_MONTHS`] months after the version that replaced it was
//! released. A mod newer than the game, or older than the window, does not
//! load; [`check_api`] says which, with a message the player can act on.
//!
//! The release history below is the only thing to change when the API gets
//! a new version: add a line with the version and its release date, and
//! bump [`CURRENT_API`] to match (a test checks they agree).

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// The modding API version this build of D2Gamma implements.
pub const CURRENT_API: u32 = 1;

/// How long a superseded API version keeps working, in months.
pub const SUPPORT_MONTHS: u32 = 12;

/// A calendar date (UTC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub const fn new(year: i32, month: u32, day: u32) -> Date {
        Date { year, month, day }
    }

    /// The date `days` days after 1970-01-01.
    pub fn from_unix_days(days: i64) -> Date {
        // Civil-from-days over 400-year eras.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Date { year, month, day }
    }

    /// Today, by the system clock.
    pub fn today() -> Date {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Date::from_unix_days((secs / 86_400) as i64)
    }

    /// This date `months` months later; a day past the end of the target
    /// month moves to that month's last day.
    pub fn plus_months(self, months: u32) -> Date {
        let total = self.year * 12 + (self.month as i32 - 1) + months as i32;
        let year = total.div_euclid(12);
        let month = total.rem_euclid(12) as u32 + 1;
        Date {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

impl fmt::Display for Date {
    /// "29 October 2026".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const MONTHS: [&str; 12] = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        let name = MONTHS
            .get((self.month as usize).wrapping_sub(1))
            .copied()
            .unwrap_or("?");
        write!(f, "{} {} {}", self.day, name, self.year)
    }
}

/// One modding API version and when it was released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiRelease {
    pub version: u32,
    pub released: Date,
}

/// Every API version released so far, oldest first.
pub const API_RELEASES: &[ApiRelease] = &[ApiRelease {
    version: 1,
    released: Date::new(2026, 10, 29),
}];

/// What the version check says about a mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiCheck {
    /// The mod's API version is supported.
    Ok,
    /// The mod is too old: the message tells the player to get an updated
    /// mod from its author.
    NeedsUpdate(String),
    /// The mod needs a newer D2Gamma: the message tells the player to
    /// update the game.
    TooNew(String),
}

/// Checks a mod's API version against this build, as of `today`.
pub fn check_api(mod_api: u32, today: Date) -> ApiCheck {
    check_against(API_RELEASES, mod_api, today)
}

/// [`check_api`] with the system clock.
pub fn check_api_now(mod_api: u32) -> ApiCheck {
    check_api(mod_api, Date::today())
}

/// The oldest API version still supported on `today`.
pub fn oldest_supported(today: Date) -> u32 {
    oldest_in(API_RELEASES, today)
}

/// The last day an API version works, or `None` for the current one (it
/// has no end date yet).
pub fn supported_until(version: u32) -> Option<Date> {
    until_in(API_RELEASES, version)
}

/// A warning for a mod that still loads but targets a superseded API
/// version: when it stops working and what to do. `None` for a mod on the
/// current version or one that does not load at all.
pub fn removal_warning(mod_api: u32, today: Date) -> Option<String> {
    warning_in(API_RELEASES, mod_api, today)
}

fn until_in(releases: &[ApiRelease], version: u32) -> Option<Date> {
    releases
        .iter()
        .find(|r| r.version > version)
        .map(|next| next.released.plus_months(SUPPORT_MONTHS))
}

fn current_in(releases: &[ApiRelease]) -> u32 {
    releases.last().map_or(0, |r| r.version)
}

fn oldest_in(releases: &[ApiRelease], today: Date) -> u32 {
    // A version works through its end date, so the oldest supported is the
    // first one whose end date is not before today.
    releases
        .iter()
        .map(|r| r.version)
        .find(|&v| until_in(releases, v).is_none_or(|end| end >= today))
        .unwrap_or_else(|| current_in(releases))
}

fn check_against(releases: &[ApiRelease], mod_api: u32, today: Date) -> ApiCheck {
    let current = current_in(releases);
    if mod_api > current {
        return ApiCheck::TooNew(format!(
            "This mod was made for a newer D2Gamma (it needs modding API version {mod_api}; \
             this D2Gamma has version {current}). Update D2Gamma, then try again."
        ));
    }
    let oldest = oldest_in(releases, today);
    if mod_api < oldest {
        let range = if oldest == current {
            format!("version {current}")
        } else {
            format!("versions {oldest} to {current}")
        };
        return ApiCheck::NeedsUpdate(format!(
            "This mod needs updating: it was made for modding API version {mod_api}, \
             which D2Gamma no longer supports (it runs mods made for {range}). \
             Ask the mod's author for an updated version."
        ));
    }
    ApiCheck::Ok
}

fn warning_in(releases: &[ApiRelease], mod_api: u32, today: Date) -> Option<String> {
    if !matches!(check_against(releases, mod_api, today), ApiCheck::Ok) {
        return None;
    }
    let end = until_in(releases, mod_api)?;
    Some(format!(
        "This mod was made for an older modding API (version {mod_api}) and will stop working \
         after {end}. Ask the mod's author to update it."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 in January 2026, 2 in June 2026, 3 in March 2027.
    const HISTORY: &[ApiRelease] = &[
        ApiRelease {
            version: 1,
            released: Date::new(2026, 1, 10),
        },
        ApiRelease {
            version: 2,
            released: Date::new(2026, 6, 1),
        },
        ApiRelease {
            version: 3,
            released: Date::new(2027, 3, 1),
        },
    ];

    #[test]
    fn current_matches_the_history() {
        assert_eq!(current_in(API_RELEASES), CURRENT_API);
        assert!(API_RELEASES
            .windows(2)
            .all(|w| w[0].version < w[1].version && w[0].released <= w[1].released));
    }

    #[test]
    fn current_version_is_ok_today() {
        assert_eq!(
            check_api(CURRENT_API, Date::new(2026, 10, 29)),
            ApiCheck::Ok
        );
        assert_eq!(check_api(CURRENT_API, Date::new(2040, 1, 1)), ApiCheck::Ok);
    }

    #[test]
    fn a_newer_mod_is_too_new() {
        let c = check_api(CURRENT_API + 1, Date::new(2026, 10, 29));
        let ApiCheck::TooNew(msg) = c else {
            panic!("expected TooNew, got {c:?}")
        };
        assert!(msg.contains("Update D2Gamma"), "{msg}");
        assert!(msg.contains(&(CURRENT_API + 1).to_string()), "{msg}");
    }

    #[test]
    fn version_zero_needs_updating() {
        assert!(matches!(
            check_api(0, Date::new(2026, 10, 29)),
            ApiCheck::NeedsUpdate(_)
        ));
    }

    #[test]
    fn old_versions_work_for_twelve_months_after_the_next_release() {
        // Version 1 was replaced on 2026-06-01, so it works through 2027-06-01.
        let at = |y, m, d| check_against(HISTORY, 1, Date::new(y, m, d));
        assert_eq!(at(2026, 6, 1), ApiCheck::Ok);
        assert_eq!(at(2027, 6, 1), ApiCheck::Ok);
        let ApiCheck::NeedsUpdate(msg) = at(2027, 6, 2) else {
            panic!("version 1 should be gone after its window")
        };
        assert!(
            msg.contains("needs updating") && msg.contains("author"),
            "{msg}"
        );
        // Version 2 is still inside its window when 3 ships and for a year after.
        assert_eq!(
            check_against(HISTORY, 2, Date::new(2027, 6, 2)),
            ApiCheck::Ok
        );
        assert_eq!(
            check_against(HISTORY, 2, Date::new(2028, 3, 1)),
            ApiCheck::Ok
        );
        assert!(matches!(
            check_against(HISTORY, 2, Date::new(2028, 3, 2)),
            ApiCheck::NeedsUpdate(_)
        ));
        // The newest never expires.
        assert_eq!(
            check_against(HISTORY, 3, Date::new(2035, 1, 1)),
            ApiCheck::Ok
        );
        assert!(matches!(
            check_against(HISTORY, 4, Date::new(2027, 1, 1)),
            ApiCheck::TooNew(_)
        ));
    }

    #[test]
    fn the_message_names_the_supported_range() {
        let ApiCheck::NeedsUpdate(msg) = check_against(HISTORY, 1, Date::new(2027, 7, 1)) else {
            panic!()
        };
        assert!(msg.contains("versions 2 to 3"), "{msg}");
    }

    #[test]
    fn oldest_supported_moves_with_the_date() {
        assert_eq!(oldest_in(HISTORY, Date::new(2026, 3, 1)), 1);
        assert_eq!(oldest_in(HISTORY, Date::new(2027, 7, 1)), 2);
        assert_eq!(oldest_in(HISTORY, Date::new(2028, 4, 1)), 3);
    }

    #[test]
    fn warns_before_removal() {
        let w = warning_in(HISTORY, 1, Date::new(2027, 1, 1)).unwrap();
        assert!(w.contains("1 June 2027"), "{w}");
        assert!(warning_in(HISTORY, 3, Date::new(2027, 1, 1)).is_none());
        // Already removed: the check says so, no warning on top.
        assert!(warning_in(HISTORY, 1, Date::new(2028, 1, 1)).is_none());
        assert_eq!(until_in(HISTORY, 3), None);
        assert_eq!(until_in(HISTORY, 2), Some(Date::new(2028, 3, 1)));
    }

    #[test]
    fn dates_add_months_and_convert() {
        assert_eq!(
            Date::new(2026, 10, 29).plus_months(12),
            Date::new(2027, 10, 29)
        );
        assert_eq!(
            Date::new(2027, 1, 31).plus_months(1),
            Date::new(2027, 2, 28)
        );
        assert_eq!(
            Date::new(2023, 2, 28).plus_months(12),
            Date::new(2024, 2, 28)
        );
        assert_eq!(
            Date::new(2023, 12, 15).plus_months(2),
            Date::new(2024, 2, 15)
        );
        assert_eq!(Date::from_unix_days(0), Date::new(1970, 1, 1));
        assert_eq!(Date::from_unix_days(20_025), Date::new(2024, 10, 29));
        assert_eq!(Date::from_unix_days(11_017), Date::new(2000, 3, 1));
        assert_eq!(Date::new(2026, 10, 29).to_string(), "29 October 2026");
    }
}
