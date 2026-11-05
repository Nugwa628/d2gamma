# AI 8 (SandRaider), as data

AI 8 (SandRaider) as a graph of three states: `calm` (circle, swing, walk, stand), `powered` and `fire` (the fire hit).

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 29 --ai 8 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-sand-raider --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `8` then runs the graph instead of the Rust body.

## How close it is

Class 29 (sand raider, AI 8), 8 seeds x 400 ticks, means (the run with `--hurt 100:30` has the same numbers for both sides):

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 60.9 | 81.8 | +20.9 **BIG** |
| ticks walking | 308.4 | 277.8 | -30.6 |
| ticks attack 1 | 13.5 | 35.2 | +21.8 **BIG** |
| ticks attack 2 | 17.2 | 5.2 | -12.0 |
| closest / farthest (sub-tiles) | 10.5 / 24.9 | 10.8 / 24.9 | +0.2 / 0 |
| hit points the character lost | 3.0 | 8.0 | +5.0 |

**Partly close, 2 BIG.** The graph now counts thinks like the body (`thinks_at_least`: the 19th think powers up, and the think
after the fire hit starts the count again), so the power-ups come at the same moments in shape. The body stands `aidel` frames
at the power-up think and starts an overlay, which the graph does not do; the swing mix (attack 1 against attack 2) also differs in
the arena, and I did not find where that comes from.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* At the power-up think the body also starts an overlay (46, or 150 for the blue rows) and stands `aidel` frames; the graph sets
  the state (91; the blue rows use 90) and nothing more.
* Below `aip1` = 40 percent of life the body walks to the nearest evil monster (up to 7 times); a graph cannot pick that unit.
* The numbers are class 29's (Normal): 40, 70, 75, 70, 18, 0, 50. The later rows (classes 600 to 604) differ (speed 15 and
  the `seq_sandraiderflurry` skill).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
