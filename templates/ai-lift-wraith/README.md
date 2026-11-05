# AI 9 (Wraith), as data

AI 9 (Wraith) as a one-state graph: swing in reach, otherwise float toward the target or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 42,632 --ai 9 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-wraith --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `9` then runs the graph instead of the Rust body.

## How close it is

Classes 42 (wraith5) and 632 (wraith6), AI 9, 8 seeds x 400 ticks, means:

| class | | Rust | data | diff |
|---|---|---|---|---|
| 42 | ticks standing | 115.6 | 118.6 | +3.0 |
| | ticks walking | 146.5 | 138.2 | -8.2 |
| | ticks attack 1 | 137.9 | 143.1 | +5.2 |
| | closest (sub-tiles) | 2.2 | 3.0 | +0.8 |
| | hit points the character lost | 34.4 | 33.1 | -1.2 |
| 632 | ticks standing / walking / attack 1 | 114.5 / 146.5 / 139.0 | 118.8 / 138.2 / 143.0 | +4.2 / -8.2 / +4.0 |
| | hit points the character lost | 25.1 | 24.4 | -0.8 |

No number is marked BIG. The body floats in hops of up to 12 and decides again after each, the graph's walk goes the whole
way; the arena shows only a few ticks of that (the wraith gets a little nearer in the body, 2.2 against 3.0).

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The hop (walk in radius of the target, 12) has no graph form; `move_to` goes all the way.
* The numbers are wraith5's (90, 4, 90), which wraith6 to 9 share; wraith1 to 4 have their own (50/12/70, 60/10/75, 70/8/80, 80/6/85).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
