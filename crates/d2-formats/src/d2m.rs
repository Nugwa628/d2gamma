//! Versioned mod character container. The header stays readable by character
//! selection; version 0x100 makes the base d2s reader reject this format.
//! The core uses wide stat ids, followed by bounded tagged mod sections.
pub mod progress;

use crate::d2s::{self, D2s, ModSection, ReadOptions, SaveTables};
use std::collections::BTreeSet;

pub const VERSION: u32 = 0x100;
pub const MAX_FILE: usize = 1024 * 1024;
const FOOTER: &[u8; 4] = b"D2GM";

pub fn write(save: &D2s, tables: &dyn SaveTables) -> Result<Vec<u8>, String> {
    if !tables.wide_stats() {
        return Err("mod codec requires wide stat tables".into());
    }
    if let Some(body) = &save.body {
        progress::read(body, save.header.towns)?;
    }
    let mut core = save.clone();
    core.header.version = d2s::VERSION;
    let sections = core
        .body
        .as_mut()
        .map(|b| std::mem::take(&mut b.mod_sections))
        .unwrap_or_default();
    if sections.len() > 256 {
        return Err("too many mod sections".into());
    }
    let mut out = d2s::write(&core, tables).map_err(|e| e.to_string())?;
    let core_len = out.len() as u32;
    out.extend_from_slice(&(sections.len() as u16).to_le_bytes());
    let mut tags = BTreeSet::new();
    for section in sections {
        if !tags.insert(section.tag) || section.data.len() > MAX_FILE {
            return Err("duplicate or oversized mod section".into());
        }
        out.extend_from_slice(&section.tag);
        out.extend_from_slice(&(section.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&section.data);
        if out.len() > MAX_FILE - 8 {
            return Err("mod save exceeds one MiB".into());
        }
    }
    out.extend_from_slice(&core_len.to_le_bytes());
    out.extend_from_slice(FOOTER);
    out[4..8].copy_from_slice(&VERSION.to_le_bytes());
    d2s::finish(&mut out);
    Ok(out)
}

pub fn read(bytes: &[u8], options: &ReadOptions, tables: &dyn SaveTables) -> Result<D2s, String> {
    if !tables.wide_stats() || bytes.len() < d2s::HEADER_SIZE + 10 || bytes.len() > MAX_FILE {
        return Err("invalid mod save size or codec".into());
    }
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    if word(0) != d2s::MAGIC
        || word(4) != VERSION
        || word(8) as usize != bytes.len()
        || &bytes[bytes.len() - 4..] != FOOTER
    {
        return Err("invalid mod save header or version".into());
    }
    let mut checked = bytes.to_vec();
    checked[12..16].fill(0);
    if d2s::checksum(&checked) != word(12) {
        return Err("mod save checksum".into());
    }
    let end = bytes.len() - 8;
    let core_len = word(end) as usize;
    if !(d2s::HEADER_SIZE..=d2s::MAX_FILE).contains(&core_len) || core_len + 2 > end {
        return Err("mod core length".into());
    }
    let mut at = core_len;
    let count = usize::from(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()));
    at += 2;
    if count > 256 {
        return Err("too many mod sections".into());
    }
    let mut sections = Vec::new();
    let mut tags = BTreeSet::new();
    for _ in 0..count {
        if end - at < 8 {
            return Err("truncated mod section".into());
        }
        let tag = bytes[at..at + 4].try_into().unwrap();
        let len = word(at + 4) as usize;
        at += 8;
        if len > end - at || !tags.insert(tag) {
            return Err("invalid mod section".into());
        }
        sections.push(ModSection::new(tag, bytes[at..at + len].to_vec()));
        at += len;
    }
    if at != end {
        return Err("trailing mod section bytes".into());
    }
    let mut core = bytes[..core_len].to_vec();
    core[4..8].copy_from_slice(&d2s::VERSION.to_le_bytes());
    d2s::finish(&mut core);
    let mut save = d2s::read(&core, options, tables).map_err(|e| e.to_string())?;
    if let Some(body) = save.body.as_mut() {
        body.mod_sections = sections;
        progress::read(body, save.header.towns)?;
    } else if !sections.is_empty() {
        return Err("stub cannot hold mod sections".into());
    }
    Ok(save)
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2s::{Body, Header, StatEntry, StatSave, Stats};
    struct Tables;
    impl SaveTables for Tables {
        fn wide_stats(&self) -> bool {
            true
        }
        fn stat_save(&self, id: u16) -> Option<StatSave> {
            (id < 16 || id == 40000).then_some(StatSave {
                bits: 32,
                param: if id == 40000 { 16 } else { 0 },
                signed: true,
            })
        }
        fn item_entry_len(&self, _: &[u8]) -> Result<usize, String> {
            Err("no items".into())
        }
    }
    fn save() -> D2s {
        let header = Header {
            status: 0,
            ..Header::default()
        };
        D2s {
            body: Some(Body {
                stats: Stats::Bits(vec![StatEntry {
                    id: 40000,
                    layer: 7,
                    value: -1234567,
                }]),
                skills: vec![0; usize::from(header.skill_count)],
                mod_sections: vec![ModSection::new(*b"TEST", vec![1, 2, 3])],
                ..Body::default()
            }),
            header,
        }
    }
    #[test]
    fn wide_mod_stats_sections_and_stubs_round_trip_and_base_rejects() {
        let s = save();
        let bytes = write(&s, &Tables).unwrap();
        let back = read(&bytes, &ReadOptions::default(), &Tables).unwrap();
        assert_eq!(back.body, s.body);
        assert_eq!(d2s::header_name(&bytes), s.header.name_bytes());
        assert!(d2s::read(&bytes, &ReadOptions::default(), &Tables).is_err());
        let stub = D2s::new_stub(b"ModHero", 0, 0, 1).unwrap();
        assert!(read(
            &write(&stub, &Tables).unwrap(),
            &ReadOptions::default(),
            &Tables
        )
        .unwrap()
        .body
        .is_none());
        assert_eq!(
            d2s::write_stats(
                &[StatEntry {
                    id: 40000,
                    layer: 7,
                    value: -1
                }],
                &Tables
            ),
            [0x67, 0x66, 0x40, 0x9C, 7, 0, 255, 255, 255, 255, 255, 255]
        );
    }
    #[test]
    fn corrupt_truncated_duplicate_oversized_and_unknown_versions_fail() {
        let good = write(&save(), &Tables).unwrap();
        for n in 0..good.len() {
            assert!(read(&good[..n], &ReadOptions::default(), &Tables).is_err());
        }
        let mut bad = good.clone();
        bad[4] = 2;
        d2s::finish(&mut bad);
        assert!(read(&bad, &ReadOptions::default(), &Tables).is_err());
        let mut bad = good;
        bad[100] ^= 1;
        assert!(read(&bad, &ReadOptions::default(), &Tables).is_err());
        let mut s = save();
        s.body
            .as_mut()
            .unwrap()
            .mod_sections
            .push(ModSection::new(*b"TEST", vec![]));
        assert!(write(&s, &Tables).is_err());
        s.body.as_mut().unwrap().mod_sections = vec![ModSection::new(*b"HUGE", vec![0; MAX_FILE])];
        assert!(write(&s, &Tables).is_err());
    }
}
