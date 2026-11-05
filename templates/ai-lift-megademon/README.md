# AI 89 (Megademon), as data

AI 89 (Megademon) as a graph of two states (`range` and `melee`): Inferno when near, swing, circle, walk, stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 360 --ai 89 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-megademon --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `89` then runs the graph instead of the Rust body.

## How close it is

Class 360 (megademon, AI 89), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 172.0 | 172.0 | 0 |
| ticks walking | 111.6 | 111.6 | 0 |
| ticks attack 1 | 96.2 | 96.2 | 0 |
| ticks skill 1 (the inferno) | 20.1 | 20.1 | 0 |
| closest / farthest (sub-tiles) | 3.5 / 24.6 | 3.5 / 24.6 | 0 |
| hit points the character lost | 36.2 | 36.2 | 0 |

No number differs: the graph and the body do the same in the arena, to the tick. The body's wait for the next cast (aip6 = 75
frames) is the graph's cooldown (`starts_cooldown`, `cooldown_ready`); the two states switch with `in_melee` and `out_of_melee`.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body casts when the target is nearer than the *skill level* of skill 1 (6 for this row); a graph holds the number.
* The body clears the inferno state (12) on the next think; the graph does the same with `set_state`.
* The numbers are megademon1's (Normal): 50, 40, 80, 70, 50, 75.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
