# AI 28 (Vampire), as data

AI 28 (Vampire) as a graph of five states: `dormant`, `normal`, `near`, `melee` and `retreat`.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 131 --ai 28 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-vampire --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `28` then runs the graph instead of the Rust body.

## How close it is

Class 131 (vampire, AI 28), 8 seeds x 400 ticks, means. The arena puts the character 25 sub-tiles away. The body stays
asleep until its target is inside `aip3` = 24, so the default run is 400 ticks of standing on both sides (identical). With
`--distance 18` (the character inside 24):

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 112.4 | 136.6 | +24.2 |
| ticks walking | 197.4 | 154.2 | -43.1 |
| ticks attack 1 | 12.2 | 30.5 | +18.2 |
| ticks casting | 78.0 | 78.6 | +0.6 |
| closest / farthest (sub-tiles) | 9.6 / 18.8 | 6.1 / 18.8 | -3.5 / 0 |
| hit points the character lost | 22.2 | 31.2 | +9.0 **BIG** |

With `--distance 18 --hurt 100:20` (the retreat phase): standing 176.1 / 213.4, walking 173.1 / 124.6 (**BIG**), casting 50.8 / 62.0,
closest 13.9 / 10.9; 1 BIG number of 8.

**Closer than before** (the first version had 3 BIG numbers, then 2). The casts now match to a tenth of a tick; what is left is that
the graph comes in closer and swings more. The body's near-but-not-in-reach step is rebuilt as two states (`normal` and
`near`, by distance 9) with the chances worked out per distance, as the comments in the file say.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The upgrade (skill 2) has a cooldown of 11 thinks in the body; the graph draws it with no cooldown.
* The body's phase lives in its AI params (0 asleep, 1 normal, 2 retreat); the graph's states are the same, plus `near` and
  `melee` because a state holds at most eight actions. The switch to `melee` is by `in_melee`, to `near` by distance (9).
* The body asks for a run bonus (speed) before it backs away; `move_away` has no speed.
* The numbers are vampire1's (Normal): 55, 20, 24, 25, 3. The others differ (e.g. aip2 25 to 40).

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
