//! The zlib (mask 0x02) and bzip2 (mask 0x10) sector stages.
//!
//! Both are whole library streams: zlib is a deflate stream with the two-byte zlib
//! header and the Adler-32 trailer, bzip2 a complete `BZh` stream. Like the other
//! decoders, each output is capped at `max_out`; a stream that would write more, ends
//! early, or is malformed is a [`CodecError`].

use super::CodecError;

/// One library decoder, fed the unread input each step.
trait Stream {
    /// Bytes read and written so far.
    fn totals(&self) -> (u64, u64);
    /// Decodes from `input` into the spare capacity of `out`; `true` at the end
    /// of the stream.
    fn step(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<bool, ()>;
}

impl Stream for flate2::Decompress {
    fn totals(&self) -> (u64, u64) {
        (self.total_in(), self.total_out())
    }

    fn step(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<bool, ()> {
        // Not `Finish`: that wants the whole output in one call, and the output
        // here grows between steps.
        self.decompress_vec(input, out, flate2::FlushDecompress::None)
            .map(|s| s == flate2::Status::StreamEnd)
            .map_err(drop)
    }
}

impl Stream for bzip2::Decompress {
    fn totals(&self) -> (u64, u64) {
        (self.total_in(), self.total_out())
    }

    fn step(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<bool, ()> {
        self.decompress_vec(input, out)
            .map(|s| s == bzip2::Status::StreamEnd)
            .map_err(drop)
    }
}

/// Output grows by at most this much per step, so a large cap is not allocated up front.
const CHUNK: usize = 64 * 1024;

fn run(
    codec: &'static str,
    mut stream: impl Stream,
    input: &[u8],
    max_out: usize,
) -> Result<Vec<u8>, CodecError> {
    let error = |reason| CodecError { codec, reason };
    // One byte of room past the cap tells a longer stream from one that fills it.
    let limit = max_out.saturating_add(1);
    let mut out = Vec::new();
    loop {
        if out.len() == out.capacity() {
            out.reserve_exact((limit - out.len()).min(CHUNK));
        }
        let before = stream.totals();
        let rest = &input[usize::try_from(before.0).map_or(input.len(), |n| n.min(input.len()))..];
        let end = stream
            .step(rest, &mut out)
            .map_err(|()| error("invalid stream"))?;
        if out.len() > max_out {
            return Err(error("output larger than expected"));
        }
        if end {
            return Ok(out);
        }
        if stream.totals() == before {
            return Err(error("stream ends early"));
        }
    }
}

/// Decompresses a zlib stream to at most `max_out` bytes.
pub(crate) fn zlib(input: &[u8], max_out: usize) -> Result<Vec<u8>, CodecError> {
    run("zlib", flate2::Decompress::new(true), input, max_out)
}

/// Decompresses a bzip2 stream to at most `max_out` bytes.
pub(crate) fn bzip2(input: &[u8], max_out: usize) -> Result<Vec<u8>, CodecError> {
    run("bzip2", bzip2::Decompress::new(false), input, max_out)
}

#[cfg(test)]
mod tests {
    use super::super::writer::{bzip2 as bzip2_of, zlib as zlib_of};
    use super::*;

    fn sample() -> Vec<u8> {
        (0..4096u32).map(|i| (i * 7 % 61) as u8).collect()
    }

    #[test]
    fn zlib_round_trip() {
        let data = sample();
        let c = zlib_of(&data);
        assert!(c.len() < data.len());
        assert_eq!(zlib(&c, data.len()).unwrap(), data);
        // A larger cap is fine: the caller checks the exact size.
        assert_eq!(zlib(&c, data.len() + 100).unwrap(), data);
    }

    #[test]
    fn zlib_refuses_bad_streams() {
        let data = sample();
        let c = zlib_of(&data);
        assert!(zlib(&c, data.len() - 1).is_err(), "longer than the cap");
        assert!(zlib(&c[..c.len() / 2], data.len()).is_err(), "truncated");
        assert!(zlib(&c[..c.len() - 1], data.len()).is_err(), "no checksum");
        let mut bad = c.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF;
        assert!(zlib(&bad, data.len()).is_err(), "checksum mismatch");
        assert!(zlib(&[], 16).is_err());
        assert!(zlib(&[0xFF; 32], 16).is_err());
    }

    // The cap only bounds the output; it is not allocated up front.
    #[test]
    fn huge_cap_is_not_preallocated() {
        let data = sample();
        assert_eq!(zlib(&zlib_of(&data), usize::MAX).unwrap(), data);
        assert_eq!(bzip2(&bzip2_of(&data), usize::MAX).unwrap(), data);
        let big = vec![0u8; 3 * CHUNK + 5];
        assert_eq!(zlib(&zlib_of(&big), big.len()).unwrap(), big);
    }

    #[test]
    fn bzip2_round_trip() {
        let data = sample();
        let c = bzip2_of(&data);
        assert!(c.starts_with(b"BZh"));
        assert_eq!(bzip2(&c, data.len()).unwrap(), data);
    }

    #[test]
    fn bzip2_refuses_bad_streams() {
        let data = sample();
        let c = bzip2_of(&data);
        assert!(bzip2(&c, data.len() - 1).is_err(), "longer than the cap");
        assert!(bzip2(&c[..c.len() / 2], data.len()).is_err(), "truncated");
        assert!(bzip2(&[], 16).is_err());
        assert!(bzip2(&[0xFF; 32], 16).is_err());
    }
}
