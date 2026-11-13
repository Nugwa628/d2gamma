# BUG-015: a resurrected hireling lies dead at the player's feet

- **Reported:** 2026-11-13, player report (game version not given).
- **What happens:** after a Resurrect at the hireling NPC, the hireling appears on the floor in her death pose next to the player and does not follow or fight.
- **Repro:** `tools/scenarios/bugs/BUG-015.scn` (checks that the resurrect takes its fee, the hireling's mode is no longer dead, and Esc closes the menu).
- **Cause:** the server's hireling rest only logged mode changes, so the resurrect's neutral mode never ran: the hireling got her life back and was warped to the player but stayed in her dead mode with no AI think. The gold checker also did not know the resurrect request (C->S 0x62) as a gold sink.
- **Fix:** hireling mode changes from the NPC desk and the hireling view go through the monster mode rules (`Desk::monster_mode`); 0x62 is a gold sink in the conservation checker. Unit test `resurrect_revives_a_real_mercenary` checks the unit's mode.
- **Status:** fix in PR #1320.
