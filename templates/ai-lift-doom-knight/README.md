# AI 72 (DoomKnight), as data

AI 72 (DoomKnight) as a one-state graph: swing in reach, walk at the target, stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 310 --ai 72 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-doom-knight --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `72` then runs the graph instead of the Rust body.

## How close it is

Class 310 (doom knight, AI 72), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 121.0 | 121.0 | 0 |
| ticks walking | 186.4 | 186.4 | 0 |
| ticks attack 1 | 92.6 | 92.6 | 0 |
| closest / farthest (sub-tiles) | 3.2 / 24.8 | 3.2 / 24.8 | 0 |
| hit points the character lost | 40.0 | 40.0 | 0 |

No number differs: the graph and the body do the same in the arena, to the tick.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The numbers are the first row's (Normal): 90, 10, 80, 10. The dkfig rows (classes 700 and 701) have the same numbers.
* The oblivion knight (AI 74) is another AI and is not covered.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
