# Monster AI Graph (template)

A worked example of the mod kit's data AI, with no game files in it. Copy the folder, change the ids and the
numbers. The walk-through is in `docs/modkit/ai.md`.

What it does (`files/ai.toml`, graph `index = 200`):

* **stalk**: runs at the target. Hurt below 70% it calls its pack (once every 20 seconds). Within 14 subtiles
  and in sight it winds up for half a second (the graph reports a `leap` event for a telegraph effect to hang
  on), then uses its Skill2 (a 6 second cooldown).
* **fight**: swings when in reach, now and then circles, and goes back to stalking when the target backs off.
* **flee**: below 20% life it runs away until it has recovered to 50%.

A second entry (`index = 201`) is a *preset*: D2Gamma's Zombie AI with three of its knobs changed, no graph.

## Table rows this folder expects

* A `monstats` row with `ai = 200` and a `skill2` (the leap), and one with `ai = 201` for the preset. Any monster row can carry it; the graph does not
  need the original AI parameters.

Run `D2Gamma-Game check-mod` on the folder once that command is available (milestone K9); until then the tests
load this file.
