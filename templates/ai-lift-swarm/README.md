# AI 19 (Swarm), as data

AI 19 (Swarm) as a one-state graph: swing in reach, walk at the target, stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 87 --ai 19 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-swarm --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `19` then runs the graph instead of the Rust body.

## How close it is

Class 87 (swarm, AI 19), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 124.4 | 124.4 | 0 |
| ticks walking | 225.5 | 225.5 | 0 |
| ticks attack 1 | 50.1 | 50.1 | 0 |
| closest / farthest (sub-tiles) | 3.1 / 24.8 | 3.1 / 24.8 | 0 |
| hit points the character lost | 9.5 | 9.5 | 0 |

No number differs: the graph and the body do the same in the arena, to the tick.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The numbers are swarm1's (Normal): 60, 20, 50. Swarm2 to 4 have the same; swarm5 differs only in speed.
* The body is shared with the goatman AI.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
