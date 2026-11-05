# AI 116 (Minion), as data

AI 116 (Minion), without its orders: one state, `serve`.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 454 --ai 116 --seeds 5 --ticks 300 \
    --mod-dir templates/ai-lift-minion --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `116` then runs the graph instead of the Rust body.

## How close it is

Class 454 (minion1, AI 116), 5 seeds x 300 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 67.2 | 69.0 | +1.8 |
| ticks walking | 158.4 | 158.4 | 0 |
| ticks attack 1 | 74.4 | 57.2 | -17.2 |
| ticks attack 2 | 0.0 | 15.4 | +15.4 **BIG** |
| closest / farthest (sub-tiles) | 3.4 / 24.8 | 3.4 / 24.8 | 0 |
| hit points the character lost | 25.8 | 23.6 | -2.2 |

Walking and standing match to the tick. The "BIG" line is one of attack: the two attacks together are 74.4 against 72.6
ticks, so the monster fights as long; the Rust trace shows all of it as attack 1 and the graph splits it. I did not find
why the body's second attacks are not recorded as mode 5 (as far as I can tell the graph makes the same mode request).

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* **Orders.** The body obeys the command its leader gives (attack this unit, until a frame) and fights it first; a graph has
  no commands, so this is a minion that serves itself. Without a leader there is no difference.
* `aip2` = 15 is used twice by the body (the stall in frames, and the chance of the second attack); the graph has both as 15.
* The numbers are Normal's (70, 15, 50, 15, 50).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
