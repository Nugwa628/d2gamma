//! Stored answers per request number (contracts 11.13): a repeated request
//! gets the first answer again and changes nothing.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Done, Error, Refusal, RowDone};
use crate::key::Scope;

/// A request's answer as stored: 0 for done (with what it made), or the
/// refusal's code; and for a request on several rows, each row's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Answer {
    pub code: u16,
    pub id: Option<i64>,
    pub rows: Vec<RowDone>,
}

impl Answer {
    pub fn of(outcome: &Result<(Option<i64>, Vec<RowDone>), Refusal>) -> Self {
        match outcome {
            Ok((id, rows)) => Answer {
                code: 0,
                id: *id,
                rows: rows.clone(),
            },
            Err(r) => Answer {
                code: r.code(),
                id: None,
                rows: Vec::new(),
            },
        }
    }

    /// The answer as a repeat.
    pub fn replay(self) -> Result<(Done, Vec<RowDone>), Error> {
        if self.code == 0 {
            let done = Done {
                id: self.id,
                repeated: true,
            };
            return Ok((done, self.rows));
        }
        match Refusal::from_code(self.code) {
            Some(r) => Err(r.into()),
            None => Err(Error::Db(format!("stored answer code {}", self.code))),
        }
    }
}

/// The rows' bytes: per row the ID, the code (0 done) and what it made.
fn encode_rows(rows: &[RowDone]) -> Option<Vec<u8>> {
    if rows.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(rows.len() * 19);
    for r in rows {
        out.extend_from_slice(&r.id.to_le_bytes());
        let (code, made) = match r.outcome {
            Ok(made) => (0, made),
            Err(refusal) => (refusal.code(), None),
        };
        out.extend_from_slice(&code.to_le_bytes());
        match made {
            Some(m) => {
                out.push(1);
                out.extend_from_slice(&m.to_le_bytes());
            }
            None => out.push(0),
        }
    }
    Some(out)
}

fn decode_rows(bytes: &[u8]) -> Result<Vec<RowDone>, Error> {
    let bad = || Error::Db("stored answer rows".into());
    let mut rows = Vec::new();
    let mut r = bytes;
    let mut take = |n: usize| -> Result<&[u8], Error> {
        if r.len() < n {
            return Err(bad());
        }
        let (a, b) = r.split_at(n);
        r = b;
        Ok(a)
    };
    while let Ok(id) = take(8) {
        let id = i64::from_le_bytes(id.try_into().map_err(|_| bad())?);
        let code = u16::from_le_bytes(take(2)?.try_into().map_err(|_| bad())?);
        let made = match take(1)?[0] {
            0 => None,
            _ => Some(i64::from_le_bytes(take(8)?.try_into().map_err(|_| bad())?)),
        };
        let outcome = match code {
            0 => Ok(made),
            c => Err(Refusal::from_code(c).ok_or_else(bad)?),
        };
        rows.push(RowDone { id, outcome });
    }
    Ok(rows)
}

pub(crate) fn find(conn: &Connection, scope: &str, req: i64) -> Result<Option<Answer>, Error> {
    let row: Option<(u16, Option<i64>, Option<Vec<u8>>)> = conn
        .query_row(
            "SELECT code, id, rows FROM requests WHERE scope = ?1 AND req = ?2",
            params![scope, req],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((code, id, rows)) = row else {
        return Ok(None);
    };
    let rows = match rows {
        Some(b) => decode_rows(&b)?,
        None => Vec::new(),
    };
    Ok(Some(Answer { code, id, rows }))
}

pub(crate) fn store(
    conn: &Connection,
    scope: &str,
    req: i64,
    answer: &Answer,
    now: i64,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO requests(scope, req, code, id, at, rows) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            scope,
            req,
            answer.code,
            answer.id,
            now,
            encode_rows(&answer.rows)
        ],
    )?;
    Ok(())
}

/// The highest request number stored for `scope` (0 for none): a client
/// numbers its next requests above it.
pub(crate) fn last(conn: &Connection, scope: &Scope) -> Result<u64, Error> {
    let n: Option<i64> = conn.query_row(
        "SELECT MAX(req) FROM requests WHERE scope = ?1",
        [scope.to_string()],
        |r| r.get(0),
    )?;
    // Request numbers are u64 on the wire; SQLite keeps the same bits.
    Ok(n.map_or(0, |n| n as u64))
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn rows_round_trip() {
        let rows = vec![
            RowDone {
                id: 7,
                outcome: Ok(None),
            },
            RowDone {
                id: -1,
                outcome: Ok(Some(i64::MAX)),
            },
            RowDone {
                id: 9,
                outcome: Err(Refusal::NotYours),
            },
        ];
        let bytes = encode_rows(&rows).unwrap();
        assert_eq!(decode_rows(&bytes).unwrap(), rows);
        assert_eq!(encode_rows(&[]), None);
        assert!(decode_rows(&bytes[..bytes.len() - 1]).is_err());
    }
}
