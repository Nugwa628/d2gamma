//! Abuse tests (plan N26): journal chunks built to cost the main server
//! memory. A game server is not trusted (player hosts are normal), so a
//! chunk's gzip member and its body are hostile input.

use d2_journal::chunk::{compress, decode_chunk, read_journal, JournalError, MAX_BODY};
use d2_journal::format::{kind, Entry, FormatError, MAX_RECORDS, VERSION};

/// A chunk body of game 1, chunk `seq`, holding `n` frame records (9 bytes
/// each, the smallest record).
fn frames(seq: u32, n: usize) -> Vec<u8> {
    let mut b = vec![VERSION];
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&seq.to_le_bytes());
    for _ in 0..n {
        b.extend_from_slice(&0u32.to_le_bytes());
        b.push(kind::FRAME);
        b.extend_from_slice(&1000u32.to_le_bytes());
    }
    b
}

#[test]
fn a_gzip_bomb_of_tiny_records_is_refused() {
    // One record over the cap: a few kilobytes of gzip.
    let gz = compress(&frames(0, MAX_RECORDS + 1));
    assert!(gz.len() < 64 << 10, "{} bytes of gzip", gz.len());
    assert_eq!(
        decode_chunk(&gz),
        Err(JournalError::Format {
            chunk: 0,
            source: FormatError::TooManyRecords
        })
    );
    // At the cap it reads, and what it decodes to stays bounded.
    let ok = decode_chunk(&compress(&frames(0, MAX_RECORDS))).expect("at the cap");
    assert_eq!(ok.entries.len(), MAX_RECORDS);
    let decoded = MAX_RECORDS * std::mem::size_of::<Entry>();
    assert!(
        decoded <= 48 << 20,
        "a full chunk decodes to {decoded} bytes"
    );
}

#[test]
fn a_body_over_the_limit_is_refused_before_it_is_all_inflated() {
    // MAX_BODY + 1 bytes of zeros after a valid header: gzip shrinks it to
    // about 64 KiB; the reader stops one byte past the limit.
    let mut body = frames(0, 0);
    body.resize(MAX_BODY + 1, 0);
    let gz = compress(&body);
    assert!(gz.len() < 256 << 10, "{} bytes of gzip", gz.len());
    assert_eq!(decode_chunk(&gz), Err(JournalError::TooLarge { chunk: 0 }));
    assert_eq!(read_journal(&gz), Err(JournalError::TooLarge { chunk: 0 }));
}

#[test]
fn huge_length_fields_are_refused_without_allocating() {
    // Every length field is a u8 or u16; one that runs past the body ends
    // the chunk as truncated.
    for (k, fields) in [
        // Intent: client, system, then a bytes16 claiming 65535 bytes.
        (kind::INTENT, vec![1, 0, 0, 0xFF, 0xFF]),
        // Start: a str8 version claiming 255 bytes.
        (kind::START, vec![0xFF]),
    ] {
        let mut b = frames(0, 0);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.push(k);
        b.extend_from_slice(&fields);
        b.extend_from_slice(&[0; 16]);
        assert_eq!(
            decode_chunk(&compress(&b)),
            Err(JournalError::Format {
                chunk: 0,
                source: FormatError::Truncated
            }),
            "kind {k}"
        );
    }
}
