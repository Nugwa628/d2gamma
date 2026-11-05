# AI 75 (QuillMother), as data

AI 75 (QuillMother) as a one-state graph: swing in reach, walk at the target, stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 313 --ai 75 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-quill-mother --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `75` then runs the graph instead of the Rust body.

## How close it is

Class 313 (quill bear, AI 75), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 103.6 | 103.6 | 0 |
| ticks walking | 265.5 | 265.5 | 0 |
| ticks attack 1 | 30.9 | 30.9 | 0 |
| closest / farthest (sub-tiles) | 4.1 / 24.6 | 4.1 / 24.6 | 0 |
| hit points the character lost | 0.0 | 0.0 | 0 |

No number differs: the graph and the body do the same in the arena, to the tick.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body has a branch for the AI state the game sets on a monster (state 3 or 19): it orders its minions to the target and goes
  at it. A graph has no such state, so the lift holds the ordinary branch only.
* The numbers are quillbear1's (Normal): 60, 50, 16, 15.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
