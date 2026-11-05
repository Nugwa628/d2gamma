# AI 32 (Npc), as data

The idle part of AI 32 (Npc): one state that idles 8 frames. **Not for play**: see the limits.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 147,148,150 --ai 32 --seeds 6 --ticks 200 \
    --mod-dir templates/ai-lift-npc --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `32` then runs the graph instead of the Rust body.

## How close it is

Classes 147 (Gheed), 148 (Akara) and 150 (Kashya) (AI 32), 6 seeds x 200 ticks, means:

| | Rust | data |
|---|---|---|
| ticks standing | 200.0 | 200.0 |
| closest / farthest (sub-tiles) | 25.2 / 25.2 | 25.2 / 25.2 |
| hit points either side lost | 0 | 0 |

No number differs, **because the arena has nobody to talk to**. The comparison cannot see what the graph lacks.

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

The Npc body is mostly things a graph cannot say, so this is a lift of its last step only (idle 8 frames when nothing
else is due), and a mod that sets it on a town NPC loses the rest:

* the route over the map's NPC nodes and the turn to face a player who comes near;
* the interaction state (the monster stops while a window is open);
* the quest hooks (Cain's, Drehya's and the other portals) and the commands other units give it.

None of these is in the graph vocabulary. Use this folder to see what an NPC's idle looks like as data, not in a game that
has to run its towns.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
