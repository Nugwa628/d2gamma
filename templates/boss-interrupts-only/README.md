# Boss Interrupts Only (template)

A worked example of the mod kit's declared crowd control, with no game files in it. Copy the folder, change the
ids and the numbers. The walk-through is in `docs/modkit/crowd-control.md`.

What it does:

* **Players** are stunned, interrupted, knocked back or frozen **only by sources that say so** (`files/rules.toml`,
  `[players] crowd_control = "declared"`). A stray arrow, a thrown axe or a small hit no longer cancels a
  teleport or a cast.
* The boss (monstats row 156 in the example) **interrupts** with its first attack and **stuns for 1.5 seconds**
  with its first special attack (`files/crowd_control.toml`).
* One missile row may **knock back**, one time in four.
* The `Shockwave` skill (`files/skills.toml`, skills row 395) is an example of a skill that stuns because its
  area part declares it.
* A cast **cannot be cancelled** once 40% of its animation has played, and Teleport (row 54) is committed from the
  first frame (`[cast]` in `files/rules.toml`).
* **Monsters** keep D2Gamma's rules; only the player side is changed.

## Table rows this folder expects

* `skills` row 395: `srvdofunc = 400` (the block slot), plus mana cost, animations, range and level columns.
* `monstats` row 156 and `missiles` row 214: change the numbers in `crowd_control.toml` to the rows of your boss
  and projectile.

Run `D2Gamma-Game check-mod` on the folder once that command is available (milestone K9); until then the tests
load these three files.
