# AI 17 (SandLeaper), as data

AI 17 (SandLeaper) as a one-state graph: leap when near, swing 2 in reach, close in, walk, circle or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 78 --ai 17 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-sand-leaper --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `17` then runs the graph instead of the Rust body.

## How close it is

Class 78 (sand leaper, AI 17), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 202.9 | 176.8 | -26.1 |
| ticks walking | 102.4 | 90.2 | -12.1 |
| ticks attack 2 | 61.1 | 67.1 | +6.0 |
| ticks casting (the leap) | 33.6 | 65.9 | +32.2 **BIG** |
| closest / farthest (sub-tiles) | 2.0 / 24.8 | 1.2 / 24.8 | -0.8 / 0 |
| hit points the character lost | 23.6 | 28.6 | +5.0 |

**Close, with one BIG number.** The graph leaps about twice as often as the body in the arena. The body tests its skill
(`skill_check`) before it leaps, and that test fails in the cases the graph's `use_skill` does not rule out; the 20 percent chance is
the same on both sides.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The far walk of the body is `wander_near` the target at velocity 75 (a walk to a random spot beside it); the graph walks at
  the target, 75 percent faster.
* The numbers are the first row's (Normal): 20, 50, 30, 50.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
