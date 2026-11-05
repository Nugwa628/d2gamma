# AI 1 (Idle), as data

AI 1 (Idle) as an ai.toml graph: one state, `stand`, that idles for 8 seconds, over and over.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 149,151,152 --ai 1 --seeds 6 --ticks 200 \
    --mod-dir templates/ai-lift-idle --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `1` then runs the graph instead of the Rust body.

## How close it is

Classes 149, 151 and 152 (AI 1), 6 seeds x 200 ticks, means:

| | Rust | data |
|---|---|---|
| ticks standing | 200.0 | 200.0 |
| closest / farthest (sub-tiles) | 25.2 / 25.2 | 25.2 / 25.2 |
| hit points either side lost | 0 | 0 |

No number differs. That is all there is to see: the body idles 200 frames when it has a room, and does nothing else.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* Nothing is left out of the Rust body that a graph could say.
* The graph has no target (`target = "none"`), as the body has none.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
