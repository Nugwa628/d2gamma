//! Version 1 authoritative mod stat totals. Original wire rejects this id.
use crate::envelope::Wire;
use crate::schema::Size;
pub const ID: u8 = 0xB8;
pub const SIZE: usize = 13;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideStat {
    pub unit_type: u8,
    pub guid: u32,
    pub stat: u16,
    pub value: i32,
}
pub fn encode(m: WideStat) -> Vec<u8> {
    let mut bytes = vec![ID, 1, m.unit_type];
    bytes.extend_from_slice(&m.guid.to_le_bytes());
    bytes.extend_from_slice(&m.stat.to_le_bytes());
    bytes.extend_from_slice(&m.value.to_le_bytes());
    bytes
}
pub fn size(wire: Wire, bytes: &[u8]) -> Size {
    if wire != Wire::Mod {
        Size::Invalid
    } else if bytes.len() < SIZE {
        Size::Incomplete
    } else {
        Size::Bytes(SIZE)
    }
}
pub fn decode(bytes: &[u8]) -> Option<WideStat> {
    if bytes.len() != SIZE || bytes[..2] != [ID, 1] || bytes[2] > 5 {
        return None;
    }
    Some(WideStat {
        unit_type: bytes[2],
        guid: u32::from_le_bytes(bytes[3..7].try_into().ok()?),
        stat: u16::from_le_bytes(bytes[7..9].try_into().ok()?),
        value: i32::from_le_bytes(bytes[9..13].try_into().ok()?),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wide_signed_round_trip_and_base_rejection() {
        let m = WideStat {
            unit_type: 1,
            guid: 9,
            stat: 40000,
            value: -123,
        };
        let b = encode(m);
        assert_eq!(
            b,
            [0xB8, 1, 1, 9, 0, 0, 0, 0x40, 0x9C, 0x85, 0xFF, 0xFF, 0xFF]
        );
        assert_eq!(decode(&b), Some(m));
        assert_eq!(
            crate::transport::server_size_on(Wire::Original, &b),
            Size::Invalid
        );
        assert_eq!(
            crate::transport::split_server_buffer_on(Wire::Mod, &b)
                .unwrap()
                .messages,
            [&b[..]]
        );
        for i in 0..b.len() {
            assert!(decode(&b[..i]).is_none());
        }
        let mut bad = b;
        bad[1] = 2;
        assert!(decode(&bad).is_none());
    }
}
