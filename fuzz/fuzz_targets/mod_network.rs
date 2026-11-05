#![no_main]
use d2_proto::{
    envelope::{self, Direction},
    mod_ui as n,
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    if let Some((id, value)) = n::read_synced(bytes) {
        assert_eq!(n::synced(id, value).unwrap(), bytes);
    }
    if let Some(id) = n::read_server_action(bytes) {
        assert_eq!(n::server_action(id).unwrap(), bytes);
    }
    for accepted in [false, true] {
        if let Some(hash) = n::read_net_identity(bytes, accepted) {
            assert_eq!(n::net_identity(accepted, &hash), bytes);
        }
    }
    if let Some(reason) = n::read_net_rejection(bytes) {
        assert_eq!(n::net_rejection(reason).unwrap(), bytes);
    }
    let _ = n::read_action_request(bytes);
    let _ = n::read_action(bytes);
    let _ = n::read_variable(bytes);
    let _ = n::read_visual(bytes);
    for direction in [Direction::ToServer, Direction::ToClient] {
        if let Some(payload) = envelope::payload(direction, bytes) {
            assert_eq!(envelope::encode(direction, payload).unwrap(), bytes);
            let _ = n::read_action_request(payload);
            let _ = n::read_server_action(payload);
            let _ = n::read_synced(payload);
            let _ = n::read_net_identity(payload, false);
            let _ = n::read_net_rejection(payload);
        }
    }
});
