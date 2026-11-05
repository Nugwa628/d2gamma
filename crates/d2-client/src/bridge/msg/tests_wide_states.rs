//! 0xB7, the wide state message: a mod game's states past the state
//! messages' byte, from the sim's builders (`d2_sim::units::wide_states`)
//! to the client's state bits and lists, on a mod game's wire only.

use d2_proto::envelope::Wire;
use d2_sim::units::messages::{unit_states, SendStat, SentState};
use d2_sim::units::wide_states::{state_change, wide_unit_states, StateChange};

use super::super::dispatch::Dispatch;
use super::super::receive::{receive_chunk_on, ChunkReport, ModInbox};
use super::super::world::{StatSend, StateRow, UnitKey, PLAYER};
use super::support::Model;

const P: UnitKey = UnitKey::new(PLAYER, 1);
/// Stat 4: 16 send bits, signed.
const STAT: u16 = 4;

fn row(id: u16) -> Option<SendStat> {
    (id == STAT).then_some(SendStat {
        bits: 16,
        param_bits: 0,
        signed: true,
    })
}

/// A model with 967 states, stat 4's send columns and the unit `P`.
fn model() -> Model {
    let mut m = Model::default();
    m.inputs.tables.states = vec![StateRow::default(); 967];
    m.inputs.tables.stats = vec![StatSend::default(); 5];
    m.inputs.tables.stats[usize::from(STAT)] = StatSend {
        bits: 16,
        param_bits: 0,
        signed: true,
        update_anim_rate: false,
    };
    m.put(P);
    m
}

/// Receives `chunk` on `wire`.
fn recv_on(m: &mut Model, wire: Wire, chunk: &[u8]) -> ChunkReport {
    let mut mods = ModInbox {
        wire,
        ..ModInbox::default()
    };
    let d = Dispatch::from_spec().unwrap();
    receive_chunk_on(
        &mut mods, &mut m.w, &m.inputs, &d, &mut m.log, &mut m.out, chunk,
    )
    .unwrap()
}

fn change(c: StateChange, state: u16, entries: &[(u16, u16, i32)]) -> Vec<u8> {
    state_change(c, PLAYER, 1, state, entries, row).unwrap()
}

#[test]
fn a_mod_wire_sets_and_ends_states_past_the_byte() {
    let mut m = model();
    let entries = [(0, STAT, -25)];
    let mut chunk = change(StateChange::Delayed, 600, &[]);
    chunk.extend(change(StateChange::Set, 966, &entries));
    // A byte state in the same chunk keeps its own message (0xA8).
    chunk.extend(change(StateChange::Set, 44, &entries));
    assert_eq!(chunk[0], 0xB7);
    let r = recv_on(&mut m, Wire::Mod, &chunk);
    assert_eq!((r.messages, r.handled), (3, 3));
    let u = m.unit(P);
    assert!([44, 600, 966].iter().all(|s| u.states.contains(s)));
    assert_eq!(u.state_lists[&966][&(STAT, 0)], -25);
    assert_eq!(u.state_lists[&44][&(STAT, 0)], -25);
    let mut chunk = change(StateChange::End, 600, &[]);
    chunk.extend(change(StateChange::End, 966, &[]));
    recv_on(&mut m, Wire::Mod, &chunk);
    let u = m.unit(P);
    assert_eq!(u.states.iter().copied().collect::<Vec<_>>(), [44]);
    assert!(!u.state_lists.contains_key(&966));
    assert!(m.log.rejected.is_empty(), "{:?}", m.rejected());
}

#[test]
fn a_units_wide_states_follow_its_add_unit() {
    let mut m = model();
    let states = [
        SentState {
            state: 3,
            entries: None,
        },
        SentState {
            state: 400,
            entries: Some(vec![(0, STAT, 9)]),
        },
        SentState {
            state: 800,
            entries: None,
        },
    ];
    let mut chunk = unit_states(PLAYER, 1, &states, row);
    for msg in wide_unit_states(PLAYER, 1, &states, row) {
        chunk.extend(msg);
    }
    let r = recv_on(&mut m, Wire::Mod, &chunk);
    assert_eq!((r.messages, r.handled), (3, 3));
    let u = m.unit(P);
    assert_eq!(u.states.iter().copied().collect::<Vec<_>>(), [3, 400, 800]);
    assert_eq!(u.state_lists[&400][&(STAT, 0)], 9);
    assert!(m.log.rejected.is_empty(), "{:?}", m.rejected());
}

#[test]
fn the_original_wire_refuses_the_wide_state() {
    let mut m = model();
    let msg = change(StateChange::Delayed, 600, &[]);
    let r = recv_on(&mut m, Wire::Original, &msg);
    assert_eq!((r.messages, r.discarded_bytes), (0, msg.len()));
    assert!(m.unit(P).states.is_empty());
    // D2RUST's own state messages are the same on both wires.
    let a7 = change(StateChange::Delayed, 254, &[]);
    assert_eq!(a7, [0xA7, PLAYER, 1, 0, 0, 0, 254]);
    for wire in [Wire::Original, Wire::Mod] {
        let mut m = model();
        recv_on(&mut m, wire, &a7);
        assert!(m.unit(P).states.contains(&254));
    }
}

#[test]
fn a_bad_wide_state_is_rejected() {
    let mut m = model();
    let mut msg = change(StateChange::Delayed, 600, &[]);
    msg[7] = 9;
    let r = recv_on(&mut m, Wire::Mod, &msg);
    assert_eq!(r.rejected, 1);
    assert!(m.unit(P).states.is_empty());
}

#[test]
fn a_stack_count_is_stored_updated_and_cleared() {
    use d2_sim::units::wide_states::stack_count_message;
    let mut m = model();
    recv_on(&mut m, Wire::Mod, &change(StateChange::Delayed, 380, &[]));
    // The sim's builder and the protocol's agree on the bytes.
    let three = stack_count_message(PLAYER, 1, 380, 3);
    assert_eq!(three, d2_proto::wide_state::stacks(PLAYER, 1, 380, 3));
    let r = recv_on(&mut m, Wire::Mod, &three);
    assert_eq!((r.messages, r.handled), (1, 1));
    assert_eq!(m.unit(P).state_stacks[&380], 3);
    // A later count replaces it; a byte state takes a count as well.
    recv_on(&mut m, Wire::Mod, &stack_count_message(PLAYER, 1, 380, 5));
    recv_on(&mut m, Wire::Mod, &stack_count_message(PLAYER, 1, 7, 2));
    assert_eq!(m.unit(P).state_stacks[&380], 5);
    assert_eq!(m.unit(P).state_stacks[&7], 2);
    // Zero clears one; the state ending clears its count (wide and byte).
    recv_on(&mut m, Wire::Mod, &stack_count_message(PLAYER, 1, 7, 0));
    assert!(!m.unit(P).state_stacks.contains_key(&7));
    recv_on(&mut m, Wire::Mod, &change(StateChange::End, 380, &[]));
    assert!(m.unit(P).state_stacks.is_empty());
    recv_on(&mut m, Wire::Mod, &change(StateChange::Delayed, 12, &[]));
    recv_on(&mut m, Wire::Mod, &stack_count_message(PLAYER, 1, 12, 4));
    recv_on(&mut m, Wire::Mod, &change(StateChange::End, 12, &[]));
    assert!(m.unit(P).state_stacks.is_empty());
    assert!(m.log.rejected.is_empty(), "{:?}", m.rejected());
}

#[test]
fn a_stack_count_of_the_wrong_size_is_rejected() {
    let mut m = model();
    // A count body of one byte: size byte 11.
    let msg = d2_proto::wide_state::encode(d2_proto::wide_state::Op::Stacks, PLAYER, 1, 380, &[2])
        .unwrap();
    let r = recv_on(&mut m, Wire::Mod, &msg);
    assert_eq!(r.rejected, 1);
    assert!(m.unit(P).state_stacks.is_empty());
}

#[test]
fn a_stack_count_for_a_unit_out_of_view_is_ignored() {
    use d2_sim::units::wide_states::stack_count_message;
    let mut m = model();
    let msg = stack_count_message(PLAYER, 99, 380, 3);
    let r = recv_on(&mut m, Wire::Mod, &msg);
    assert_eq!((r.messages, r.handled, r.rejected), (1, 1, 0));
    assert!(m.unit(P).state_stacks.is_empty());
}

#[test]
fn the_original_wire_refuses_a_stack_count() {
    use d2_sim::units::wide_states::stack_count_message;
    let mut m = model();
    let msg = stack_count_message(PLAYER, 1, 380, 3);
    let r = recv_on(&mut m, Wire::Original, &msg);
    assert_eq!((r.messages, r.discarded_bytes), (0, msg.len()));
}
