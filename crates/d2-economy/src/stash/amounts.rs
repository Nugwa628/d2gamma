//! Stack counts and gold in a stash: add and remove inside a request's
//! transaction.

use rusqlite::{params, OptionalExtension};

use crate::db::Act;
use crate::error::{Error, Refusal};

impl Act<'_> {
    pub fn stack_count(&self, account: i64, split: &str, code: &str) -> Result<i64, Error> {
        Ok(self
            .conn
            .query_row(
                "SELECT count FROM stash_stacks WHERE account = ?1 AND economy = ?2 AND code = ?3",
                params![account, split, code],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Adds `count` of `code`; `capped` refuses a count past `u32`.
    pub fn add_stack(
        &self,
        account: i64,
        split: &str,
        code: &str,
        count: u32,
        capped: bool,
    ) -> Result<(), Error> {
        let have = self.stack_count(account, split, code)?;
        let total = have + i64::from(count);
        if capped && total > i64::from(u32::MAX) {
            return Err(Refusal::StashFull.into());
        }
        self.exec(
            "INSERT INTO stash_stacks(account, economy, code, count) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(account, economy, code) DO UPDATE SET count = excluded.count",
            params![account, split, code, total],
        )?;
        Ok(())
    }

    /// Removes `count` of `code`; `short` is the refusal when the stash
    /// holds less.
    pub fn remove_stack(
        &self,
        account: i64,
        split: &str,
        code: &str,
        count: u32,
        short: Refusal,
    ) -> Result<(), Error> {
        let have = self.stack_count(account, split, code)?;
        let left = have - i64::from(count);
        if left < 0 {
            return Err(short.into());
        }
        if left == 0 {
            self.exec(
                "DELETE FROM stash_stacks WHERE account = ?1 AND economy = ?2 AND code = ?3",
                params![account, split, code],
            )?;
        } else {
            self.exec(
                "UPDATE stash_stacks SET count = ?4 WHERE account = ?1 AND economy = ?2 AND code = ?3",
                params![account, split, code, left],
            )?;
        }
        Ok(())
    }

    pub fn gold(&self, account: i64, split: &str) -> Result<i64, Error> {
        Ok(self
            .conn
            .query_row(
                "SELECT gold FROM stash_gold WHERE account = ?1 AND economy = ?2",
                params![account, split],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Adds gold; `capped` refuses a balance past `stash_gold_max`.
    pub fn add_gold(
        &self,
        account: i64,
        split: &str,
        gold: u64,
        capped: bool,
    ) -> Result<(), Error> {
        let gold = i64::try_from(gold).map_err(|_| Refusal::BadRequest)?;
        let total = self
            .gold(account, split)?
            .checked_add(gold)
            .ok_or(Refusal::StashFull)?;
        let max = i64::try_from(self.config.stash_gold_max).unwrap_or(i64::MAX);
        if capped && total > max {
            return Err(Refusal::StashFull.into());
        }
        self.set_gold(account, split, total)
    }

    /// Removes gold; `short` is the refusal when the stash holds less.
    pub fn remove_gold(
        &self,
        account: i64,
        split: &str,
        gold: u64,
        short: Refusal,
    ) -> Result<(), Error> {
        let gold = i64::try_from(gold).map_err(|_| short)?;
        let left = self.gold(account, split)? - gold;
        if left < 0 {
            return Err(short.into());
        }
        self.set_gold(account, split, left)
    }

    fn set_gold(&self, account: i64, split: &str, gold: i64) -> Result<(), Error> {
        self.exec(
            "INSERT INTO stash_gold(account, economy, gold) VALUES (?1, ?2, ?3) \
             ON CONFLICT(account, economy) DO UPDATE SET gold = excluded.gold",
            params![account, split, gold],
        )?;
        Ok(())
    }
}
