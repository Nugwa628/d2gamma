# AI 37 (SkeletonBow), as data

AI 37 (SkeletonBow) as a one-state graph: shoot the second target inside 20, otherwise hop toward the target or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 170,171 --ai 37 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-skeleton-bow --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `37` then runs the graph instead of the Rust body.

## How close it is

Classes 170 and 171 (skeleton archers, AI 37), 8 seeds x 400 ticks, means:

| class | | Rust | data | diff |
|---|---|---|---|---|
| 170 | ticks standing | 157.6 | 157.6 | 0 |
| | ticks walking | 96.9 | 96.9 | 0 |
| | ticks attack 1 | 145.5 | 145.5 | 0 |
| | closest / farthest (sub-tiles) | 18.5 / 24.8 | 18.5 / 24.8 | 0 |
| | hit points the character lost | 7.0 | 7.0 | 0 |
| 171 | (same ticks and distances) | | | |
| | hit points the character lost | 10.8 | 10.8 | 0 |

**Identical to the tick.** The first version of this lift walked with `move_to` and was 3 BIG numbers off (it walked on to 11
sub-tiles where the body stops near 18). The body works on its second target (`at = "second"`, `second_distance_below`) and
closes in by hops (`hop_to`: a hop of 5 sub-tiles, keeping 6 from the target, then it thinks again); with those two the graph does the
same moves.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body's first step shoots at once when `ai_state_set` holds (the AI state 3 or 19 the game sets on a monster); a graph has no
  such state.
* The numbers are Normal's (75, 15, 50, 5, 6).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
