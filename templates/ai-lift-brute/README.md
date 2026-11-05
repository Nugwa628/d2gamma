# AI 7 (Brute), as data

AI 7 (Brute) as a one-state graph: swing 1 or 2 in reach, circle or stand, otherwise walk faster the more hurt.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 447,448 --ai 7 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-brute --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `7` then runs the graph instead of the Rust body.

## How close it is

Classes 447 and 448 (snow yetis, AI 7), 8 seeds x 400 ticks, means:

| class | | Rust | data | diff |
|---|---|---|---|---|
| 447 | ticks standing | 119.5 | 125.4 | +5.9 |
| | ticks walking | 188.5 | 188.4 | -0.1 |
| | ticks attack 1 / 2 | 25.4 / 66.6 | 14.5 / 71.8 | -10.9 / +5.1 |
| | closest | 3.0 | 3.0 | 0 |
| | hit points the character lost | 40.0 | 39.6 | -0.4 |
| 448 | ticks standing | 148.2 | 133.5 | -14.8 |
| | ticks walking | 161.9 | 161.8 | -0.1 |
| | ticks attack 1 / 2 | 18.1 / 71.8 | 36.2 / 68.5 | +18.1 / -3.2 |
| | hit points the character lost | 40.0 | 39.1 | -0.9 |

No number is marked BIG. The approach matches to the tick. The split between swing 1 and 2 is a matter of a few swings
out of 90 percent draws made in a different order.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body's walking speed is 100 minus its life percent (between 40 and 100); the graph has three steps of it, by
  `life_below`. The arena never hurts the monster, so this report does not exercise them.
* The body asks for the speed with method 13 (a different velocity request from the plain one the graph makes).
* The numbers are the yetis' (aip3 90, aip4 40); the brute rows have 75/100, 80/85, 85/75, 90/60 and 100/45.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
