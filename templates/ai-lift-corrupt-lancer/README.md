# AI 36 (CorruptLancer), as data

AI 36 (CorruptLancer) as a one-state graph: run in from afar, swing in reach, walk short steps in between.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 165 --ai 36 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-corrupt-lancer --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `36` then runs the graph instead of the Rust body.

## How close it is

Class 165 (corrupt lancer, AI 36), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 129.1 | 129.2 | +0.1 |
| ticks walking | 4.2 | 4.2 | 0 |
| ticks running | 117.8 | 117.8 | 0 |
| ticks attack 1 | 148.9 | 148.8 | -0.1 |
| closest / farthest (sub-tiles) | 3.4 / 24.8 | 3.4 / 24.8 | 0 |
| hit points the character lost | 9.5 | 9.6 | +0.1 |

The graph and the body do the same in the arena, within a tick.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body remembers in its AI params that it ran in from afar, and then swings without the 75 percent draw; a graph cannot.
* The rows of the later lancers (classes 616 to 618) have skill chances (aip6 to aip8: 20, 20) for the jab sequence and a
  second skill; the graph holds none.
* The numbers are class 165's (Normal): 60, 75, 9, 0, 15.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
