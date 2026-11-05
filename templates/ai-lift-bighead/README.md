# AI 4 (Bighead), as data

AI 4 (Bighead) as a graph of two states by life: `healthy` (close in and swing) and `hurt` (keep a distance).

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 10,581 --ai 4 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-bighead --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `4` then runs the graph instead of the Rust body.

## How close it is

Classes 10 and 581 (bigheads, AI 4), 8 seeds x 400 ticks, means. First the arena as it is (the monster is not hurt, so
only the `healthy` state runs), then with `--hurt 100:30` (the monster loses 30 percent of its life at tick 100, so the `hurt`
state runs):

| class | | Rust | data | diff |
|---|---|---|---|---|
| 10 | ticks standing | 114.1 | 114.1 | 0 |
| | ticks walking | 188.2 | 188.2 | 0 |
| | ticks attack 1 | 97.6 | 97.6 | 0 |
| | closest / farthest | 3.0 / 24.8 | 3.0 / 24.8 | 0 |
| | hit points the character lost | 24.5 | 27.1 | +2.6 |
| 10, hurt | ticks standing / walking / attack 2 | 135.4 / 186.0 / 78.6 | 135.4 / 186.0 / 78.6 | 0 |
| | closest | 4.1 | 4.1 | 0 |
| 581 | ticks standing | 211.1 | 193.1 | -18.0 |
| | ticks walking | 123.9 | 126.4 | +2.5 |
| | ticks attack 1 | 56.0 | 80.5 | +24.5 **BIG** |
| | ticks attack 2 | 9.0 | 0 | -9.0 |
| 581, hurt | ticks standing / walking / attack 2 | 242.9 / 121.1 / 36.0 | 242.9 / 121.1 / 36.0 | 0 |

Class 10 is the row the graph holds (aip3 = 0): identical to the tick in both phases. Class 581 (a later row, aip3 = 85) also
casts attack 2 inside 15 whenever a second target is in sight; the graph holds that as a commented action (take out the `#`
for those rows) and runs without it by default, so the healthy phase is off by one BIG number. The hurt phase, measured with the new
arena option, is identical for both classes.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* `aip3`, the chance to cast attack 2 when a second target is in sight, is 0 in classes 10 to 14 and 85 in classes 581 to 585;
  the graph holds 0 (class 581 needs the commented action).
* In the hurt phase the body asks for speed 50 before it backs away from a target nearer than 3; `move_away` has no speed.
* The numbers are Normal's (88, 40, 0, 60).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
