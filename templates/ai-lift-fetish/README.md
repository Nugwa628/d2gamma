# AI 30 (Fetish), as data

AI 30 (Fetish) as a graph of three states: `approach`, `fight`, `back_off`.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 141,142,143 --ai 30 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-fetish --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `30` then runs the graph instead of the Rust body.

## How close it is

Classes 141, 142 and 143 (fetishes, AI 30), 8 seeds x 400 ticks, means:

| class | | Rust | data | diff |
|---|---|---|---|---|
| 141 | ticks standing | 178.2 | 178.2 | 0 |
| | ticks walking | 123.9 | 123.9 | 0 |
| | ticks attack 1 | 97.9 | 97.9 | 0 |
| | closest / farthest | 2.8 / 24.8 | 2.8 / 24.8 | 0 |
| | hit points the character lost | 40.0 | 40.0 | 0 |
| 142 | ticks standing / walking / attack 1 | 234.2 / 111.4 / 54.4 | 234.2 / 111.4 / 54.4 | 0 |
| 143 | ticks standing / walking / attack 1 | 260.6 / 103.1 / 36.2 | 260.6 / 103.1 / 36.2 | 0 |

The graph and the body do the same in the arena, to the tick. Before every walk the body stages a velocity request
(method 13, speed 50), and the graph's `move_to` asks for the same with `speed = 50`. (Before `move_to` had a speed the graph
walked about 70 ticks more in the first approach, and the report had three BIG rows.)

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body leaves the fight phase after `aip3` = 4 thinks **and** only if the target's life is above `aip4` = 33 percent;
  the graph leaves after 4 seconds in `fight` and cannot read the target's life. The arena's target never gets that low, so
  the report cannot show it.
* The body follows an order (a command of kind 1 or 14: go and walk to a unit); a graph has no commands.
* The numbers are Normal's (100, 10, 4, 33).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
