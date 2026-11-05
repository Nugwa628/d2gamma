//! The sound archive's two cursor index files end in `.wav` but are not
//! sounds: a conversion skips them as never loaded and still completes.

use std::fs;
use std::path::{Path, PathBuf};

use d2_convert::kinds::{Excel, Native};
use d2_convert::{convert, Kind, Options};
use d2_formats::mpq::writer::MpqWriter;
use d2_native::asset::AssetKind;

fn scratch(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// A 16-bit mono PCM RIFF/WAVE file holding `samples`.
fn pcm_wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut o = Vec::new();
    o.extend_from_slice(b"RIFF");
    o.extend_from_slice(&(36 + data_len).to_le_bytes());
    o.extend_from_slice(b"WAVEfmt ");
    o.extend_from_slice(&16u32.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes()); // PCM
    o.extend_from_slice(&1u16.to_le_bytes()); // mono
    o.extend_from_slice(&22_050u32.to_le_bytes());
    o.extend_from_slice(&44_100u32.to_le_bytes());
    o.extend_from_slice(&2u16.to_le_bytes());
    o.extend_from_slice(&16u16.to_le_bytes());
    o.extend_from_slice(b"data");
    o.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        o.extend_from_slice(&s.to_le_bytes());
    }
    o
}

/// The nine required archives, empty but for `d2sfx.mpq`: one real
/// sound and the two cursor index files (plain bytes, no RIFF header).
fn install(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    for name in d2_convert::convert::REQUIRED_ARCHIVES {
        let mut w = MpqWriter::new();
        if name == "d2sfx.mpq" {
            w = w.with_listfile();
            w.add_file(
                r"data\global\sfx\cursor\button.wav",
                pcm_wav(&[0, 120, -120, 0]),
            );
            w.add_file(
                r"data\global\sfx\cursor\curindx.wav",
                b"\x07\x00\x00\x00\x01\x02\x03\x04".to_vec(),
            );
            w.add_file(
                r"data\global\sfx\cursor\wavindx.wav",
                b"\x03\x00\x00\x00\x09\x08".to_vec(),
            );
        }
        w.write(dir.join(name)).unwrap();
    }
}

#[test]
fn cursor_index_files_are_skipped_and_the_run_completes() {
    let dir = scratch("index_wavs");
    install(&dir.join("game"));
    let kinds: Vec<Box<dyn Kind>> = vec![Box::new(Excel), Box::new(Native(AssetKind::Wav))];
    let out = dir.join("native");
    let mut o = Options::new(dir.join("game"), &out);
    o.converter_commit = "test".into();
    let s = convert(&o, &kinds).unwrap();
    assert!(s.failures.is_empty(), "{:?}", s.failures);
    assert_eq!(s.exit_code(), 0);
    let m = s.manifest.unwrap();
    assert!(m.complete);
    let c = m.counts["wav"];
    assert_eq!((c.converted, c.failed), (1, 0));
    assert!(c.skipped >= 2, "{c:?}");
    // The two index files are listed in the report with the reason.
    let report = fs::read_to_string(out.join("report.txt")).unwrap();
    for f in ["curindx.wav", "wavindx.wav"] {
        assert!(report.contains(f), "{report}");
    }
    assert!(report.contains("an index file"), "{report}");
    let files = fs::read_to_string(out.join("files.tsv")).unwrap();
    assert!(!files.contains("failed:"), "{files}");
}
