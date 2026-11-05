# AI 119 (SuccubusWitch), as data

AI 119 (SuccubusWitch) as a one-state graph: swing in reach, bolt the second target, walk, circle or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 475 --ai 119 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-succubus-witch --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `119` then runs the graph instead of the Rust body.

## How close it is

Class 475 (succubus witch, AI 119), 8 seeds x 400 ticks, means (the run with `--hurt 100:30` is the same on both sides):

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 175.6 | 174.2 | -1.4 |
| ticks walking | 76.5 | 76.5 | 0 |
| ticks attack 1 | 65.0 | 66.4 | +1.4 |
| ticks skill 2 (the bolt) | 82.9 | 82.9 | 0 |
| closest / farthest (sub-tiles) | 13.5 / 24.8 | 13.5 / 24.8 | 0 |
| hit points the character lost | 34.4 | 35.8 | +1.4 |

**Close, no BIG number** (the first version had 5). The earlier version cast skill 2 where the row has a skill 5, the bolt, and
the body casts that one: at 90 percent, when a second target is in sight, then at 66 percent. With `at = "second"` and
`second_target` the graph now does the same. The gap the first version showed (the body swinging from about 13.5 sub-tiles
while the graph never did) came from that cast, not from the swing: `in_melee` is the body's own reach test, so no new
"combat range" condition was needed.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The row's curse and flee steps (aip3, aip4) are 0 in Normal and so are not in the graph; Nightmare and Hell use them (the curse
  tests are a cascade of skill tests a graph cannot hold).
* The numbers are the first row's (Normal): 90, 25, 0, 0, 90, 15, 80, 66.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
