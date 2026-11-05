# AI 12 (Goatman), as data

AI 12 (Goatman) as a one-state graph: swing in reach, otherwise walk toward the target or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 53 --ai 12 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-goatman --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `12` then runs the graph instead of the Rust body.

## How close it is

Class 53 (goatman, AI 12), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 80.4 | 80.4 | 0 |
| ticks walking | 225.9 | 225.9 | 0 |
| ticks attack 1 | 93.8 | 93.8 | 0 |
| closest / farthest (sub-tiles) | 3.0 / 24.8 | 3.0 / 24.8 | 0 |
| hit points the character lost | 8.8 | 8.8 | 0 |

The graph and the body do the same in the arena, to the tick. (The body is shared with Skeleton, Wraith and Swarm, each
with its own twist; this lift is the plain one.)

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The numbers are Normal's (75, 10, 80), and the rows of the later goatmen (classes 586 to 590) have the same Normal
  numbers. Nightmare and Hell differ.
* The same function serves the swarm (AI 19) with its own monstats numbers; `index = 19` in this graph would be a lift of the swarm if its numbers match.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
