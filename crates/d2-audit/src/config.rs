//! The audit settings: the main server's config keys of contracts 11.8, with
//! the plan's proposed defaults (open questions 15 and 16) as constants.

/// Share of official games spot-checked, in percent (plan C4, proposed).
pub const SPOT_OFFICIAL_PCT: u32 = 1;
/// Share of the other players' characters in player-hosted games
/// spot-checked, in percent (plan C4, proposed).
pub const SPOT_HOSTED_PCT: u32 = 5;
/// Days a journal is kept when nothing holds it (plan C4, proposed).
pub const JOURNAL_KEEP_DAYS: u32 = 7;

/// What a failed audit of a player-hosted game does to the characters it
/// affected (open question 15).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RollbackMode {
    /// The rollbacks wait for the owner (`d2rust-audit rollback ID`).
    #[default]
    OwnerConfirms,
    /// The rollbacks are applied as soon as the audit fails.
    Automatic,
}

/// The audit settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditConfig {
    /// The automatic triggers (spot checks, the host's own character) are
    /// on. On-demand audits, reports and flagged saves queue either way.
    pub on: bool,
    pub spot_official_pct: u32,
    pub spot_hosted_pct: u32,
    pub journal_keep_days: u32,
    pub rollback: RollbackMode,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            on: true,
            spot_official_pct: SPOT_OFFICIAL_PCT,
            spot_hosted_pct: SPOT_HOSTED_PCT,
            journal_keep_days: JOURNAL_KEEP_DAYS,
            rollback: RollbackMode::OwnerConfirms,
        }
    }
}

impl AuditConfig {
    /// Applies one `key = value` line of the main server's config. Returns
    /// `Ok(false)` for a key that isn't an audit key (the caller reads it),
    /// and an error for an audit key with a bad value.
    pub fn set(&mut self, key: &str, value: &str) -> Result<bool, String> {
        let pct = |v: &str| match v.parse::<u32>() {
            Ok(n) if n <= 100 => Ok(n),
            _ => Err(format!("{key}: a percentage 0-100, not `{v}`")),
        };
        match key {
            "audits" => {
                self.on = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(format!("audits: `on` or `off`, not `{value}`")),
                }
            }
            "spot_official_pct" => self.spot_official_pct = pct(value)?,
            "spot_hosted_pct" => self.spot_hosted_pct = pct(value)?,
            "journal_keep_days" => {
                self.journal_keep_days = value
                    .parse()
                    .map_err(|_| format!("journal_keep_days: a number of days, not `{value}`"))?
            }
            "audit_rollback" => {
                self.rollback = match value {
                    "confirm" => RollbackMode::OwnerConfirms,
                    "automatic" => RollbackMode::Automatic,
                    _ => {
                        return Err(format!(
                            "audit_rollback: `confirm` or `automatic`, not `{value}`"
                        ))
                    }
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The audit keys of a whole `key = value` config text; other keys,
    /// blank lines and `#` comments are skipped.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut cfg = Self::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            cfg.set(k.trim(), v.trim())
                .map_err(|e| format!("line {}: {e}", n + 1))?;
        }
        Ok(cfg)
    }

    /// The journal window in milliseconds.
    pub fn keep_ms(&self) -> u64 {
        u64::from(self.journal_keep_days) * 86_400_000
    }
}
