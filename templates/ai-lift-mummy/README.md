# AI 21 (Mummy), as data

AI 21 (Mummy) as a one-state graph: wander and stand when far, swing 1 or 2 in reach, walk when near.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 96 --ai 21 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-mummy --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `21` then runs the graph instead of the Rust body.

## How close it is

Class 96 (mummy, AI 21), 8 seeds x 400 ticks, means. At the arena's default distance (25) the mummy only wanders
and never gets within reach of the character, so the table below is a second run with `--distance 6`:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 163.8 | 158.9 | -4.9 |
| ticks walking | 134.9 | 134.9 | 0 |
| ticks attack 1 | 77.0 | 77.0 | 0 |
| ticks attack 2 | 24.4 | 29.2 | +4.9 |
| closest / farthest (sub-tiles) | 4.2 / 7.1 | 4.2 / 7.1 | 0 |
| hit points the character lost | 19.6 | 20.0 | +0.4 |

The default run is identical to the tick (97.1 ticks standing, 302.9 walking, closest 21.0). In the near run no number is
marked BIG.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body has a branch for the AI state the game sets on a monster (state 3 or 19); a graph has no such state.
* The numbers are the first row's (Normal): 5, 60, 100, 65, 10. The other mummies differ in aip1 (10 to 25), aip2 and aip4.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
