# BUG-016: a hireling left dead in the wild can't be resurrected and the NPC menu hangs

- **Reported:** 2026-11-13, player report (game version not given; "cannot walk and the mouse freezes after reviving them at the NPC").
- **What happens:** the hireling dies, the player goes back to town; a while later the Resurrect at the NPC does nothing, the menu disappears and Esc no longer ends the talk.
- **Repro:** `tools/scenarios/bugs/BUG-016.scn` (waits until her room goes inactive; checks she is still a unit, the resurrect works and Esc closes the menu).
- **Cause:** when the room of a dead hireling went inactive, the inactive store freed her unit: the player-pet rule needs the owner flag and a player pet, and the pet test never counted hirelings. The resurrect was then refused (S->C 0x2A result 9). On any refused service the client closed the NPC menu and never built it again, so the talk could not be ended with Esc or Cancel.
- **Fix:** a hireling counts as a player pet for the inactive store (kept and detached, her path's room cleared); the client builds the NPC menu again after a refused service. Tests `a_dead_hireling_is_kept_when_her_room_goes_inactive` and `a_refused_resurrect_builds_the_menu_again_and_esc_ends_it`.
- **Status:** fix in PR #1320.
