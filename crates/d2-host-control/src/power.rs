//! Windows sleep notices. The game client's window receives
//! `WM_POWERBROADCAST`; when it says the PC is about to sleep
//! (`PBT_APMSUSPEND`), the client passes `sleep` on to its hosting process,
//! which saves everyone and closes the game while it still can. Plain
//! numbers, so this builds and is tested on every platform.

/// The window message of power events.
pub const WM_POWERBROADCAST: u32 = 0x0218;

/// `WM_POWERBROADCAST`'s `wParam`: the PC is about to be suspended.
pub const PBT_APMSUSPEND: usize = 0x0004;

/// Whether a window message is the sleep notice.
pub fn is_sleep_notice(msg: u32, wparam: usize) -> bool {
    msg == WM_POWERBROADCAST && wparam == PBT_APMSUSPEND
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_suspend_notice_is_sleep() {
        assert!(is_sleep_notice(0x0218, 4));
        // Resume (PBT_APMRESUMEAUTOMATIC) and battery changes are not.
        assert!(!is_sleep_notice(0x0218, 0x12));
        assert!(!is_sleep_notice(0x0218, 0xA));
        // Another message with the same wParam.
        assert!(!is_sleep_notice(0x0010, 4));
    }
}
