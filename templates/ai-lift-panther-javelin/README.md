# AI 95 (PantherJavelin), as data

AI 95 (PantherJavelin) as a one-state graph: back off when close, close in, throw inside 20, otherwise stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 373 --ai 95 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-panther-javelin --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `95` then runs the graph instead of the Rust body.

## How close it is

Class 373 (slinger, AI 95), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 93.4 | 83.4 | -10.0 |
| ticks walking | 246.5 | 217.9 | -28.6 |
| ticks running | 0 | 20.0 | +20.0 |
| ticks attack 1 | 60.1 | 78.8 | +18.6 |
| closest / farthest (sub-tiles) | 4.1 / 24.8 | 3.8 / 24.8 | -0.4 / 0 |
| hit points the character lost | 9.6 | 13.0 | +3.4 |

No number is marked BIG. (A first version that read "wander near the target" as the `wander` action stayed at 19.8 from
the character against 4.1; the body's wander is a walk to a spot within 4 of the target, so `move_to` is the right action.)
The graph runs away a little (20 ticks) where the body does not in the arena: the body's retreat draw is made on the second
target's distance, and a graph has only the main target.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body works on its *secondary target* (the nearest enemy); the graph on the main target.
* With nothing in range the body walks to a pack mate that is farther than `aip3` = 12; a graph cannot find one and stands.
* `move_to` replaces "wander near the target"; the real wander is a random spot within 4 of the target.
* The numbers are class 373's (Normal): 70, 70, 12, 10, 15, 20; the four slingers differ only in aip2 (70, 75, 80, 85).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
