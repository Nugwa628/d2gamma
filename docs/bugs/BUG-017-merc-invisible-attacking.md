# BUG-017: a hireling turns invisible while she attacks

- **Reported:** 2026-11-13, player report (game version not given; the act of the hireling not given).
- **What happens:** an act 3 or act 5 hireling (and an act 2 hireling's auras) disappears for the length of each skill use, then reappears.
- **Repro:** the real-data test `hireling_rows_find_the_act_of_a_name` (`crates/d2-client/tests/app_client_drlg.rs`); live, hire an act 3 hireling and let her cast: her client animation mode was 14 (sequence) with no sequence for her skill.
- **Cause:** the client built its `hireling` rows from the raw table set, where `NameFirst` / `NameLast` are still string keys and the name ids at +0x114 / +0x116 read 0. The client finds a hireling's act from her name id, so every hireling fell back to act 1 and got the rogue's row: skills 7 / 8 in mode 4 instead of her own skills and modes. Her real skills then took their `monanim` mode (sequence) from `skills.txt`, which only the act 2 hireling's Jab has a sequence for, so no animation was drawn.
- **Fix:** `d2_data::fixup::hireling_names` resolves the two name keys (the fix-up the server's table set already runs); the client's hireling rows run it before decoding. Unit test in `gaps_patch_fixup_tests.rs` checks it matches the full fix-up.
- **Status:** fix in PR #1320.
