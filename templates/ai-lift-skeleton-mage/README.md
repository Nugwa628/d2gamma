# AI 64 (SkeletonMage), as data

AI 64 (SkeletonMage) as a one-state graph: close in, cast, circle, stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 274,379 --ai 64 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-skeleton-mage --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `64` then runs the graph instead of the Rust body.

## How close it is

Classes 274 and 379 (skeleton mages, AI 64), 8 seeds x 400 ticks, means (the two classes differ only in
the hit points the character lost):

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 81.4 | 94.0 | +12.6 |
| ticks walking | 232.6 | 230.1 | -2.5 |
| ticks attack 1 | 86.0 | 75.9 | -10.1 |
| closest (sub-tiles) | 7.0 | 6.8 | -0.2 |
| farthest (sub-tiles) | 24.8 | 24.8 | 0 |
| hit points the character lost (274 / 379) | 2.0 / 7.2 | 1.6 / 6.1 | -0.4 / -1.1 |

No number is marked BIG. The body walks in short hops (`move_steps` of aip2 = 9 at a slowed velocity) and thinks again
between them. The graph's `move_to` asks for the same two things, `steps = 9` and `speed = 10`, and so stops walking and
casts at about the same distance. (Before `move_to` had `steps` and `speed` it walked the whole way to the target and cast
a fifth as often.) What is left is the graph choosing by state and time where the body counts thinks.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The graph's cast is a little rarer (75.9 ticks against 86.0), because it thinks on time and not on the body's think counter.
* The body works on the *secondary target* (the nearest enemy it sees, which is not always the one it follows); the graph has
  only the main target.
* The body's retreat branch (aip4/aip5, back off when the target is within 5) has a chance of 0 in every row (aip5 = 0), so it
  is not in the graph.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
