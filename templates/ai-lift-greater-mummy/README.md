# AI 22 (GreaterMummy), as data

AI 22 (GreaterMummy) as a one-state graph: swing, heal or raise other undead, cast the resurrection sequence, walk slowly.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 101 --ai 22 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-greater-mummy --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `22` then runs the graph instead of the Rust body.

## How close it is

Class 101 (unraveler, AI 22), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 144.4 | 139.9 | -4.5 |
| ticks walking | 116.6 | 117.0 | +0.4 |
| ticks sequence | 139.0 | 143.1 | +4.1 |
| closest / farthest (sub-tiles) | 24.8 / 24.9 | 24.8 / 24.8 | 0 |
| hit points the character lost | 9.1 | 9.5 | +0.4 |

**Close, no BIG number** (the first version had 4). The graph now has the body's scan: `hurt_within`, `corpse_within` and
`undead_within` read the same undead scan (radius 24), the heal (skill 2) and the raise (skill 1) are aimed `at = "hurt"` and
`at = "corpse"`, and the resurrection sequence goes to the second target. The arena has no other undead, so the scan finds
none, and the numbers show the cast and the slow walk only.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The heal and the raise are not exercised by the arena (no other undead in it); the unit tests of the undead scan cover them.
* Radament (class 229) runs this AI with a quest call and a wider scan; not in the graph.
* The numbers are class 101's (Normal): 70, 30, 40, 60, 24.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
