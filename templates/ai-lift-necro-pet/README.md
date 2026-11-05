# AI 67 (NecroPet), as data

The melee form of AI 67 (NecroPet), the Necromancer's skeleton and clay warriors, as a one-state graph: stay with the owner, swing in reach, lunge at an enemy, wander.

Lifted from the Rust body in `crates/d2-sim/src/monsters/ai` (AI3). The graph is the body's logic written in the graph vocabulary, with the numbers from the monstats `aip` columns; the comments in `files/ai.toml` say which line is which.

## Try it

```text
D2Gamma-Game ai-closeness --classes 351 --ai 67 --seeds 8 --ticks 400 \
    --mod-dir templates/ai-lift-necro-pet --native <your Diablo II install>
```

runs the test arena twice per class and seed, once on the Rust body and once under this graph, and prints the means side by
side (see `docs/modkit/ai.md`). Open the folder in Mod Studio's Monster AI tab to see and edit the graph. Install it as a mod
(`--mod-dir`) to play with it: every monster of the AI row `67` then runs the graph instead of the Rust body.

## How close it is

**Not measured with `ai-closeness`.** The arena has one character and one monster, and nothing in it gives the monster an owner,
so the Rust body and the graph both run their "no owner" branch (idle) and the tables would show 400 ticks of standing on
both sides. The graph is checked by unit tests of its pieces instead (`follow_owner` reaches the same pet-follow the body
calls, with the same numbers; the owner conditions and aims read the same owner and the same owner view).

The report is a report: it shows where the vocabulary does not reach, and nothing fails on it.

## What the graph does not do

* **The ranged form** (the skeleton mage, and any pet the raising skill marks as ranged) is picked by the first AI param of the
  unit; a graph cannot read it, so only the melee form is lifted. A mod that wants a ranged pet can write its own graph with
  `follow_owner at = "second"` and `use_skill`.
* The body strays 8 sub-tiles instead of 7 on 15 percent of its thinks (a draw it makes before it follows); the graph uses 7.
* The body's own target search is capped at 24 and falls back on the owner's target within 36; the graph acts on its target.
* A pet that has lost its owner (the owner left the game) walks to where it last saw the owner by the guid it keeps in an AI param,
  and idles when it has none; the graph idles.
* The numbers have no `aip` columns: the pet has none.

Every graph holds one difficulty's numbers (Normal; the monstats `aip` columns differ in Nightmare and Hell), and a graph
reads the monster's own row only where the vocabulary says so.
