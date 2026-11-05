# AI 5 (BloodHawk), as data

AI 5 (BloodHawk) as a graph of two states (`circle` and `strike`): lunge, wander, back off, strike.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 15 --ai 5 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-blood-hawk --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `5` then runs the graph instead of the Rust body.

## How close it is

Class 15 (blood hawk, AI 5), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 26.5 | 66.1 | +39.6 **BIG** |
| ticks walking | 357.2 | 302.9 | -54.4 |
| ticks attack 1 | 16.2 | 31.0 | +14.8 |
| closest / farthest (sub-tiles) | 12.9 / 26.4 | 6.9 / 26.6 | -6.0 **BIG** / +0.2 |
| hit points the character lost | 1.0 | 2.0 | +1.0 |

**Partly close, 2 BIG.** The body keeps a flag in an AI param: a lunge sets it, and the next think in reach strikes at once. The
graph stands for that in two states and the think counter (`thinks_at_least`), which is right when the hawk comes in by its lunge but
not when the character walks into it, and the hawk gets in reach sooner in the graph (6.9 against 12.9 at the closest).
I did not find a better stand-in for the flag.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The flag of the lunge is not a graph value; the two states and the think counter are a stand-in that differs when the hawk
  is walked into.
* The body asks for speed 50 before it backs away and speed 100 before the lunge; `move_away` has no speed.
* The numbers are foulcrow1's (Normal): 30, 90, 5, 50, 100.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
