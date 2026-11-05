# AI 3 (Zombie), as data

AI 3 (Zombie) as a one-state graph: swing in reach, run at a near target, wander.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 5 --ai 3 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-zombie --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `3` then runs the graph instead of the Rust body.

## How close it is

Class 5 (zombie, AI 3), 8 seeds x 400 ticks, means. The arena's default distance is 25 sub-tiles, and the zombie does
nothing but wander toward the character the whole run, so the swings are not tested. Run twice:

Default distance (25):

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 4.0 | 4.0 | 0 |
| ticks walking | 396.0 | 396.0 | 0 |
| closest / farthest (sub-tiles) | 21.9 / 25.5 | 21.9 / 25.5 | 0 |

With `--distance 6`:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 65.9 | 65.9 | 0 |
| ticks walking | 220.4 | 220.4 | 0 |
| ticks running | 29.6 | 29.6 | 0 |
| ticks attack 1 | 19.5 | 19.5 | 0 |
| ticks attack 2 | 64.6 | 64.6 | 0 |
| closest / farthest (sub-tiles) | 3.6 / 6.5 | 3.6 / 6.5 | 0 |
| hit points the character lost | 3.5 | 3.5 | 0 |

No number differs in either run: the graph and the body do the same, to the tick.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body has a branch for the AI state the game sets on a monster (state 3 or 19): it follows the target with a different step.
  A graph has no such state, so the lift holds the ordinary branch only.
* The body has a level rule for the level-17 zombies that the graph does not hold.
* The numbers are the first row's (Normal): aip1 = 30, aip2 = 10, aip4 = 20. The other zombies have their own (40/15/25, 50/20/30,
  100/30/35, 60/25/40).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
