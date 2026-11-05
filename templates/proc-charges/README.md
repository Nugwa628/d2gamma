# Proc Charges (template)

A worked example of the mod kit's skills, states and triggers, with no game files in it. Copy the folder, change
the ids and numbers, and add the table rows below. The walk-through is in `docs/modkit/skills.md`.

What it does:

* 8% of the unit's melee attacks open a one-second **reaction window** (state 190).
* Using skill 390, 391 or 392 while the window is open gives **+1 charge** (state 191, at most 5) and closes the
  window.
* A window that runs out unused **costs a charge**.
* With **3 or more charges**, skill 390 (Ember Lash) does 25% more damage.
* The HUD shows the charges as an icon with a count (the `icon` of state 191).

The server decides the window, so a laggy player gets the same window as everyone else. Making the window fair to
a high ping is a later milestone and is not built.

## Table rows this folder expects

The files here are the *rules*; the game still needs rows for them in its tables (a table patch of the mod):

* `skills` rows 390, 391, 392: `srvdofunc = 400` (the block slot), plus the mana cost, animations, range and level
  columns you want. Row 393 is the passive Opportunist; the unit must have it.
* `states` rows 190 and 191 (a state row has to exist in the game's `states` table).
* an `itemstatcost` row 340 (the bonus damage percent stat that state 191 gives and skill 390 reads).
