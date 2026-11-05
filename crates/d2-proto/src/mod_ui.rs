//! Modkit UI payloads inside the existing mod envelope. Keep this namespace
//! and its version here so later mod networking can route it as one service.
const PREFIX: &[u8; 5] = b"LCUI\x01";
pub const MAX_VARIABLES: usize = 256;

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
}

pub fn variable(name: &str, value: i32) -> Option<Vec<u8>> {
    if !valid_name(name) {
        return None;
    }
    let mut bytes = PREFIX.to_vec();
    bytes.push(1);
    bytes.extend_from_slice(&value.to_le_bytes());
    bytes.extend_from_slice(name.as_bytes());
    Some(bytes)
}

pub fn read_variable(bytes: &[u8]) -> Option<(&str, i32)> {
    if bytes.get(..5)? != PREFIX || bytes.get(5) != Some(&1) {
        return None;
    }
    let value = i32::from_le_bytes(bytes.get(6..10)?.try_into().ok()?);
    let name = std::str::from_utf8(bytes.get(10..)?).ok()?;
    valid_name(name).then_some((name, value))
}

/// Reserve even malformed/unknown LCUI versions so they cannot fall through
/// into another mod service.
pub fn claims(bytes: &[u8]) -> bool {
    bytes.starts_with(b"LCUI")
}

pub fn action(name: &str) -> Option<Vec<u8>> {
    if !valid_name(name) {
        return None;
    }
    let mut bytes = PREFIX.to_vec();
    bytes.push(2);
    bytes.extend_from_slice(name.as_bytes());
    Some(bytes)
}

pub fn read_action(bytes: &[u8]) -> Option<&str> {
    if bytes.get(..5)? != PREFIX || bytes.get(5) != Some(&2) {
        return None;
    }
    let name = std::str::from_utf8(bytes.get(6..)?).ok()?;
    valid_name(name).then_some(name)
}

/// Both U3's original name request and N1's schema ID enter one service.
pub enum ActionRequest<'a> {
    Legacy(&'a str),
    Declared(u16),
}
pub fn read_action_request(bytes: &[u8]) -> Option<ActionRequest<'_>> {
    read_server_action(bytes)
        .map(ActionRequest::Declared)
        .or_else(|| read_action(bytes).map(ActionRequest::Legacy))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variables_are_small_versioned_and_strict() {
        let b = variable("combat/rage", i32::MIN).unwrap();
        assert_eq!(read_variable(&b), Some(("combat/rage", i32::MIN)));
        for n in 0..10 {
            assert!(read_variable(&b[..n]).is_none());
        }
        let mut bad = b.clone();
        bad[4] = 2;
        assert!(read_variable(&bad).is_none());
        assert!(variable("", 0).is_none());
        assert!(variable(&"a".repeat(129), 0).is_none());
    }
    #[test]
    fn actions_are_name_only_bounded_and_unknown_versions_stay_reserved() {
        let bytes = action("hud/allocate").unwrap();
        assert_eq!(bytes.len(), 6 + 12);
        assert_eq!(read_action(&bytes), Some("hud/allocate"));
        assert!(read_variable(&bytes).is_none());
        for n in 0..=6 {
            assert!(read_action(&bytes[..n]).is_none());
        }
        let mut bad = bytes.clone();
        bad[4] = 2;
        assert!(claims(&bad));
        assert!(read_action(&bad).is_none());
        bad[4] = 1;
        bad[5] = 255;
        assert!(read_action(&bad).is_none());
        assert!(action("").is_none());
        assert!(action(&"x".repeat(129)).is_none());
    }
}

/// Server-owned named visual event. Coordinates name a sub-tile cell; zero
/// duration cancels the matching (source, name). This is presentation only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualEvent {
    pub name: String,
    pub source_type: u8,
    pub source_guid: u32,
    pub x: u16,
    pub y: u16,
    pub duration: u16,
    pub follow: bool,
}
pub fn visual(event: &VisualEvent) -> Option<Vec<u8>> {
    if !valid_name(&event.name) || event.source_type > 5 || event.duration > 3600 {
        return None;
    }
    let mut b = PREFIX.to_vec();
    b.push(3);
    b.push(event.source_type);
    b.extend(event.source_guid.to_le_bytes());
    b.extend(event.x.to_le_bytes());
    b.extend(event.y.to_le_bytes());
    b.extend(event.duration.to_le_bytes());
    b.push(u8::from(event.follow));
    b.extend(event.name.as_bytes());
    Some(b)
}
pub fn read_visual(b: &[u8]) -> Option<VisualEvent> {
    if b.get(..5)? != PREFIX || b.get(5) != Some(&3) || *b.get(6)? > 5 || *b.get(17)? > 1 {
        return None;
    }
    let duration = u16::from_le_bytes(b.get(15..17)?.try_into().ok()?);
    if duration > 3600 {
        return None;
    }
    let name = std::str::from_utf8(b.get(18..)?).ok()?;
    if !valid_name(name) {
        return None;
    }
    Some(VisualEvent {
        name: name.into(),
        source_type: b[6],
        source_guid: u32::from_le_bytes(b[7..11].try_into().ok()?),
        x: u16::from_le_bytes(b[11..13].try_into().ok()?),
        y: u16::from_le_bytes(b[13..15].try_into().ok()?),
        duration,
        follow: b[17] == 1,
    })
}

#[cfg(test)]
mod visual_tests {
    use super::*;
    #[test]
    fn visual_events_are_small_versioned_and_strict() {
        let e = VisualEvent {
            name: "ember/windup".into(),
            source_type: 1,
            source_guid: u32::MAX,
            x: u16::MAX,
            y: 7,
            duration: 3600,
            follow: true,
        };
        let b = visual(&e).unwrap();
        assert_eq!(b.len(), 18 + e.name.len());
        assert_eq!(read_visual(&b), Some(e.clone()));
        assert!(read_action(&b).is_none());
        assert!(read_variable(&b).is_none());
        for n in 0..=18 {
            assert!(read_visual(&b[..n]).is_none());
        }
        for (index, value) in [(4, 2), (5, 2), (6, 6), (17, 2)] {
            let mut bad = b.clone();
            bad[index] = value;
            assert!(read_visual(&bad).is_none());
            assert!(claims(&bad));
        }
        let mut bad = b.clone();
        bad[15..17].copy_from_slice(&3601_u16.to_le_bytes());
        assert!(read_visual(&bad).is_none());
        let mut cancel = e.clone();
        cancel.duration = 0;
        assert_eq!(read_visual(&visual(&cancel).unwrap()), Some(cancel));
        let mut bad = e.clone();
        bad.name = "x".repeat(129);
        assert!(visual(&bad).is_none());
        bad = e;
        bad.source_type = 6;
        assert!(visual(&bad).is_none());
    }
}

/// Typed mod-schema value publication; the envelope is still 0xB5.
pub fn synced(id: u16, value: i32) -> Option<Vec<u8>> {
    if id == 0 || id == u16::MAX {
        return None;
    }
    let mut b = PREFIX.to_vec();
    b.push(4);
    b.extend(id.to_le_bytes());
    b.extend(value.to_le_bytes());
    Some(b)
}
pub fn read_synced(b: &[u8]) -> Option<(u16, i32)> {
    if b.len() != 12 || b.get(..5)? != PREFIX || b[5] != 4 {
        return None;
    }
    let id = u16::from_le_bytes(b[6..8].try_into().ok()?);
    if id == 0 || id == u16::MAX {
        return None;
    }
    Some((id, i32::from_le_bytes(b[8..12].try_into().ok()?)))
}
/// A declared server action supplies no client-selected gameplay values.
pub fn server_action(id: u16) -> Option<Vec<u8>> {
    if id == 0 || id == u16::MAX {
        return None;
    }
    let mut b = PREFIX.to_vec();
    b.push(5);
    b.extend(id.to_le_bytes());
    Some(b)
}
pub fn read_server_action(b: &[u8]) -> Option<u16> {
    if b.len() != 8 || b.get(..5)? != PREFIX || b[5] != 5 {
        return None;
    }
    let id = u16::from_le_bytes(b[6..8].try_into().ok()?);
    (id != 0 && id != u16::MAX).then_some(id)
}
#[cfg(test)]
mod net_tests {
    use super::*;
    #[test]
    fn declared_messages_are_fixed_typed_and_share_the_existing_envelope() {
        for id in [1, 65534] {
            let b = synced(id, i32::MIN).unwrap();
            assert_eq!(read_synced(&b), Some((id, i32::MIN)));
            for n in 0..12 {
                assert!(read_synced(&b[..n]).is_none());
            }
            assert!(read_action(&b).is_none());
            let a = server_action(id).unwrap();
            assert_eq!(read_server_action(&a), Some(id));
            assert!(read_synced(&a).is_none());
            let e = crate::envelope::encode(crate::envelope::Direction::ToServer, &a).unwrap();
            assert_eq!(e[0], 0x71);
            assert_eq!(
                crate::envelope::payload(crate::envelope::Direction::ToServer, &e),
                Some(a.as_slice())
            );
        }
        for id in [0, 65535] {
            assert!(synced(id, 0).is_none());
            assert!(server_action(id).is_none());
        }
    }
}

/// A contract identity uses LCUI operations 6 (hello) and 7 (accepted).
pub fn net_identity(accepted: bool, hash: &[u8; 32]) -> Vec<u8> {
    let mut b = PREFIX.to_vec();
    b.push(if accepted { 7 } else { 6 });
    b.extend(hash);
    b
}
pub fn read_net_identity(b: &[u8], accepted: bool) -> Option<[u8; 32]> {
    (b.len() == 38 && b.get(..5)? == PREFIX && b[5] == if accepted { 7 } else { 6 })
        .then(|| b[6..].try_into().unwrap())
}
pub fn net_rejection(reason: &str) -> Option<Vec<u8>> {
    if reason.is_empty() || reason.len() > 400 || reason.chars().any(char::is_control) {
        return None;
    }
    let mut b = PREFIX.to_vec();
    b.push(8);
    b.extend(reason.as_bytes());
    Some(b)
}
pub fn read_net_rejection(b: &[u8]) -> Option<&str> {
    if b.len() < 7 || b.len() > 406 || b.get(..5)? != PREFIX || b[5] != 8 {
        return None;
    }
    let s = std::str::from_utf8(&b[6..]).ok()?;
    (!s.chars().any(char::is_control)).then_some(s)
}
#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn network_identity_and_rejection_decoders_are_strict() {
        let hash = [17; 32];
        for accepted in [false, true] {
            let b = net_identity(accepted, &hash);
            assert_eq!(read_net_identity(&b, accepted), Some(hash));
            assert!(read_net_identity(&b, !accepted).is_none());
            for n in 0..b.len() {
                assert!(read_net_identity(&b[..n], accepted).is_none());
            }
            let mut extra = b;
            extra.push(0);
            assert!(read_net_identity(&extra, accepted).is_none());
        }
        for s in ["", "bad\nreason"] {
            assert!(net_rejection(s).is_none());
        }
        assert!(net_rejection(&"x".repeat(401)).is_none());
        let b = net_rejection("This mod needs updating: server alpha 2.0.0").unwrap();
        assert_eq!(
            read_net_rejection(&b),
            Some("This mod needs updating: server alpha 2.0.0")
        );
        assert!(read_net_rejection(b"LCUI\x01\x08\xff").is_none());
    }

    #[test]
    fn network_action_request_preserves_legacy_and_declared_decoders() {
        let old = action("hud/allocate").unwrap();
        let new = server_action(7).unwrap();
        assert!(matches!(
            read_action_request(&old),
            Some(ActionRequest::Legacy("hud/allocate"))
        ));
        assert!(matches!(
            read_action_request(&new),
            Some(ActionRequest::Declared(7))
        ));
        assert!(read_action_request(&synced(7, 10).unwrap()).is_none());
        for end in 0..=6 {
            assert!(read_action_request(&old[..end]).is_none());
        }
        for end in 0..new.len() {
            assert!(read_action_request(&new[..end]).is_none());
        }
        for bytes in [old, new] {
            let mut extra = bytes;
            extra.push(0);
            assert!(read_action_request(&extra).is_none());
        }
    }
}
