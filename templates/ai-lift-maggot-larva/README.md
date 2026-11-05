# AI 38 (MaggotLarva), as data

AI 38 (MaggotLarva) as a one-state graph: swing, then stand once, in reach; walk or stand out of reach.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 180 --ai 38 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-maggot-larva --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `38` then runs the graph instead of the Rust body.

## How close it is

Class 180 (maggot larva, AI 38), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 88.6 | 81.6 | -7.0 |
| ticks walking | 276.6 | 276.6 | 0 |
| ticks attack 1 | 34.8 | 41.8 | +7.0 |
| closest / farthest (sub-tiles) | 3.5 / 24.8 | 3.5 / 24.8 | 0 |
| hit points the character lost | 3.2 | 3.8 | +0.5 |

No number is marked BIG. The body keeps a flag in an AI param (after a swing it stands once before it can swing again); the graph
keeps it with a cooldown on the swing (`starts_cooldown` and `cooldown_ready`).

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The cooldown is a time (1.2 seconds), where the body's flag is "the next think"; a swing that takes longer or shorter than this
  one moves the share of swings and stands a little.
* The numbers are the first row's (Normal): 75, 20, 60, 15, 20.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
