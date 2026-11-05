//! cltdofunc 37 (Charge): one function of the client skill do
//! table (`super::client_do`); the shared helpers live in `super`.

use super::*;

/// `cltdofunc` 37 (Charge). E := the used
/// entry, none → 0; F := E-flags. The sequence gives n :=
/// its count (none: 15) and the loop end k := −1 when its first frame's
/// event byte is 0 (reads only the first frame), else 7. An
/// action event of 3 (+0x4E) on a player or monster is the hit: 0 (its
/// attack sound is the audio layer's). Else T := the unit of E's
/// parameters 1 / 2 (none when parameter 1 is 6). With F bit 0:
/// - T in melee range → F := 0, the position := (position & 0xFF) +
///   (k + 1) · 256, the count := n · 256. Result 1.
/// - F bit 1 clear (the charge runs): a position at or past k →
///   position := position & 0xFF, count := n · 256 and F := 1; flag 0x40
///   off; the counter + 1 while below 2. Result 1.
/// - F bit 1 set (the path ended): S := the pick with (U, 3, 3,
///   parameter 2) ([`charge_pick`]); S in melee range → F := 0 and
///   parameters 1 / 2 := S's type and GUID. Else F := 0 and, but for a
///   monster of base id 436 or 441, the count := 0x100 (the mode ends
///   this update). Result 1.
///
/// Without F bit 0 (the hit runs): F and parameters 1 / 2 := 0, the
/// count := (n − (position >> 8)) · 256; then T, else the pick with the
/// cleared parameter 2, re-aims U's path at that unit's cell (1); with
/// neither, 0.
pub(super) fn charge(
    w: &mut ClientWorld,
    inputs: &ModelInputs,
    key: UnitKey,
) -> Result<i32, HandlerError> {
    let Some(e) = current_entry(w, key).copied() else {
        return Ok(0);
    };
    let Some(u) = w.units.get(&key) else {
        return Ok(0);
    };
    let (n, k) = match &u.sequence {
        Some(q) => {
            let n = q.length >> 8;
            let first = q.frames.first().map_or(1, |f| f.event);
            (n, if n > 0 && first == 0 { -1 } else { 7 })
        }
        None => (15, 7),
    };
    if key.unit_type <= MONSTER && u.action_frame == 3 {
        return Ok(0);
    }
    let env = search_env(inputs);
    let t = (e.param != 6)
        .then(|| UnitKey::new(e.param as u8, e.param2 as u32))
        .filter(|t| w.units.contains_key(t));
    if e.flags & 1 == 0 {
        set_entry(w, key, |e| (e.flags, e.param, e.param2) = (0, 0, 0));
        let u = w.units.get_mut(&key).expect("present");
        let pos = u.sequence.as_ref().map_or(0, |q| q.pos);
        u.frame_count = (n - (pos >> 8)) << 8;
        let aim = match t {
            Some(t) => Some(t),
            None => charge_pick(w, &env, key, 0)?,
        };
        return Ok(match aim {
            Some(a) => {
                face_unit(w, key, a);
                1
            }
            None => 0,
        });
    }
    if let Some(t) = t {
        if crate::bridge::client_missiles::in_melee_range(w, &env, key, t, 0)? {
            set_entry(w, key, |e| e.flags = 0);
            let u = w.units.get_mut(&key).expect("present");
            if let Some(q) = u.sequence.as_mut() {
                q.pos = (q.pos & 0xFF) + ((k + 1) << 8);
            }
            u.frame_count = n << 8;
            return Ok(1);
        }
    }
    if e.flags & 2 == 0 {
        let u = w.units.get_mut(&key).expect("present");
        if let Some(q) = u.sequence.as_mut() {
            if k <= q.pos >> 8 {
                q.pos &= 0xFF;
                u.frame_count = n << 8;
                if let Some(e) = current_entry_mut(w, key) {
                    e.flags = 1;
                }
            }
        }
        let u = w.units.get_mut(&key).expect("present");
        u.flag_40 = false;
        if let Some(e) = current_entry_mut(w, key) {
            if e.counter < 2 {
                e.counter += 1;
            }
        }
        return Ok(1);
    }
    let s = charge_pick(w, &env, key, e.param2 as u32)?;
    if let Some(s) = s {
        if crate::bridge::client_missiles::in_melee_range(w, &env, key, s, 0)? {
            set_entry(w, key, |e| {
                (e.flags, e.param, e.param2) = (0, i32::from(s.unit_type), s.guid as i32)
            });
            return Ok(1);
        }
    }
    set_entry(w, key, |e| e.flags = 0);
    const BASE_KEEPS_RUNNING: [u16; 2] = [436, 441];
    let keeps = key.unit_type == MONSTER
        && w.units
            .get(&key)
            .and_then(|u| inputs.tables.monsters.get(u.class as usize))
            .and_then(|c| c.as_ref())
            .is_some_and(|c| BASE_KEEPS_RUNNING.contains(&c.base_id));
    if !keeps {
        if let Some(u) = w.units.get_mut(&key) {
            u.frame_count = 0x100;
        }
    }
    Ok(1)
}

/// The Charge do's pick with (U, 3, 3, g): among the players and monsters
/// within radius 3 of U that are not dead (the search mask 0xA283), the
/// next GUID after `g`.
fn charge_pick(
    w: &ClientWorld,
    env: &crate::bridge::client_missiles::Env,
    key: UnitKey,
    g: u32,
) -> Result<Option<UnitKey>, HandlerError> {
    crate::bridge::client_missiles::next_guid(w, env, key, (0, 0), 3, 3, g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::client_do::test_util::*;
    use crate::bridge::world::{MonsterClass, MonsterSequence};

    /// Charge's 15 frames (event 1 on the first) at position `pos`.
    fn charge_sequence(pos: i32) -> MonsterSequence {
        let frames = vec![
            d2_sim::skills::sequences::SeqFrame {
                mode: 3,
                frame: 0,
                event: 1,
            };
            15
        ];
        MonsterSequence {
            frames,
            length: 15 << 8,
            pos,
            step: 0x100,
            drawn_mode: 18,
        }
    }

    /// A monster of class 2 (base id `base`) charging with E-flags
    /// `flags` at (5000, 5000), sequence position `pos`, and a monster
    /// next to it at (5001, 5000) that E's parameters name when `aimed`.
    fn charger(
        flags: u32,
        pos: i32,
        aimed: bool,
        base: u16,
    ) -> (ClientWorld, ModelInputs, UnitKey) {
        let mut i = inputs(37);
        i.tables.monsters = vec![None; 3];
        i.tables.monsters[2] = Some(MonsterClass {
            base_id: base,
            ..MonsterClass::default()
        });
        let (mut pw, pkey) = world();
        let key = UnitKey::new(MONSTER, 1);
        let mut u = pw.units.remove(&pkey).unwrap();
        u.key = key;
        u.class = 2;
        u.sequence = Some(charge_sequence(pos));
        u.frame_count = 0x700;
        let l = u.skills.as_mut().unwrap();
        let e = &mut l.entries[0];
        e.flags = flags;
        (e.param, e.param2) = if aimed {
            (i32::from(MONSTER), 9)
        } else {
            (6, 0)
        };
        let mut w = ClientWorld::default();
        w.units.insert(key, u);
        let t = UnitKey::new(MONSTER, 9);
        let mut c = ClientUnit::new(t);
        (c.position, c.mode) = (Some((5001, 5000)), 1);
        w.units.insert(t, c);
        (w, i, key)
    }

    /// Every code is a weapon with a reach of 3 (`rangeadder`).
    struct Polearm;
    impl crate::bridge::item_lists::StreamProps for Polearm {
        fn props(&self, _: &[u8]) -> (Vec<crate::bridge::item_lists::ItemProp>, bool) {
            (Vec::new(), false)
        }
        fn weapon_range_adder(&self, _: [u8; 4]) -> Option<i32> {
            Some(3)
        }
    }

    /// An item stream head at body location 4 (mode 1) of code `code`.
    fn body_stream(code: &[u8; 4]) -> Vec<u8> {
        let bits: [(u32, u32); 8] = [
            (0x10, 32),
            (0x65, 10),
            (1, 3),
            (4, 4),
            (0, 4),
            (0, 4),
            (0, 3),
            (u32::from_le_bytes(*code), 32),
        ];
        let (mut out, mut acc, mut n) = (Vec::new(), 0u64, 0u32);
        for (v, w) in bits {
            acc |= u64::from(v) << n;
            n += w;
            while n >= 8 {
                out.push(acc as u8);
                acc >>= 8;
                n -= 8;
            }
        }
        if n > 0 {
            out.push(acc as u8);
        }
        out
    }

    /// A player other than the local one (GUID 7, the item tables with a
    /// reach-3 weapon) at (5000, 5000) holding a weapon on body location 4
    /// when `armed`, and a monster at (5004, 5000) the Charge names.
    fn remote_charger(armed: bool) -> (ClientWorld, ModelInputs, UnitKey) {
        use crate::bridge::world::{ItemData, ItemRecord, KindData, PLAYER};
        let i = inputs(37);
        let (mut pw, pkey) = world();
        let key = UnitKey::new(PLAYER, 7);
        let mut u = pw.units.remove(&pkey).unwrap();
        u.key = key;
        u.sequence = Some(charge_sequence(0x310));
        u.skills.as_mut().unwrap().entries[0].flags = 0x1001;
        (
            u.skills.as_mut().unwrap().entries[0].param,
            u.skills.as_mut().unwrap().entries[0].param2,
        ) = (i32::from(MONSTER), 9);
        let mut w = ClientWorld::default();
        w.units.insert(key, u);
        w.local_player = Some(UnitKey::new(PLAYER, 1));
        w.item_tables = crate::bridge::item_lists::ItemTablesRef(Some(Arc::new(Polearm)));
        let t = UnitKey::new(MONSTER, 9);
        let mut c = ClientUnit::new(t);
        (c.position, c.mode) = (Some((5004, 5000)), 1);
        w.units.insert(t, c);
        if armed {
            let ik = UnitKey::new(crate::bridge::items::ITEM, 30);
            let mut item = ClientUnit::new(ik);
            item.kind = KindData::Item(ItemData {
                last: Some(ItemRecord {
                    id: 0x9D,
                    action: 0x04,
                    category: 0,
                    owner: Some(key),
                    seq: 0,
                    stream: body_stream(b"spr "),
                }),
                ..Default::default()
            });
            w.units.insert(ik, item);
        }
        (w, i, key)
    }

    fn entry(w: &ClientWorld, key: UnitKey) -> SkillEntry {
        w.units[&key].skills.as_ref().unwrap().entries[0]
    }

    #[test]
    fn the_charge_do_loops_while_the_path_runs_and_ends_it_after() {
        use crate::bridge::use_state::client_start_apply;
        let (mut w, key) = aimed_at((5010, 5000));
        let mut i = inputs(37);
        i.tables.skills[usize::from(FROST_NOVA)].cltstfunc = 25;
        client_start_apply(&mut w, &i, key, FROST_NOVA, 1).unwrap();
        let e = w.units[&key].skills.as_ref().unwrap().entries[0];
        assert_eq!((e.flags, e.param, e.param2, e.param3), (0x1001, 6, 0, 7));
        assert_eq!(w.units[&key].path_live, Some(true));
        let u = w.units.get_mut(&key).unwrap();
        u.sequence = Some(charge_sequence(0x710));
        u.flag_40 = true;
        // The charge runs: past frame 7 the loop restarts, flag 0x40 off.
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 1);
        let u = &w.units[&key];
        assert_eq!(
            (u.sequence.as_ref().unwrap().pos, u.frame_count),
            (0x10, 15 << 8)
        );
        assert!(!u.flag_40);
        assert_eq!(u.skills.as_ref().unwrap().entries[0].counter, 1);
        // The path ended (bit 1) and nothing in radius 3: count 0x100.
        let l = w.units.get_mut(&key).unwrap().skills.as_mut().unwrap();
        l.entries[0].flags |= 2;
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 1);
        let u = &w.units[&key];
        assert_eq!(u.frame_count, 0x100);
        assert_eq!(u.skills.as_ref().unwrap().entries[0].flags, 0);
    }

    #[test]
    fn a_target_in_reach_jumps_the_charge_to_its_hit_frames() {
        let (mut w, i, key) = charger(0x1001, 0x310, true, 0);
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 1);
        let u = &w.units[&key];
        // The position goes to frame k + 1 = 8, keeping its fraction.
        assert_eq!(
            (u.sequence.as_ref().unwrap().pos, u.frame_count),
            (0x810, 15 << 8)
        );
        assert_eq!(entry(&w, key).flags, 0);
        // Out of reach: the charge runs on.
        let (mut w, i, key) = charger(0x1001, 0x310, true, 0);
        w.units.get_mut(&UnitKey::new(MONSTER, 9)).unwrap().position = Some((5010, 5000));
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 1);
        assert_eq!(w.units[&key].sequence.as_ref().unwrap().pos, 0x310);
        assert_eq!(entry(&w, key).flags, 0x1001);
    }

    #[test]
    fn the_hit_clears_the_target_and_re_aims_the_path_at_it() {
        let (mut w, i, key) = charger(0, 0x910, true, 0);
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 1);
        let u = &w.units[&key];
        // The count is the frames left: (15 − 9) · 256.
        assert_eq!(u.frame_count, 6 << 8);
        assert_eq!(u.faces, 1);
        let e = entry(&w, key);
        assert_eq!((e.flags, e.param, e.param2), (0, 0, 0));
        // No target and nothing to pick: 0, no turn.
        let (mut w, i, key) = charger(0, 0x910, false, 0);
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 0);
        assert_eq!(w.units[&key].faces, 0);
        assert_eq!(w.units[&key].frame_count, 6 << 8);
    }

    #[test]
    fn the_hit_event_returns_0_and_changes_nothing() {
        let (mut w, i, key) = charger(0x1001, 0x310, true, 0);
        w.units.get_mut(&key).unwrap().action_frame = 3;
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 0);
        assert_eq!(entry(&w, key).flags, 0x1001);
        assert_eq!(w.units[&key].sequence.as_ref().unwrap().pos, 0x310);
    }

    #[test]
    fn two_monster_kinds_keep_running_when_the_path_ends_empty() {
        for (base, count) in [(436, 0x700), (441, 0x700), (435, 0x100)] {
            let (mut w, i, key) = charger(0x1003, 0x310, false, base);
            assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 1);
            assert_eq!(w.units[&key].frame_count, count, "base {base}");
            assert_eq!(entry(&w, key).flags, 0);
        }
    }

    /// Another player's Charge in a realm client: its reach is the
    /// `rangeadder` of the weapon the server shows on its body (a 0x9D
    /// record naming it as owner), not an error as for a unit other than the
    /// local player.
    #[test]
    fn another_players_charge_reads_its_own_weapon_reach() {
        // Armed with a reach of 3 the target 4 cells away is in reach.
        let (mut w, i, key) = remote_charger(true);
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 1);
        assert_eq!(w.units[&key].sequence.as_ref().unwrap().pos, 0x810);
        assert_eq!(entry(&w, key).flags, 0);
        // Unarmed the reach is 0 and the charge runs on.
        let (mut w, i, key) = remote_charger(false);
        assert_eq!(client_do(&mut w, &i, key, FROST_NOVA, 1).unwrap(), 1);
        assert_eq!(w.units[&key].sequence.as_ref().unwrap().pos, 0x310);
        assert_eq!(entry(&w, key).flags, 0x1001);
    }
}
