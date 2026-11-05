# The block vocabulary

Skills, triggers and quests (and later monster AI) are made of named building blocks. The engine keeps **one list**
of every block a mod file may use, with a description and the modding API version that introduced it. Mod Studio
shows that list (the "Block reference" under the Skills, Triggers and Quests pages, and a tooltip on every block in the
"add" menus), and `check-mod` validates the names a mod uses against it.

The list lives in the engine, in `crates/d2-sim/src/rules/data_mod/blocks/`, one file per family.

## Families

| Family | Names | Written as | File |
| --- | --- | --- | --- |
| Skill parts | the steps of a skill | `kind = "area_hit"` in a skill `part` | `skill_parts.rs` |
| Events | what a trigger reacts to | `on = "attack"` | `events.rs` |
| Conditions | tests a trigger needs to hold | `kind = "has_state"` in a `condition` | `conditions.rs` |
| Actions | what a trigger does | `kind = "apply_state"` in an `action` | `actions.rs` |
| Targets | whom or where a block works | `affects = "enemies"`, `who = "other"`, `around = "caster"`, `on = "area"` | `targets.rs` |
| Values | numbers a formula can read | `lvl`, `par1`, `ln12` | `values.rs` |
| Quest objectives | what a quest asks of the player | `kind = "kill"` in a quest `objective` (see [quests](quests.md)) | `objectives.rs` |
| Quest rewards | what a finished quest gives | `kind = "stat"` in a quest `reward` | `rewards.rs` |

Each entry has the **name** as written in the file, a **label** and a plain-words **description** for tools, the
API version it came in (**since**), and its **keys**: for each key the kind of value it holds, whether it is
required, what it defaults to and what it means. Targets also name the **slot** (the key) they are written under.

## Names are versioned

A block name is part of the modding API and keeps its meaning for as long as the API version that introduced it is
supported (12 months after the next API version is released; see `d2_modkit::api`).

* A block is never renamed in place. A better block gets a new name and the old one is marked **deprecated** in the
  list, with the name to use instead. It keeps working for the support window.
* `check-mod` tells a mod made for API N that it uses a block that only exists from a later API, and asks it to
  raise `api` in `mod.toml`.
* `check-mod` warns about deprecated blocks and names the replacement.
* An unknown name is an error with the nearest known name ("did you mean `area_hit`?") and the list of the family.

## What check-mod reports

The "building-block names and rules files" stage runs after the global rules parse:

| Category | Severity | Meaning |
| --- | --- | --- |
| unknown or too-new blocks | error | a name the vocabulary does not have, or a block newer than the mod's `api` |
| deprecated blocks | warning | a block that works but is going away |
| invalid building-block files | error | the files fail the engine's readers as a whole (a wrong value, a state with no `states.toml` entry, a state that would never end) |

The location is the file and the place in it: `skills.toml: skill[0].part[1].kind`.

## Adding a block

Adding a block is a small change in one place per family:

1. Add the variant and its running code where the family is run (a variant of `Part`, `Condition`, `Action` or
   `On`, and its arm in the family's `run`).
2. Add **one row** to the family's table in `blocks/` (name, label, since, description, keys).
3. Run the tests of `blocks/`. They check that the table lists exactly the names the readers accept, that the
   keys of a row are exactly the keys the reader accepts for that block, that every required key is required, and
   that a sample file built from each row reads back. A missing or misspelled key in the row fails the build.
4. Give the block its own behaviour test where the family is tested (`wiring/interaction/tests/skill_host/`).

A new **family** (for example monster AI conditions) adds its own table file, a `Family` variant, and a row in
`PLACES` (in `blocks/mod.rs`) for each key of its mod file that carries a name.

`since` is always the API version that is current when the block is added; a block can never claim an API
that does not exist yet (the tests check it against `d2_modkit::CURRENT_API`).
