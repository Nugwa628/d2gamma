//! Versioned per-difficulty mod progress, stored in reserved section DPRG.
use crate::d2s::{Body, ModSection};
pub const TAG: [u8; 4] = *b"DPRG";
const RECORD: usize = 129;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub quests: [u8; 96],
    pub npc_a: [u8; 8],
    pub npc_b: [u8; 8],
    pub waypoints: [u8; 16],
    pub town: u8,
}
impl Default for Progress {
    fn default() -> Self {
        let mut waypoints = [0; 16];
        waypoints[..3].copy_from_slice(&[2, 1, 1]);
        Self {
            quests: [0; 96],
            npc_a: [0; 8],
            npc_b: [0; 8],
            waypoints,
            town: 0,
        }
    }
}

pub fn read(body: &Body, towns: [u8; 3]) -> Result<Vec<Progress>, String> {
    let Some(section) = body.mod_sections.iter().find(|s| s.tag == TAG) else {
        return Ok((0..3)
            .map(|d| Progress {
                quests: body.quests.records[d],
                npc_a: body.npcs.a[d],
                npc_b: body.npcs.b[d],
                waypoints: body.waypoints.records[d],
                town: towns[d],
            })
            .collect());
    };
    let bytes = &section.data;
    if bytes.len() < 2
        || bytes[0] != 1
        || bytes[1] == 0
        || bytes.len() != 2 + usize::from(bytes[1]) * RECORD
    {
        return Err("invalid DPRG version, count or size".into());
    }
    bytes[2..]
        .as_chunks::<RECORD>()
        .0
        .iter()
        .map(|r| {
            if !matches!(u16::from_le_bytes([r[112], r[113]]), 0 | 0x101 | 0x102) {
                return Err("invalid DPRG waypoint magic".into());
            }
            Ok(Progress {
                quests: r[..96].try_into().unwrap(),
                npc_a: r[96..104].try_into().unwrap(),
                npc_b: r[104..112].try_into().unwrap(),
                waypoints: r[112..128].try_into().unwrap(),
                town: r[128],
            })
        })
        .collect()
}

pub fn write(body: &mut Body, records: &[Progress]) -> Result<(), String> {
    if records.is_empty() || records.len() > 255 {
        return Err("DPRG requires 1..255 difficulties".into());
    }
    let mut data = vec![1, records.len() as u8];
    for r in records {
        data.extend_from_slice(&r.quests);
        data.extend_from_slice(&r.npc_a);
        data.extend_from_slice(&r.npc_b);
        data.extend_from_slice(&r.waypoints);
        data.push(r.town);
    }
    body.mod_sections.retain(|s| s.tag != TAG);
    body.mod_sections.push(ModSection::new(TAG, data));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fourth_progress_is_independent_and_base_has_exactly_three() {
        let mut body = Body::default();
        assert_eq!(read(&body, [0; 3]).unwrap().len(), 3);
        let mut records = vec![Progress::default(); 4];
        records[3].quests[4] = 7;
        records[3].waypoints[3] = 16;
        records[3].town = 0x84;
        write(&mut body, &records).unwrap();
        assert_eq!(read(&body, [0; 3]).unwrap(), records);
        assert_eq!(body.quests.records, [[0; 96]; 3]);
        let good = body.mod_sections[0].data.clone();
        for end in 0..good.len() {
            body.mod_sections[0].data = good[..end].to_vec();
            assert!(read(&body, [0; 3]).is_err());
        }
        body.mod_sections[0].data = good;
        body.mod_sections[0].data[0] = 2;
        assert!(read(&body, [0; 3]).is_err());
        assert!(write(&mut body, &vec![Progress::default(); 256]).is_err());
    }
}
