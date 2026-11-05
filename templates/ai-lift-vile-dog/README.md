# AI 69 (VileDog), as data

AI 69 (VileDog) as a graph of two states: `wake` (one short stand), then `hunt` (swing, walk or stand).

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 301,552 --ai 69 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-vile-dog --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `69` then runs the graph instead of the Rust body.

## How close it is

Classes 301 (vile dog) and 552 (pain worm), AI 69, 8 seeds x 400 ticks, means:

| class | | Rust | data | diff |
|---|---|---|---|---|
| 301 | ticks standing | 165.2 | 165.2 | 0 |
| | ticks walking | 142.6 | 142.6 | 0 |
| | ticks attack 1 | 92.1 | 92.1 | 0 |
| | closest / farthest | 3.0 / 24.8 | 3.0 / 24.8 | 0 |
| | hit points the character lost | 34.9 | 34.9 | 0 |
| 552 | ticks standing | 138.0 | 138.0 | 0 |
| | ticks walking | 142.6 | 142.6 | 0 |
| | ticks attack 1 | 119.4 | 119.4 | 0 |
| | hit points the character lost | 30.1 | 30.1 | 0 |

The graph and the body do the same in the arena, to the tick, for both classes.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The numbers are Normal's (80, 10, 80), the same in every row of the AI.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
