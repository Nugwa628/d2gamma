# AI 18 (PantherWoman), as data

AI 18 (PantherWoman) as a one-state graph: swing in reach, run at the target, circle or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 83 --ai 18 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-panther-woman --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `18` then runs the graph instead of the Rust body.

## How close it is

Class 83 (panther woman, AI 18), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 117.5 | 117.5 | 0 |
| ticks walking | 126.9 | 126.9 | 0 |
| ticks attack 1 | 155.6 | 155.6 | 0 |
| closest / farthest (sub-tiles) | 2.9 / 24.8 | 2.9 / 24.8 | 0 |
| hit points the character lost | 22.8 | 22.8 | 0 |

No number differs: the graph and the body do the same in the arena, to the tick (the arena has no pack, so the pack step
below is not tested).

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body looks for a pack mate and walks to it when it is farther than aip3 = 8 away, before it circles or stands. That is a
  scan of the monster's pack, which a graph has no condition for, so the graph leaves it out. A lone panther woman does the same
  as the body; one in a pack stays less close to it.
* The numbers are the first row's (Normal): 70, 70, 8, 6, 0.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
