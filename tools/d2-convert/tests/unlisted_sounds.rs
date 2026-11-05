//! The sound archive's own `(listfile)` names almost none of its sounds;
//! the converter finds them through the file names in `sounds.txt` (sound
//! effects, speech and music, each folder by id range) and converts them.

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

/// `sounds.txt` with a sound effect (id 1), a speech line (id 3000) and a
/// song (id 4657); every other row names no file.
fn sounds_txt() -> Vec<u8> {
    let mut s = String::from("Sound\tIndex\tFileName\r\n");
    for i in 0..4660 {
        let file = match i {
            1 => "cursor\\pass.wav",
            3000 => "npc\\akara\\hi.wav",
            4657 => "act1\\town1.wav",
            _ => "",
        };
        s.push_str(&format!("r{i}\t{i}\t{file}\r\n"));
    }
    s.into_bytes()
}

/// Sounds in archives with no `(listfile)` (`d2sfx.mpq` also holds a block
/// no table names); `sounds.txt` in `d2data.mpq`.
fn install(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    for name in d2_convert::convert::REQUIRED_ARCHIVES {
        let mut w = MpqWriter::new();
        match name {
            "d2data.mpq" => {
                w = w.with_listfile();
                w.add_file(r"data\global\excel\sounds.txt", sounds_txt());
            }
            "d2sfx.mpq" => {
                w.add_file(r"data\global\sfx\cursor\pass.wav", pcm_wav(&[0, 9, -9]));
                w.add_file(r"data\global\sfx\unused\nobody.wav", pcm_wav(&[1]));
            }
            "d2speech.mpq" => {
                w.add_file(r"data\local\sfx\npc\akara\hi.wav", pcm_wav(&[5, -5]));
            }
            "d2music.mpq" => {
                w.add_file(r"data\global\music\act1\town1.wav", pcm_wav(&[7; 4]));
            }
            _ => {}
        }
        w.write(dir.join(name)).unwrap();
    }
}

#[test]
fn sounds_named_only_by_sounds_txt_are_converted() {
    let dir = scratch("unlisted_sounds");
    install(&dir.join("game"));
    let kinds: Vec<Box<dyn Kind>> = vec![Box::new(Excel), Box::new(Native(AssetKind::Wav))];
    let out = dir.join("native");
    let mut o = Options::new(dir.join("game"), &out);
    o.converter_commit = "test".into();
    let s = convert(&o, &kinds).unwrap();
    assert!(s.failures.is_empty(), "{:?}", s.failures);
    let m = s.manifest.unwrap();
    assert!(m.complete);
    assert_eq!((m.counts["wav"].converted, m.counts["wav"].failed), (3, 0));
    for f in [
        "data/global/sfx/cursor/pass.wav",
        "data/local/sfx/npc/akara/hi.wav",
        "data/global/music/act1/town1.wav",
    ] {
        let native = fs::read(out.join("base").join(f)).unwrap();
        assert!(d2_formats::wav::Wav::parse(&native).is_ok(), "{f}");
    }
    // Only the block no table names stays unnamed.
    assert_eq!(m.unnamed_blocks["d2sfx.mpq"], 1);
    assert_eq!(m.unnamed_blocks["d2speech.mpq"], 0);
    assert!(!out.join("base/data/global/sfx/unused/nobody.wav").exists());
}

/// A root converted before the converter followed `sounds.txt` carries the
/// old `excel` version and has no base sounds: the game refuses it, and
/// converting again adds the sounds.
#[test]
fn a_root_converted_before_the_sounds_is_refused_and_refreshed() {
    use d2_native::manifest::{parse_files_tsv, write_files_tsv, Manifest};
    use d2_native::source::{NativeSource, KNOWN_KINDS};
    use std::collections::BTreeMap;

    let dir = scratch("unlisted_sounds_old_root");
    install(&dir.join("game"));
    let kinds: Vec<Box<dyn Kind>> = vec![Box::new(Excel), Box::new(Native(AssetKind::Wav))];
    let out = dir.join("native");
    let mut o = Options::new(dir.join("game"), &out);
    o.converter_commit = "test".into();
    convert(&o, &kinds).unwrap();

    // Turn the result into what the converter wrote before: excel at
    // version 1 and no sound files or rows.
    let mut old =
        Manifest::from_toml(&fs::read_to_string(out.join("manifest.toml")).unwrap()).unwrap();
    old.kinds.insert("excel".into(), 1);
    old.kinds.remove("wav");
    old.counts.remove("wav");
    let tsv = fs::read_to_string(out.join("files.tsv")).unwrap();
    let rows: Vec<_> = parse_files_tsv(&tsv, false)
        .unwrap()
        .into_iter()
        .filter(|r| r.kind != "wav")
        .collect();
    for f in [
        "data/global/sfx/cursor/pass.wav",
        "data/local/sfx/npc/akara/hi.wav",
        "data/global/music/act1/town1.wav",
    ] {
        fs::remove_file(out.join("base").join(f)).unwrap();
    }
    fs::write(out.join("files.tsv"), write_files_tsv(&rows)).unwrap();
    fs::write(out.join("manifest.toml"), old.to_toml()).unwrap();

    let known: BTreeMap<String, u32> = KNOWN_KINDS
        .iter()
        .map(|&(k, v)| (k.to_owned(), v))
        .collect();
    let refused = old.check_loadable(&known).unwrap_err().to_string();
    assert!(refused.contains("excel"), "{refused}");
    assert!(NativeSource::open(&out).is_err());

    let s = convert(&o, &kinds).unwrap();
    assert!(s.failures.is_empty(), "{:?}", s.failures);
    let m = s.manifest.unwrap();
    assert_eq!(m.kinds["excel"], 2);
    assert_eq!(m.counts["wav"].converted, 3);
    assert!(out.join("base/data/global/sfx/cursor/pass.wav").exists());
    NativeSource::open(&out).unwrap();
}
