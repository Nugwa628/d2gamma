# AI 39 (PinHead), as data

AI 39 (PinHead) as a graph of two states (`approach` and `fight`): walk, stand, Smite or swing in reach.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 186 --ai 39 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-pin-head --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `39` then runs the graph instead of the Rust body.

## How close it is

Class 186 (blunderbore, AI 39), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 32.4 | 32.4 | 0 |
| ticks walking | 357.4 | 357.4 | 0 |
| ticks attack 1 | 1.0 | 1.0 | 0 |
| ticks sequence (Smite) | 9.2 | 9.2 | 0 |
| closest / farthest (sub-tiles) | 4.1 / 24.6 | 4.1 / 24.6 | 0 |
| hit points the character lost | 4.2 | 4.2 | 0 |

No number differs: the graph and the body do the same in the arena, to the tick. The blunderbore is slow and reaches the
character only at the end of the run, so the in-reach part is a short sample.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body keeps a flag in an AI param (cleared by any think out of reach, set at the first think in reach); the graph keeps it
  with two states and the think counter, which differs when the monster is walked into.
* The numbers are blunderbore1's (Normal): 75, 15, 60, 12, 40.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
