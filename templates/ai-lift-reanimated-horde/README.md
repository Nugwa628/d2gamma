# AI 114 (ReanimatedHorde), as data

AI 114 (ReanimatedHorde) as a one-state graph: swing in reach, charge from mid range, walk, circle or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 437 --ai 114 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-reanimated-horde --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `114` then runs the graph instead of the Rust body.

## How close it is

Class 437 (reanimated horde, AI 114), 8 seeds x 400 ticks, means. The horde walks slowly (velocity 1), so in 400 ticks of the
arena's default distance it never comes within the 12 sub-tiles of its charge, or within reach:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 34.0 | 40.0 | +6.0 |
| ticks walking | 366.0 | 360.0 | -6.0 |
| closest / farthest (sub-tiles) | 17.6 / 24.8 | 18.1 / 24.8 | +0.5 / 0 |
| hit points either side lost | 0 | 0 | 0 |

No number is marked BIG. **The swing and the charge are not exercised by this run**: only the walk, the circle and the stand are.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body marks the unit as landed on every think (a flag with no use in a graph).
* `line_of_sight` stands for the body's "can reach directly" test of the charge.
* The numbers are the first row's (Normal): 30, 20, 12, 20, 20, 65, 25, 30.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
