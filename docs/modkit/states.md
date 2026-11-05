# States and stacks (`files/states.toml`)

A state is a buff or a debuff on a unit. The `states` table says how a state looks and which groups it belongs to
(flags, colour, overlay); the skill that applies it says for how long and with which stats. `states.toml` is the mod
kit's way to give a state its own rules: how long it lasts, how many **stacks** it can have, what a second
application does, how the stacks run out, which icon the HUD shows and what each stack gives.

A mod with no `states.toml`, or an empty one, plays exactly like D2Gamma: the apply path is not touched for a state the
file does not mention.

## Why a side file and not table patches

The `states` table is a table of flags. Its row has no duration, stat list or stack column, and the states table is
not where a modder expects to find "+5% damage per stack". The file is also read as a whole by the host (like
`rules.toml`), which lets it check the entries against each other (duplicate ids, a `lose_one` decay without a time)
and gives Mod Studio one schema to edit. The state still **needs its row in the `states` table** (add it with a
[table patch](tables.md)); the file only adds rules to a row.

## The file

```toml
version = 1                 # optional; the only version

[[state]]
id = 380                    # the row in the states table (required)
name = "Smolder"            # a label for tools; the game ignores it
duration_seconds = 8        # how long a stack lasts; leave out to use the applying skill's duration
max_stacks = 5              # 1..=255, default 1
reapply = "add_stack"       # what applying it again does: "refresh" (default), "add_stack" or "ignore"
decay = "lose_one"          # when the time runs out: "lose_all" (default) or "lose_one"
icon = "smolder"            # the HUD icon, see below
overlay = 17                # an existing overlay row shown while the state is on

[[state.effect]]            # what ONE stack gives; the state gives this times the stack count
stat = 25                   # the row in itemstatcost
per_stack = 5               # e.g. +5 per stack: 3 stacks give +15
```

| Key | Default | Meaning |
| --- | --- | --- |
| `id` | required | The `states` row. Two entries for one id are refused. |
| `name` | empty | Shown by tools and messages. |
| `duration_seconds` | the skill's | 1..=86400. Counted at D2Gamma's 25 ticks a second and converted to the game's tick rate. |
| `max_stacks` | 1 | 1..=255. |
| `reapply` | `refresh` | `refresh`: the clock starts over, the count stays. `add_stack`: one more stack up to `max_stacks` (the clock starts over, also at the maximum). `ignore`: nothing changes, the running clock is kept. |
| `decay` | `lose_all` | `lose_all`: the state ends when the time runs out. `lose_one`: one stack is lost and the time starts over, until none is left. Needs `duration_seconds`. |
| `icon` | none | The name of the icon the client draws; see below. |
| `overlay` | none | An `overlay` row (existing effects only; the effects editor is later). |
| `effect.stat`, `effect.per_stack` | none | A stat of the `itemstatcost` table and its value for one stack. Negative values are allowed. |
| `effect.min_stacks` | 1 | The effect counts only from this many stacks on (1 to `max_stacks`); below it the state gives 0 of that stat. |
| `effect.flat` | false | Give `per_stack` once while `min_stacks` is met, whatever the count above it, instead of `per_stack` times the count. "From 3 charges on, +25%" is `min_stacks = 3`, `flat = true`. |

The first application always puts the state on with one stack. Any skill that applies the state stacks it: the
applying skill and its level do not matter. Values are integers; a very large `per_stack` times the count saturates.

Row ids are numbers for now: the state row and the `itemstatcost` stat are the row numbers of the effective tables
(base rows plus the mod's patches). Mod Studio fills them in from names; `check-mod` (K9) will check them against
the tables. A state id that is not in the tables is never applied.

## What the game does

* **Where it applies:** every skill or effect that applies a state through the engine's standard state apply
  (`apply_state` in the skill bodies and the combat events) stacks it by the rules above. The
  building-block skill parts (K7) will use the same call. Places that build a state list by hand (an aura's
  periodic refresh, a Rust mod's own `SkillHost::apply_state`) keep their own rules.
* **Where the stack count lives:** the simulation keeps the count on the unit's state list, so it is part of the
  game state, follows the unit and is gone when the state is.
* **Stat effects:** the list always holds `per_stack × count` (or `per_stack` once with `flat`, and 0 under `min_stacks`); the unit's totals follow.
* **Stacks do not persist through save and quit** (as no state does today). Owner question pending.
* **State ids:** states past id 254 work online through the wide state message. The id limits come from the table
  checks of [tables](tables.md).

## What clients see

The server decides and sends the count: a state with rules is followed by a wide state message, op 3 (`Stacks`), with
the state id and a 16-bit count, whenever the state is set or its count changes (`d2_proto::wide_state`). It exists
on a mod game's wire only, so D2Gamma's own protocol and its fuzz targets' inputs for it are unchanged. The client
keeps the count on the unit and the HUD draws one icon per state with a definition and an `icon`, with the count
in its corner when it is above 1.

### Icons

`icon = "smolder"` names a PNG at `files/icons/smolder.png` in the mod (up to 32×32; transparent pixels allowed).
Without the file, the HUD draws a coloured square with the first letter of the name, so a mod works before it has art.
