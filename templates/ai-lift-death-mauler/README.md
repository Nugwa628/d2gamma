# AI 130 (DeathMauler), as data

AI 130 (DeathMauler) as a one-state graph: swing in reach, cast skill 1 when near, walk, stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 530 --ai 130 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-death-mauler --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `130` then runs the graph instead of the Rust body.

## How close it is

Class 530 (death mauler, AI 130), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 73.9 | 73.9 | 0 |
| ticks walking | 278.5 | 278.5 | 0 |
| ticks attack 1 | 37.0 | 37.0 | 0 |
| ticks casting (sequence) | 10.6 | 10.6 | 0 |
| closest / farthest (sub-tiles) | 3.2 / 24.6 | 3.2 / 24.6 | 0 |
| hit points the character lost | 18.1 | 18.1 | 0 |

No number differs: the graph and the body do the same in the arena, to the tick.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The numbers are the first row's (Normal): 75, 65, 15, 50.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
