# AI 124 (FrozenHorror), as data

AI 124 (FrozenHorror) as a one-state graph: Arctic Blast when close, swing in reach, walk, stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 502 --ai 124 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-frozen-horror --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `124` then runs the graph instead of the Rust body.

## How close it is

Class 502 (frozen horror, AI 124), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 58.0 | 59.2 | +1.2 |
| ticks walking | 333.2 | 333.2 | 0 |
| ticks attack 1 | 3.2 | 3.2 | 0 |
| ticks casting (sequence) | 5.5 | 4.2 | -1.2 |
| closest / farthest (sub-tiles) | 5.6 / 24.6 | 5.6 / 24.6 | 0 |
| hit points the character lost | 0.9 | 0.9 | 0 |

No number is marked BIG. The horror is slow and reaches the character only at the end of the run, so the swing and the blast are
short samples.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body casts when the target is nearer than the *skill level* of skill 1 (4 for this row); a graph holds the number, so the
  distance is a constant 4. A row with another skill level needs its own graph.
* The body clears the blast's state (the inferno state, 12) on the next think; the graph does the same with `set_state`.
* The numbers are the first row's (Normal): 60, 40, 60, 10.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
