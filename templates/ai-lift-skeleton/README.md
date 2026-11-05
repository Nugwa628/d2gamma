# AI 2 (Skeleton), as data

AI 2 (Skeleton) as a one-state graph: swing 1 or 2 in reach, otherwise walk toward the target or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 4,391 --ai 2 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-skeleton --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `2` then runs the graph instead of the Rust body.

## How close it is

Classes 4 (skeleton5) and 391 (hellbovine), AI 2, 8 seeds x 400 ticks, means:

| class | | Rust | data | diff |
|---|---|---|---|---|
| 4 | ticks standing | 74.0 | 74.0 | 0 |
| | ticks walking | 225.9 | 225.9 | 0 |
| | ticks attack 1 | 30.1 | 20.6 | -9.5 |
| | ticks attack 2 | 70.0 | 79.5 | +9.5 |
| | closest / farthest | 3.0 / 24.8 | 3.0 / 24.8 | 0 |
| | hit points the character lost | 20.0 | 21.4 | +1.4 |
| 391 | ticks standing | 108.4 | 102.6 | -5.8 |
| | ticks walking | 222.6 | 222.6 | 0 |
| | ticks attack 1 / 2 | 17.2 / 51.8 | 23.0 / 51.8 | +5.8 / 0 |
| | hit points the character lost | 39.4 | 40.0 | +0.6 |

No number is marked BIG. The approach matches to the tick; the split between swing 1 and swing 2 differs by a few swings
because the graph draws its two chances in sequence and the body draws them nested.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The numbers are skeleton5's (99, 5, 95, 33), which skeleton6 to 8 and the hellbovine share; skeleton1 to 4 have their own
  (60/15/75/75, 70/12/80/25, 80/9/85/50, 90/7/90/66).
* The skeletons that rise from the dead (the Necromancer's) run another AI.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
