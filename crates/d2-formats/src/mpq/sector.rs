//! Decompressing one sector.
//!
//! A sector of a block with the IMPLODE flag is a bare PKWARE stream. A sector of a
//! block with the COMPRESS flag starts with a mask byte naming the stages it went
//! through ([`compression`]); the reader undoes them in the reverse of the order a
//! writer applies them. Stages the reader has no decoder for (sparse, LZMA) are refused
//! with a [`CodecError`], never guessed at.
//!
//! An ADPCM stream has no length of its own, and some writers end one before its
//! sector does: a sector of odd size (the end of a file of odd length) holds no half
//! sample, and the last sector of a file can hold a bare stream header. The game
//! plays such a sector, so the bytes the stream lacks read as silence (zero). Every
//! other stage must fill its sector exactly.

use super::{
    adpcm, compression, explode, flags, huffman, streams, BlockEntry, CodecError, SectorStats,
};

/// The largest sector an archive has (`0x200 << 15`). Bounds the padding of
/// an ADPCM sector, since `expected` is untrusted.
const MAX_SECTOR_SIZE: usize = 0x200 << 15;

/// Decompresses one sector to exactly `expected` bytes (an ADPCM sector that ends
/// early is padded with zeros).
pub(super) fn decompress_sector(
    block: &BlockEntry,
    sector: &[u8],
    expected: usize,
    stats: Option<&mut SectorStats>,
) -> Result<Vec<u8>, CodecError> {
    let out = if block.has(flags::IMPLODE) {
        if let Some(s) = stats {
            s.imploded += 1;
        }
        explode::explode(sector, expected)?
    } else {
        let (&mask, payload) = sector.split_first().ok_or(CodecError {
            codec: "sector",
            reason: "empty compressed sector",
        })?;
        if let Some(s) = stats {
            s.masks[usize::from(mask)] += 1;
        }
        let mut out = decompress_masked(mask, payload, expected)?;
        if mask & (compression::ADPCM_MONO | compression::ADPCM_STEREO) != 0
            && expected <= MAX_SECTOR_SIZE
        {
            out.resize(expected, 0);
        }
        out
    };
    if out.len() != expected {
        return Err(CodecError {
            codec: "sector",
            reason: "decompressed size does not match",
        });
    }
    Ok(out)
}

/// Runs the decoders a compression `mask` names over `payload`. Masks with an unknown
/// bit, both ADPCM bits, or a stage without a decoder (sparse, LZMA) are refused.
pub(super) fn decompress_masked(
    mask: u8,
    payload: &[u8],
    expected: usize,
) -> Result<Vec<u8>, CodecError> {
    use compression::*;
    let unsupported = |reason| CodecError {
        codec: "sector",
        reason,
    };
    if mask == LZMA {
        return Err(unsupported("LZMA compression is not supported"));
    }
    if mask & SPARSE != 0 {
        return Err(unsupported("sparse compression is not supported"));
    }
    let known = HUFFMAN | ZLIB | PKWARE | BZIP2 | ADPCM_MONO | ADPCM_STEREO;
    if mask & !known != 0 || mask & (ADPCM_MONO | ADPCM_STEREO) == ADPCM_MONO | ADPCM_STEREO {
        return Err(unsupported("invalid compression mask"));
    }

    // A writer applies ADPCM (mono or stereo), then Huffman, then zlib, PKWARE and
    // bzip2, so decoders run in the order bzip2, PKWARE, zlib, Huffman, ADPCM
    // (stereo, then mono); each consumes the previous output, and every output is
    // capped at `expected`.
    let mut buf: Option<Vec<u8>> = None;
    if mask & BZIP2 != 0 {
        buf = Some(streams::bzip2(buf.as_deref().unwrap_or(payload), expected)?);
    }
    if mask & PKWARE != 0 {
        buf = Some(explode::explode(
            buf.as_deref().unwrap_or(payload),
            expected,
        )?);
    }
    if mask & ZLIB != 0 {
        buf = Some(streams::zlib(buf.as_deref().unwrap_or(payload), expected)?);
    }
    if mask & HUFFMAN != 0 {
        buf = Some(huffman::decompress(
            buf.as_deref().unwrap_or(payload),
            expected,
        )?);
    }
    if mask & ADPCM_STEREO != 0 {
        buf = Some(adpcm::decompress(
            buf.as_deref().unwrap_or(payload),
            2,
            expected,
        ));
    }
    if mask & ADPCM_MONO != 0 {
        buf = Some(adpcm::decompress(
            buf.as_deref().unwrap_or(payload),
            1,
            expected,
        ));
    }
    buf.ok_or(unsupported("empty compression mask"))
}

#[cfg(test)]
mod tests {
    use super::super::writer;
    use super::*;

    #[test]
    fn masked_pkware_sector() {
        let mut sector = vec![compression::PKWARE];
        sector.extend_from_slice(&[0x00, 0x04, 0x82, 0x24, 0x25, 0x8F, 0x80, 0x7F]);
        let block = BlockEntry {
            file_pos: 0,
            compressed_size: 0,
            file_size: 13,
            flags: flags::EXISTS | flags::COMPRESS,
        };
        let out = decompress_sector(&block, &sector, 13, None).unwrap();
        assert_eq!(out, b"AIAIAIAIAIAIA");
    }

    #[test]
    fn size_mismatch_is_an_error() {
        let mut sector = vec![compression::PKWARE];
        sector.extend_from_slice(&[0x00, 0x04, 0x82, 0x24, 0x25, 0x8F, 0x80, 0x7F]);
        let block = BlockEntry {
            file_pos: 0,
            compressed_size: 0,
            file_size: 20,
            flags: flags::EXISTS | flags::COMPRESS,
        };
        assert!(decompress_sector(&block, &sector, 20, None).is_err());
    }

    fn sample() -> Vec<u8> {
        (0..4096u32).map(|i| (i * 13 % 97) as u8).collect()
    }

    fn compressed_block(size: usize) -> BlockEntry {
        BlockEntry {
            file_pos: 0,
            compressed_size: 0,
            file_size: size as u32,
            flags: flags::EXISTS | flags::COMPRESS,
        }
    }

    fn sector_of(mask: u8, payload: &[u8]) -> Vec<u8> {
        let mut sector = vec![mask];
        sector.extend_from_slice(payload);
        sector
    }

    #[test]
    fn zlib_and_bzip2_sectors() {
        let data = sample();
        let block = compressed_block(data.len());
        for (mask, payload) in [
            (compression::ZLIB, writer::zlib(&data)),
            (compression::BZIP2, writer::bzip2(&data)),
        ] {
            let sector = sector_of(mask, &payload);
            let mut stats = SectorStats::default();
            let out = decompress_sector(&block, &sector, data.len(), Some(&mut stats));
            assert_eq!(out.unwrap(), data, "mask {mask:#x}");
            assert_eq!(stats.masks[usize::from(mask)], 1);
            // A stream that decodes to fewer bytes than the sector holds is refused.
            assert!(decompress_sector(&block, &sector, data.len() + 1, None).is_err());
            // So is one that decodes to more.
            assert!(decompress_sector(&block, &sector, data.len() - 1, None).is_err());
            // And a damaged one.
            let cut = &sector[..sector.len() - 4];
            assert!(decompress_sector(&block, cut, data.len(), None).is_err());
        }
    }

    // Stages are undone in the reverse of the order a writer applies them:
    // ADPCM, Huffman, zlib, PKWARE, bzip2. Each pair below only decodes in
    // that order.
    #[test]
    fn combined_masks_decode_in_stage_order() {
        let data = sample();
        let pk = writer::Pkware::default();
        let huff = writer::huffman(0, &data);
        // Each stage's output is capped at the sector size.
        assert!(huff.len() <= data.len());
        let cases = [
            // zlib, then PKWARE over the zlib stream.
            (
                compression::ZLIB | compression::PKWARE,
                writer::implode(&writer::zlib(&data), pk),
            ),
            // Huffman, then zlib.
            (
                compression::HUFFMAN | compression::ZLIB,
                writer::zlib(&huff),
            ),
            // Huffman, then bzip2.
            (
                compression::HUFFMAN | compression::BZIP2,
                writer::bzip2(&huff),
            ),
            // PKWARE, then bzip2 (zlib with bzip2, 0x12, is the LZMA byte).
            (
                compression::PKWARE | compression::BZIP2,
                writer::bzip2(&writer::implode(&data, pk)),
            ),
        ];
        for (mask, payload) in cases {
            let out = decompress_masked(mask, &payload, data.len());
            assert_eq!(out.unwrap(), data, "mask {mask:#x}");
        }
        // ADPCM under zlib: zlib first, then the ADPCM stage on its output.
        let adpcm = [0x00, 0x00, 0x34, 0x12, 0xCC, 0xED, 0x80, 0x80, 0x07];
        let mask = compression::ZLIB | compression::ADPCM_MONO;
        let out = decompress_masked(mask, &writer::zlib(&adpcm), 64).unwrap();
        assert_eq!(out, adpcm::decompress(&adpcm, 1, 64));
    }

    #[test]
    fn rejects_unknown_masks() {
        for mask in [0x04, 0x12, 0x20, 0xC0] {
            assert!(
                decompress_masked(mask, &[0; 8], 8).is_err(),
                "mask {mask:#x}"
            );
        }
    }

    #[test]
    fn adpcm_mask_selects_channel_count() {
        // C = 1 for mask 0x40, 2 for mask 0x80; samples are i16 LE.
        let payload = [0x00, 0x00, 0x34, 0x12, 0xCC, 0xED, 0x80, 0x80, 0x07];
        let mono = decompress_masked(compression::ADPCM_MONO, &payload, 64).unwrap();
        assert_eq!(mono, adpcm::decompress(&payload, 1, 64));
        let stereo = decompress_masked(compression::ADPCM_STEREO, &payload, 64).unwrap();
        assert_eq!(stereo, adpcm::decompress(&payload, 2, 64));
        assert_ne!(mono, stereo);
        // Mono: 0x1234, then 0xEDCC read as an op stream; stereo: both
        // initial samples first.
        assert_eq!(mono[..2], 0x1234i16.to_le_bytes());
        assert_eq!(stereo[..4], [0x34, 0x12, 0xCC, 0xED]);
        // Stereo repeats: channel 0 then channel 1.
        assert_eq!(stereo[4..8], [0x34, 0x12, 0xCC, 0xED]);
    }

    #[test]
    fn adpcm_sector_that_ends_early_reads_as_silence() {
        // The payload decodes to 4 mono samples (8 bytes). A sector of 9 bytes
        // (the end of a file of odd length) gets its missing half sample as 0,
        // and a longer sector reads zeros after the stream ends.
        let payload = [0x00, 0x00, 0x34, 0x12, 0xCC, 0xED, 0x80, 0x80, 0x07];
        let samples = adpcm::decompress(&payload, 1, 64);
        assert_eq!(samples.len(), 8);
        let block = compressed_block(12);
        for mask in [
            compression::ADPCM_MONO,
            compression::ADPCM_MONO | compression::HUFFMAN,
            compression::ADPCM_STEREO,
        ] {
            let body = if mask & compression::HUFFMAN != 0 {
                writer::huffman(0, &payload)
            } else {
                payload.to_vec()
            };
            let sector = sector_of(mask, &body);
            let channels = if mask & compression::ADPCM_STEREO != 0 {
                2
            } else {
                1
            };
            let decoded = adpcm::decompress(&payload, channels, 64);
            for expected in [9, 12] {
                let out = decompress_sector(&block, &sector, expected, None).unwrap();
                assert_eq!(out.len(), expected, "mask {mask:#x}");
                // Whole samples only: the half sample of an odd sector is 0.
                let n = decoded.len().min(expected / 2 * 2);
                assert_eq!(out[..n], decoded[..n], "mask {mask:#x}");
                assert!(out[n..].iter().all(|&b| b == 0), "mask {mask:#x}");
            }
        }
        // A stream that is only its header (the last 4 bytes of a file).
        let sector = sector_of(compression::ADPCM_MONO, &[0x00, 0x04]);
        assert_eq!(decompress_sector(&block, &sector, 4, None).unwrap(), [0; 4]);
        // No sector is bigger than an archive allows.
        assert!(decompress_sector(&block, &sector, MAX_SECTOR_SIZE + 1, None).is_err());
        // A stream that fills its sector is exact.
        let sector = sector_of(compression::ADPCM_MONO, &payload);
        assert_eq!(
            decompress_sector(&block, &sector, 8, None).unwrap(),
            samples
        );
        // Without ADPCM a sector one byte short stays an error.
        let data = sample();
        let sector = sector_of(compression::ZLIB, &writer::zlib(&data[..9]));
        assert!(decompress_sector(&block, &sector, 10, None).is_err());
    }
}
