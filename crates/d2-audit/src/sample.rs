//! Spot checks: which games and characters are audited without a reason.
//!
//! A pick is SHA-256 of the realm's secret sampling key, the game and the
//! lease, read as a number below 10,000 and compared with the share. The
//! key is made once per audit database and never leaves it, so a game
//! server can't tell which of its games will be audited, and the same game
//! always gets the same answer (a restart doesn't re-roll it).

use sha2::{Digest, Sha256};

/// The secret sampling key.
pub type SampleKey = [u8; 32];

/// A new random key.
pub fn new_key() -> SampleKey {
    let mut k = [0u8; 32];
    getrandom::fill(&mut k).expect("the operating system has a random source");
    k
}

fn draw(key: &SampleKey, what: u8, game: u32, lease: u64) -> u32 {
    let mut h = Sha256::new();
    h.update(b"d2-audit spot\0");
    h.update(key);
    h.update([what]);
    h.update(game.to_le_bytes());
    h.update(lease.to_le_bytes());
    let d = h.finalize();
    u32::from_le_bytes([d[0], d[1], d[2], d[3]]) % 10_000
}

fn under(draw: u32, pct: u32) -> bool {
    draw < pct.min(100) * 100
}

/// The spot check of an official game picks `game`.
pub fn official_game(key: &SampleKey, game: u32, pct: u32) -> bool {
    under(draw(key, 1, game, 0), pct)
}

/// The spot check of player-hosted games picks the character of `lease` in
/// `game` (another player than the host; the host's is audited every game).
pub fn hosted_seat(key: &SampleKey, game: u32, lease: u64, pct: u32) -> bool {
    under(draw(key, 2, game, lease), pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: SampleKey = [7; 32];

    #[test]
    fn shares_hold_over_many_games() {
        let n = 100_000u32;
        let official = (0..n).filter(|g| official_game(&KEY, *g, 1)).count();
        let hosted = (0..n)
            .filter(|g| hosted_seat(&KEY, *g, u64::from(*g) * 3 + 1, 5))
            .count();
        // 1% and 5% of 100,000, within a few standard deviations.
        assert!((850..=1150).contains(&official), "official {official}");
        assert!((4650..=5350).contains(&hosted), "hosted {hosted}");
    }

    #[test]
    fn edges_and_stability() {
        assert!((0..1000).all(|g| !official_game(&KEY, g, 0)));
        assert!((0..1000).all(|g| official_game(&KEY, g, 100)));
        // The same game gets the same answer; another key another set.
        let a: Vec<bool> = (0..2000).map(|g| official_game(&KEY, g, 5)).collect();
        let b: Vec<bool> = (0..2000).map(|g| official_game(&KEY, g, 5)).collect();
        let c: Vec<bool> = (0..2000).map(|g| official_game(&[8; 32], g, 5)).collect();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
