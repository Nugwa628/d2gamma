//! Granting blocks of realm item IDs to games (plan C3: `StartGame` and
//! `ItemIds`).

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{meta, set_meta};
use crate::{Block, Error, Registry};

impl Registry {
    /// Grants game `game` its next block of IDs: up to `block_size` IDs
    /// from the counter on, stopping short of the next ID registered
    /// already (first-sight IDs of old items sit anywhere in the range).
    /// The counter only rises, so no ID is granted twice. The caller sends
    /// the block (`StartGame`, `ItemIds`); the game's journal records it.
    pub fn grant(&self, conn: &Connection, game: u32, now: i64) -> Result<Block, Error> {
        let mut next: u64 = meta(conn, "next_id")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(u64::from(self.config.first_id));
        loop {
            if next > u64::from(u32::MAX) {
                return Err(Error::Exhausted);
            }
            let used: Option<i64> = conn.query_row(
                "SELECT MIN(id) FROM items WHERE id >= ?1",
                [next as i64],
                |r| r.get(0),
            )?;
            let used = used.map(|v| v as u64);
            let end = (next + u64::from(self.config.block_size))
                .min(used.unwrap_or(u64::MAX))
                .min(u64::from(u32::MAX) + 1);
            if end == next {
                // `next` is registered: go on after its run of used IDs.
                let after: i64 = conn.query_row(
                    "SELECT MIN(a.id) + 1 FROM items a WHERE a.id >= ?1
                     AND NOT EXISTS (SELECT 1 FROM items b WHERE b.id = a.id + 1)",
                    [next as i64],
                    |r| r.get(0),
                )?;
                next = after as u64;
                continue;
            }
            let block = Block {
                first: next as u32,
                count: (end - next) as u32,
            };
            conn.execute(
                "INSERT INTO item_blocks(first, count, game, granted) VALUES (?1, ?2, ?3, ?4)",
                params![block.first, block.count, game, now],
            )?;
            set_meta(conn, "next_id", &end.to_string())?;
            return Ok(block);
        }
    }

    /// The open blocks granted to `game`, oldest first.
    pub fn blocks(&self, conn: &Connection, game: u32) -> Result<Vec<Block>, Error> {
        let mut q = conn.prepare(
            "SELECT first, count FROM item_blocks WHERE game = ?1 AND open = 1 ORDER BY first",
        )?;
        let rows = q.query_map([game], |r| {
            Ok(Block {
                first: r.get(0)?,
                count: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `id` is in an open block of `game`.
    pub(crate) fn granted(conn: &Connection, game: u32, id: u32) -> Result<bool, Error> {
        // Blocks never overlap: the one that could hold `id` starts at the
        // highest `first` not above it.
        let b: Option<(u32, u32, u32, bool)> = conn
            .query_row(
                "SELECT first, count, game, open FROM item_blocks
                 WHERE first <= ?1 ORDER BY first DESC LIMIT 1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        Ok(b.is_some_and(|(first, count, g, open)| {
            open && g == game && Block { first, count }.contains(id)
        }))
    }

    /// `id` lies in any block ever granted.
    pub(crate) fn in_any_block(conn: &Connection, id: u32) -> Result<bool, Error> {
        let b: Option<(u32, u32)> = conn
            .query_row(
                "SELECT first, count FROM item_blocks WHERE first <= ?1 ORDER BY first DESC LIMIT 1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(b.is_some_and(|(first, count)| Block { first, count }.contains(id)))
    }
}
