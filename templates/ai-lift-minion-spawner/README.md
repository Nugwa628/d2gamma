# AI 121 (MinionSpawner), as data

AI 121 (MinionSpawner) as a graph of three states: `watch`, `summon` (cast skill 1) and `wait`.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 485 --ai 121 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-minion-spawner --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `121` then runs the graph instead of the Rust body.

## How close it is

Class 485 (minion spawner, AI 121), 8 seeds x 400 ticks, means. The arena puts the character 25 sub-tiles away, outside
the spawner's range of `aip4` = 20, so the default run is 400 ticks of standing on both sides (identical, and no test of
anything else). With `--distance 15`:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 198.0 | 198.0 | 0 |
| ticks attack 1 (the cast) | 202.0 | 202.0 | 0 |
| closest / farthest (sub-tiles) | 14.6 / 14.6 | 14.6 / 14.6 | 0 |
| hit points the character lost | 29.1 | 29.1 | 0 |

The graph and the body cast at the same moments, to the tick, in the arena. (A first version waited 4 seconds after each
cast, which is the body's delay *plus* the cast; it cast 115 ticks of 202. The body's `aip3` = 100 frames counts from the start
of a cast.)

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The body stops casting while `aip2` = 25 of its minions are alive near it, and for good after `aip1` = 100 casts. A graph
  cannot count units of a class or keep a total, so the graph casts for ever, without the caps. In a real map that
  matters: **this graph would fill a level with minions**.
* The numbers are the first row's (Normal): 100, 25, 100, 20.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
