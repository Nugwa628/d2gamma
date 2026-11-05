# AI 16 (ClawViper), as data

AI 16 (ClawViper) as a one-state graph: strike in reach, SerpentCharge when near, walk or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 73 --ai 16 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-claw-viper --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `16` then runs the graph instead of the Rust body.

## How close it is

Class 73 (claw viper, AI 16), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 100.4 | 88.8 | -11.6 |
| ticks walking | 270.8 | 270.8 | 0 |
| ticks attack 1 | 8.5 | 17.0 | +8.5 |
| ticks attack 2 | 16.0 | 19.1 | +3.1 |
| ticks casting (the charge) | 4.4 | 4.4 | 0 |
| closest / farthest (sub-tiles) | 4.0 / 24.6 | 4.0 / 24.6 | 0 |
| hit points the character lost | 5.5 | 10.0 | +4.5 |

No number is marked BIG. The strikes are few (the viper is in reach for a short time), so their share moves with the draws.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* From its first charge on the body turns on a glow state (aip6); it is a look only and the graph leaves it out.
* The strike is one draw of 75 percent, then half and half between swing 1 and 2; the graph draws 38 percent swing 1, then 60
  percent of the rest swing 2 (37 and 37 of every 100).
* The numbers are the first row's (Normal): 40, 8, 75, 50, 15, 1.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
