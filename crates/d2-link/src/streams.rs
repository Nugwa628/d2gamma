//! The record and journal streams (contracts 11.2): each character record
//! (a `SeatOffer`'s or a `Save`'s) and each journal chunk travels on a
//! unidirectional stream of its own, after a header:
//!
//! ```text
//! u8  what     1 record, 2 journal chunk
//! u64 lease    (record)  or  u32 game (journal chunk)
//! u32 seq
//! u32 length   then `length` bytes: at most 256 KiB for a record,
//!              1 MiB for a chunk; nothing after them
//! ```

use d2_net::NetError;
use d2_quic::{QuicConn, UniRecv};

/// The largest character record.
pub const MAX_RECORD: usize = 256 * 1024;

/// The largest journal chunk.
pub const MAX_JOURNAL_CHUNK: usize = 1024 * 1024;

/// The longest header.
pub const MAX_HEADER: usize = 1 + 8 + 4 + 4;

const RECORD: u8 = 1;
const JOURNAL: u8 = 2;

/// What a stream carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// A character record, for the seat lease `lease`.
    Record { lease: u64 },
    /// A journal chunk of game `game`.
    Journal { game: u32 },
}

impl What {
    /// The most bytes this kind of stream carries.
    pub fn max(self) -> usize {
        match self {
            What::Record { .. } => MAX_RECORD,
            What::Journal { .. } => MAX_JOURNAL_CHUNK,
        }
    }
}

/// One stream's contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carried {
    pub what: What,
    pub seq: u32,
    pub bytes: Vec<u8>,
}

/// The stream's bytes: the header, then `bytes`. Malformed when `bytes` is
/// over its kind's limit.
pub fn encode(what: What, seq: u32, bytes: &[u8]) -> Result<Vec<u8>, NetError> {
    if bytes.len() > what.max() {
        return Err(NetError::Malformed);
    }
    let mut out = Vec::with_capacity(MAX_HEADER + bytes.len());
    match what {
        What::Record { lease } => {
            out.push(RECORD);
            out.extend_from_slice(&lease.to_le_bytes());
        }
        What::Journal { game } => {
            out.push(JOURNAL);
            out.extend_from_slice(&game.to_le_bytes());
        }
    }
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(out)
}

fn take<const N: usize>(b: &mut &[u8]) -> Result<[u8; N], NetError> {
    if b.len() < N {
        return Err(NetError::Malformed);
    }
    let (head, rest) = b.split_at(N);
    *b = rest;
    Ok(head.try_into().expect("N bytes"))
}

/// A whole stream back: Malformed for an unknown kind, a length over the
/// limit, or bytes missing or left over.
pub fn decode(mut b: &[u8]) -> Result<Carried, NetError> {
    let [kind] = take::<1>(&mut b)?;
    let what = match kind {
        RECORD => What::Record {
            lease: u64::from_le_bytes(take(&mut b)?),
        },
        JOURNAL => What::Journal {
            game: u32::from_le_bytes(take(&mut b)?),
        },
        _ => return Err(NetError::Malformed),
    };
    let seq = u32::from_le_bytes(take(&mut b)?);
    let len = u32::from_le_bytes(take(&mut b)?) as usize;
    if len > what.max() || b.len() != len {
        return Err(NetError::Malformed);
    }
    Ok(Carried {
        what,
        seq,
        bytes: b.to_vec(),
    })
}

/// Sends one record or chunk on its own stream of `conn`.
pub async fn send(conn: &QuicConn, what: What, seq: u32, bytes: &[u8]) -> Result<(), NetError> {
    conn.send_uni(&encode(what, seq, bytes)?).await
}

/// Reads one stream the other end opened.
pub async fn recv(stream: UniRecv) -> Result<Carried, NetError> {
    let all = stream.read_all(MAX_HEADER + MAX_JOURNAL_CHUNK).await?;
    decode(&all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_chunks_round_trip_with_their_header() {
        let r = encode(What::Record { lease: 77 }, 3, b"save").unwrap();
        let mut want = vec![1];
        want.extend_from_slice(&77u64.to_le_bytes());
        want.extend_from_slice(&3u32.to_le_bytes());
        want.extend_from_slice(&4u32.to_le_bytes());
        want.extend_from_slice(b"save");
        assert_eq!(r, want);
        assert_eq!(
            decode(&r).unwrap(),
            Carried {
                what: What::Record { lease: 77 },
                seq: 3,
                bytes: b"save".to_vec()
            }
        );
        let j = encode(What::Journal { game: 9 }, 1, &[5; 1000]).unwrap();
        assert_eq!(j.len(), 1 + 4 + 4 + 4 + 1000);
        assert_eq!(decode(&j).unwrap().what, What::Journal { game: 9 });
    }

    #[test]
    fn limits_and_lengths_are_enforced() {
        let big = vec![0; MAX_RECORD + 1];
        assert_eq!(
            encode(What::Record { lease: 1 }, 0, &big),
            Err(NetError::Malformed)
        );
        assert!(encode(What::Journal { game: 1 }, 0, &big).is_ok());
        // A record header claiming more than a record may hold.
        let mut r = encode(What::Journal { game: 1 }, 0, &big).unwrap();
        r[0] = 1;
        assert_eq!(decode(&r), Err(NetError::Malformed));
        let ok = encode(What::Record { lease: 1 }, 0, b"abc").unwrap();
        assert_eq!(decode(&ok[..ok.len() - 1]), Err(NetError::Malformed));
        let mut extra = ok.clone();
        extra.push(0);
        assert_eq!(decode(&extra), Err(NetError::Malformed));
        let mut kind = ok;
        kind[0] = 3;
        assert_eq!(decode(&kind), Err(NetError::Malformed));
        assert_eq!(decode(&[]), Err(NetError::Malformed));
    }
}
