# AI 125 (BloodLord), as data

AI 125 (BloodLord) as a one-state graph: in reach cast skill 1 or swing, out of reach walk or stand.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 507 --ai 125 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-blood-lord --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `125` then runs the graph instead of the Rust body.

## How close it is

Class 507 (blood lord, AI 125), 8 seeds x 400 ticks, means:

| | Rust | data | diff |
|---|---|---|---|
| ticks standing | 190.6 | 187.9 | -2.8 |
| ticks walking | 123.9 | 123.9 | 0 |
| ticks attack 1 | 40.5 | 43.2 | +2.8 |
| ticks attack 2 | 45.0 | 45.0 | 0 |
| closest / farthest (sub-tiles) | 3.5 / 24.6 | 3.5 / 24.6 | 0 |
| hit points the character lost | 40.0 | 40.0 | 0 |

No number is marked BIG. The body decides between its skill and a swing with two draws (90 percent act, then 50 percent skill,
then 82 percent of the rest swing 1 and the others swing 2); the graph draws once per action, with the numbers worked out so
that the shares match (45 percent skill, 40 swing 1, 5 swing 2 of every 100 thinks in reach). What is left is the draws landing
differently.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* The numbers are the first row's (Normal): 90, 85, 50, 10.
* The two draws are folded into per-action chances, as the comments in the file say.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
