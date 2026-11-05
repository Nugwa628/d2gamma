# AI 10 (CorruptRogue), as data

AI 10 (CorruptRogue) as a one-state graph: run in from far, swing in reach, otherwise stand, run or walk.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 43 --ai 10 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-corrupt-rogue --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `10` then runs the graph instead of the Rust body.

## How close it is

Class 43 (corrupt rogue, AI 10), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 171.5 | 166.4 | -5.1 |
| ticks walking | 12.4 | 8.5 | -3.9 |
| ticks running | 91.6 | 93.1 | +1.5 |
| ticks attack 1 | 124.5 | 132.0 | +7.5 |
| closest / farthest (sub-tiles) | 3.0 / 24.8 | 3.0 / 24.8 | 0 |
| hit points the character lost | 7.0 | 7.1 | +0.1 |

No number is marked BIG. The graph's chances are the body's, worked out to one draw per action; what is left is the draws
landing differently.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body runs in when the target is farther than `20 - 3 * difficulty` subtiles; a graph holds one number, and this one holds
  Normal's 20.
* The numbers are the first row's (Normal): 60, 15, 75, 100, 20.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
