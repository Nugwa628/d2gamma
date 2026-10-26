#![no_main]
//! The game journal (d2-journal): the chunk body decoder, the chunk
//! decoder (one gzip member) and the journal file reader. The input is
//! read as a body, as gzip bytes, and compressed as one chunk so mutations
//! of the body reach the gzip path too. A body that decodes encodes back
//! to the same bytes.
use d2_journal::chunk::{compress, decode_chunk, read_journal};
use d2_journal::format::{decode_body, encode_body};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    d2_fuzz::arm("journal", data);
    if let Ok(b) = decode_body(data) {
        assert_eq!(encode_body(b.game, b.seq, &b.entries), data);
        let gz = compress(data);
        assert_eq!(decode_chunk(&gz).as_ref().ok(), Some(&b));
        let _ = read_journal(&gz);
    }
    let _ = decode_chunk(data);
    let _ = read_journal(data);
    let gz = compress(data);
    let _ = decode_chunk(&gz);
    // The same chunk twice: the second member is out of order.
    let mut two = gz.clone();
    two.extend_from_slice(&gz);
    let _ = read_journal(&two);
});
