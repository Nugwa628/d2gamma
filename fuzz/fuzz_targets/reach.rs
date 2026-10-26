#![no_main]
//! Reachability (d2-reach): the punch datagram reader and the test a
//! shared socket uses to take reach datagrams off before QUIC, and the
//! default-gateway parsers (Linux `/proc/net/route`, Windows `route
//! print`). Nothing QUIC-shaped (fixed bit set) is ever a reach datagram;
//! a punch that reads writes back to the same bytes.
use d2_reach::gateway::{parse_proc_net_route, parse_route_print};
use d2_reach::wire::{is_reach, punch, read_punch};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    d2_fuzz::arm("reach", data);
    if data.first().is_some_and(|b| b & 0x40 != 0) {
        assert!(!is_reach(data), "a QUIC packet taken for a reach datagram");
    }
    if let Some(game) = read_punch(data) {
        assert!(is_reach(data));
        assert_eq!(punch(game), data);
    }
    if let Ok(text) = std::str::from_utf8(data) {
        let gws = parse_proc_net_route(text);
        assert!(gws.iter().all(|g| !g.is_unspecified()));
        let gws = parse_route_print(text);
        assert!(gws.iter().all(|g| !g.is_unspecified()));
    }
});
