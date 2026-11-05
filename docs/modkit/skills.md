# Skills from building blocks (`files/skills.toml`)

A mod can add a skill without writing Rust: the skill is a row of the `skills` table plus a list of **parts** in
`files/skills.toml` that say what it does when it fires. The parts are the blocks the engine's own skills are made of:
a missile, an aura pulse, a summon, a state (with the stack rules of [states](states.md)), a melee hit and an area hit.

A mod with no `skills.toml`, or an empty one, plays exactly like D2Gamma: the slot the block skills use is not
answered, so no engine skill and no frame hash changes.

## Why a parts file and a table row

The skill's **row** carries everything the game already knows how to do with a skill: the mana cost, the cast and
attack animations, the range, the level requirements, the description and the icon. It is added or changed with a
[table patch](tables.md), so the skill goes through the normal skill path: the client asks, the **server decides**
(mana, range, cooldown, animation), and the effect happens on the server, so it works online with no client code.

The parts file only says what the skill *does*. That keeps the table row editable in Mod Studio's table view and the
effect in one schema that is checked as a whole.

Hook the two together with one column. Set the row's `srvdofunc` to **400** (the block slot) and add an entry with the
same row id to `skills.toml`:

```toml
# a patch to the skills table: the row 390 gets srvdofunc = 400, plus mana, animation, level, ... as usual
```

```toml
version = 1                    # optional; the only version

[[skill]]
id = 390                       # the skills row (required, once per row)
name = "Ember Burst"           # a label for tools; the game ignores it

[[skill.part]]
kind = "area_hit"              # damage everything around the aim point
radius = 12                    # sub-tiles (five to a tile)
[skill.part.damage]
element = "fire"
min = "10 + lvl * 4"           # formulas, see below
max = "20 + lvl * 6"

[[skill.part]]
kind = "state"                 # then add a burn stack to them
op = "add_stack"
state = 380
on = "area"
radius = 12
```

A skill has 1 to 16 parts. They run in order, once per cast. The skill's do step answers 1 when at least one
part did something and 0 when none did (a melee hit with no target, a summon that could not be placed), which is
what the engine's own skills answer when they find nothing to do.

## Formulas

Every number that depends on the skill level is a **formula** in the engine's own skill formula language, the one the
skills table's `calc` columns use: `"10 + lvl * 4"`, `"min(ln12 * 2, 40)"`, `"par1 + lvl"`. A plain integer
works too. The names are the `skillcalc` names (`lvl`, `ln12`, `dm12`, `par1` to `par8`, ...). Formulas are compiled
when the mod loads, so a typo is an error that names the part and quotes the text, not a skill that silently does
nothing.

## The parts

| `kind` | What it does | Keys |
| --- | --- | --- |
| `missile` | Fires an existing `missiles` row from the caster toward the aim point, or with `at_aim` drops it on the aim point as Meteor does (the point must be within 100 of the caster and free of anything that blocks a missile of that row's size). The missile row says how it flies and what it hits. | `missile` (row), `lob` (default false), `at_aim` (default false), `channel` (default false: set the missile's length from the row's `calc1`, as a channel does), `[[skill.part.cc]]` (what its hits declare) |
| `aura` | Once, as an aura does each period: puts a state with stats on the caster and on every unit around the caster. | `state`, `target_state` (the state the units around get when it is another one; default the same), `range`, `seconds`, `affects` (default `allies`), `[[skill.part.stat]]` with `stat` and `value` (at most 6) |
| `curse` | A curse: puts a state with stats on the units around the aim point (not the caster) that pass the engine's curse tests: hostile, alive, awake, not a town unit. The monster-AI curses (Dim Vision, Terror, Taunt) also change the monster's AI in the engine's body; this part only puts the state on. | `state`, `range`, `seconds`, `affects` (default `enemies`), `[[skill.part.stat]]` with `stat` and `value` (at most 6) |
| `wall` | A wall of missiles across the skill's target (the target unit's place, else the aim point), as Fire Wall lays it: two missiles of the row run out along the wall and an optional second row sits at its middle. The place must be free of what blocks such a missile and not in town. | `missile` (row), `centre` (row, optional) |
| `nest` | Hatches a nest: spawns the monster that the skill's start function (the engine's Nest start, `srvstfunc` 49, which a lifted row keeps) chose from the caster's `monstats` spawn columns (`spawn`, `spawnx`, `spawny`, `spawnmode`), at the place it chose, with an optional overlay on it. Does nothing when the start function did not run. | `overlay` (row, optional) |
| `resurrect` | Raises the dead monster that is the skill's target, as the monsters' Resurrect does: the monster's own revive (full life, back among the living) and its rise animation mode request, with an optional overlay row played on it. The target must be a monster that is dead or dying and not hidden, with a free place under it; otherwise the cast does nothing. | `overlay` (row, optional) |
| `heal` | Gives the caster's unit target back a share of its maximum life, as the Zakarum heal does: the share is rolled between `min` and `max` percent (each held to 0 to 100, `min` held to `max`) on the caster's seed. The target never goes above its maximum life or to 0. Does nothing without a unit target. | `min`, `max` (formulas, percent) |
| `summon` | Makes pets of a `monstats` row; the oldest goes when the pet type's maximum is passed. | `class`, `pet_type`, `pet_max` (default 1), `count` (default 1, at most 8), `level` (default 0: follow the caster) |
| `state` | Applies, stacks, unstacks or ends a state. | `state`, `op`, `on`, `radius`, `affects`, `seconds`, `count` |
| `melee_hit` | A melee attack on the skill's target: the hit test, the weapon's damage and the skill's own damage on top. Needs a target; a miss or a block still counts as the attack. | `weapon_percent` (default 100), `attack_rating` (formula, added), `[skill.part.damage]`, `enhance` (percent added to a hit that lands, default 0), `own_element` (add the row's own elemental damage, default false), `life_leech`, `mana_leech` (formulas, default 0: taken from the target by a hit that lands, on top of the weapon's own leech), `sequence` (formula: hits in a row, the target then other enemies within melee reach, each once, at most eight; default 0 is one hit), `stun_frames` (default 0), `monster_mode` (`a1`, `a2` or `s1`: a monster caster takes the damage and to-hit of that attack mode first), `overlay` (an `overlay` row played on the target by a hit that lands), `target_state` with `target_frames` (formula, base frames, at least one) and up to six `[[skill.part.target_stat]]` (`stat`, `value`) (a state with those stats put on the target by a hit that lands), `result_flags`, `hit_flags`, `hit_class` (the skills table's `ResultFlags`, `HitFlags`, `HitClass` added to a hit that lands: how the target reacts), `[[skill.part.cc]]` |
| `area_hit` | Damage to every unit in a radius around the caster or the aim point. | `radius`, `around` (`target` default or `caster`), `affects` (default `enemies`), `[skill.part.damage]`, `[[skill.part.cc]]` |
| `missile_ring` | A ring of evenly spaced missiles around the caster or the aim point, as a nova. 64 is the engine's own nova. | `missile`, `count` (default 16, 1 to 64), `velocity`, `around` (`caster` default or `target`) |
| `missile_arc` | A fan of evenly spaced missiles centred on the way to the aim point. | `missile`, `count` (default 5, 1 to 64), `span` (default 16, in 64ths of a circle), `velocity` |
| `missile_volley` | Several missiles at the aim point, each with a small random spread drawn from the caster's seed (multi-shot). | `missile`, `count` (default 3, 1 to 64) |
| `chain` | Damage to the skill's target, then to the nearest other unit in reach of it, and on. Each unit is hit once. Needs a target. | `radius` (reach of a jump), `jumps` (0 to 16), `falloff` (percent lost per jump, default 0), `affects`, `[skill.part.damage]`, `[[skill.part.cc]]` |
| `bounce` | Like `chain`, but a unit may be hit again after another one was (never twice in a row). | the keys of `chain` |
| `teleport` | Moves the caster to the aim point; a farther point takes it only `max_range` toward it. | `max_range` (formula, default no limit) |
| `raise` | Raises the corpse the skill is aimed at (a dead monster that can be revived) as a pet of the caster. Needs such a target. | `pet_type`, `pet_max` (default 1) |
| `minion_order` | Orders the caster's living minions within `radius`: `recall` (come to the caster), `follow`, or `attack` (each casts `skill` at the skill's target). | `order`, `radius`, `skill` (for `attack`, required), `level` (default: this skill's level) |
| `engine_body` | Runs D2Gamma's own code for one skill function (`srvdofunc`) slot, so a data skill keeps that behaviour and adds parts before or after it. Counts as acting when the engine's body did something. | `slot` (1 to 190), `skill` (the skills row it reads its numbers from, default this skill), `level` (default this skill's level) |
| `repeat` | Runs another skill of this file's parts (or, with no `skill`, the parts that follow it in the same skill) again and again, as this skill at this skill's level: a channel, an aura zone, a charge that builds. See [Over time](#over-time). | `skill` (a skills row of this file; leave it out to repeat the parts after this one), `every` (base ticks between runs, default 25), `times` (default 5, 1 to 600), `now` (first run at once, default true), `while_state`, `stop_on_move` |
| `leap` | Moves the caster to the aim point in steps over a few ticks, then runs another skill's parts where it lands. See [Over time](#over-time). | `frames` (default 10), `max_range`, `land` (a skills row of this file) |
| `rush` | Runs the caster at its target (else the aim point) one step a tick, as a charge does, then runs parts where it stops. See [Over time](#over-time). | `speed` (percent of running speed, default 100), `steps` (default 25, at most 125), `stop_in_reach` (default true), `each`, `land` (skills rows of this file; with no `land` the parts after the rush run where it stops) |

A **damage** block is `element` (`physical` default, `fire`, `lightning`, `magic`, `cold`, `poison`), `min` and `max`
(formulas, whole points, the roll is between them), `seconds` (poison or cold duration) and `bonus_stat`: the number
of an `itemstatcost` stat read on the caster as a percent that raises this part's damage (`25` makes it 125%). That is
how "+Y% damage while charged" reaches a skill: a [state effect](states.md) grants the stat, the part reads it.

A damage block can also carry `charge = { state = 191, percent = 10, consume = false }`, a **charge-up**: each stack of
that state on the caster adds `percent` (a formula) to the part's damage, and `consume = true` ends the state on the
caster once the part has hit. Together with the `state` part's `add_stack` this makes "build up, then release" skills
out of blocks. A charge that builds while it is held is `repeat` of a `state` part (see below).

The `state` part's `op` is `apply` (default; a second application does what the state's `reapply` says),
`add_stack` (`count` more stacks up to `max_stacks`), `remove_stack` (`count` fewer; the state ends with the last) or
`remove`. `on` is `caster` (default), `target` or `area` (units within `radius` of the aim point that pass
`affects`). The duration is `seconds`, or the state's `duration_seconds` from `states.toml`; a state part that gives
neither is refused when the mod loads, because the state would never end.

`affects` is the engine's unit filter: `enemies` (hostile, attackable, not in town), `allies` (the caster's allies,
the caster included), `everyone` (attackable, not in town) or `allied_players` (allies that are players, the caster
included; pets and mercenaries are left out).

## Over time

A skill can keep acting after the cast frame. The host has a **tick seam**: a block skill asks for a tick on the
game's timer list (the unit timer of event 14 with a mod callback, so nothing of D2Gamma's own timers changes), and
when it is due the host hands it back to the skill, which does its step and asks for the next. Ticks belong to the
casting unit: they end when it dies or leaves the game, and a new cast of the same skill ends the ones before it.
A game with no mod never schedules one.

`repeat` runs another skill's parts on each tick. The skill it names is a `[[skill]]` of the same file with parts of
its own, and it has no `repeat` or `leap` itself (the mod is refused at load otherwise, as it is when the row is not
in the file). Its parts run **as the skill that repeats it**: the same skills row, level and kill credit.

With no `skill`, the parts **after** the `repeat` in the same skill are what repeats: they run on each tick (the first
at the cast, unless `now = false`) and not again at the cast. The mod is refused at load when nothing follows, or when
a `repeat` or `leap` does. This keeps a channel in one `[[skill]]`:

```toml
[[skill]]                       # a channel of flames: one every half second for three seconds
id = 393
[[skill.part]]
kind = "repeat"
every = 12
times = 6
[[skill.part]]
kind = "missile"                # runs on each repeat
missile = 14
```

```toml
[[skill]]                       # a channel: a pulse of fire a second, for as long as you stand still
id = 391
name = "Burning Ground"
[[skill.part]]
kind = "repeat"
skill = 392                     # what runs each time
every = 25                      # base ticks (25 to a second)
times = "4 + lvl"
stop_on_move = true             # leaving the spot ends it

[[skill]]
id = 392                        # not cast by anything itself
[[skill.part]]
kind = "area_hit"
radius = 10
around = "caster"
[skill.part.damage]
element = "fire"
min = "lvl * 3"
max = "lvl * 5"
```

* **Channel:** `repeat` of damage parts, `stop_on_move = true` and/or `while_state` (it ends when the caster no
  longer has that state: have a `state` part switch the state on in the same skill).
* **Periodic aura:** `repeat` of an `aura` part. Each tick pulses the aura around where the caster stands then, so the
  zone moves with it. (A skill row with the `aura` flag keeps pulsing on D2Gamma's own timer; this is for casts that
  start a timed aura.)
* **Charge-up over time:** `repeat` with `now = false` of a `state` part with `op = "add_stack"`; a damage block's
  `charge` reads the stacks when the release skill hits.

`leap` moves the caster toward the aim point in steps of 2 ticks, `frames` ticks in all (at most 50 steps; `max_range`
shortens it), and runs the `land` skill's parts, as this skill, where the caster ends. Each step places the caster
at a free spot on the line, so a wall does not stop the leap, and a step that finds no spot ends it there. There is
no flight animation: the client sees the caster move in steps.

`rush` runs the caster toward its target (the aim point when it has none) one step every tick, at most `steps`
steps. A step is a fifth of the caster's run velocity (the class's `RunVelocity`, the monster's `Run`) times `speed`
percent, at least one sub-tile; the caster is put on the line to the target's *current* place each step, never closer
than a sub-tile to it, so a wall does not stop the rush and a step that finds no free spot ends it. It stops where the
target is within melee reach (`stop_in_reach`, on by default; off runs on to where the target stood), where a step
finds no spot, or when the steps are spent; a target already in reach is hit at once with no run. The caster is in
the engine's skill-move state (it cannot be interrupted) while it runs. `each` is a skill whose parts run after every
step (an `area_hit` around the caster makes a whirling run); `land` is the skill whose parts run where the rush stops.
With no `land`, the parts that follow the `rush` in the same skill run there instead (as with `repeat`), and not at the
cast, so a charge is one `[[skill]]`: `rush` then `melee_hit`. A new cast ends the rush it started before. The rush
does not walk the game's path code, so the client sees the caster move in steps, with no run animation, and the unit's
own speed bonus is not read. It unlocks Charge (lifted from `srvdofunc` 67) and the skills of the same shape: a run
that ends in a hit.

**Hold position and guard** orders for minions are not blocks. They need a pet behaviour D2Gamma does not have (a
minion that stays put and still fights what comes near, and goes back to its post), and a small flag is not enough:
the pet AI would have to be taught what to do with it. `minion_order` stays `recall`, `follow` and `attack`.

## Base skills as data

The skills D2Gamma ships run as Rust. They stay Rust: a mod that changes nothing plays exactly like D2Gamma. To
change one, **lift** it: the lifter reads one row of *your own* `skills` table (D2Gamma carries no game table) and
writes a `skills.toml` entry you can copy and edit, plus the cells to change in the row (`srvdofunc = 400`, and for a
missile skill an empty `srvmissile`, because the core fires that missile after any do function).

| Family | The row | Becomes |
| --- | --- | --- |
| attack | `srvdofunc` 1 | a `melee_hit` part |
| missile | no `srvdofunc`, a `srvmissile` | a `missile` part |
| nova | `srvdofunc` 22 | a `missile_ring` of 64 with the engine's velocity (`vel + vellev * lvl / 8 + calc1`) |
| aura | `srvdofunc` 65 | an `aura` part with the row's `aurastate` for the caster, its `auratargetstate` for the units around (a row with none reaches only the caster, so the part gets range 0), range, length, filter and stats |
| curse | `srvdofunc` 30 | a `curse` part with the row's `auratargetstate`, range, length, filter and stats |
| melee_state | `srvdofunc` 2 with `srvstfunc` 6 (Power Strike) or 32 (Bash, Stun, Concentrate) | a `melee_hit` as the Jab's but for a player (no `monster_mode`), with `srvoverlay` as `overlay`, `auratargetstate` as `target_state` (`auralencalc` frames, `aurastat1`..`6` as `target_stat`) and, for start 32, `resultflags`, `hitflags` and `hitclass`; the row drops its start function (`srvstfunc` 0). Left out, with the reason: other start functions, a row with its own `aurastate` (the engine's lasts to the unit's next mode change), and a start 32 row with `calc2` (flat damage the engine adds after the enhanced damage). `calc3`'s temporary attack rate is not applied (noted) |
| sequence | `srvdofunc` 13 with `srvstfunc` 9 (Fend) or 37 (Zeal) | a `melee_hit` with `calc2` as `enhance` and `calc1` as `sequence`; the row drops its start function (`srvstfunc` 0). Other start functions are left out with the reason. The engine swings once per animation pass and rewinds it for the next enemy; the data hit makes every swing in the one cast (noted). Not compared in the closeness report: the fixture has no animation records for the rewind |
| hunger | `srvdofunc` 122 | a `melee_hit` with `calc1` as `enhance`, `calc2` as `life_leech` and `calc3` as `mana_leech`. The engine does nothing on a miss; the data hit counts a miss as an attack (noted) |
| jab | `srvdofunc` 7 | a `melee_hit` with `srcdam` as the weapon percent, `calc1` as `enhance`, the row's own element, the row's to-hit as `attack_rating` and `monster_mode = "s1"` |
| smite | `srvdofunc` 150 | a `melee_hit` with `calc2` as `stun_frames` and `monster_mode = "a2"` (a player's Smite uses the shield's damage in the engine; the data hit uses the weapon's) |
| wall | `srvdofunc` 24 | a `wall` part with `srvmissilea` and `srvmissileb` as the middle row |
| meteor | `srvdofunc` 28 | a `missile` part with `srvmissilea` and `at_aim = true` |
| heal | `srvdofunc` 96 | a `heal` part with the row's `calc1` and `calc2` as the two percents |
| nest | `srvdofunc` 91 | a `nest` part with `sumoverlay` as the overlay row; the row keeps the engine's start function 49, which picks the monster and the place |
| inferno | `srvdofunc` 95 | a `repeat` (no `skill`) of a `missile` part with `srvmissilea` and `channel = true`: `every` is `calc3` and `times` is the channel's length (`calc2`, or `calc1` for the Imp's start function) divided by `calc3`, rounded up, plus one (the engine fires a flame before it looks at the length, so the one that finds it used up is fired too) |
| charge | `srvdofunc` 67 | a `rush` (`speed` is `param1` + 50, `steps` 40) then a `melee_hit` as the Jab's (attack mode A1 for a monster); the row drops the engine's start function (`srvstfunc` 31), which sets the unit running on its path |
| resurrect | `srvdofunc` 97 | a `resurrect` part with `srvoverlay` as the overlay row |

These four are first because most classes and monsters use them; `lift::rank` orders the do functions of your tables
by the number of classes and monsters that use them, so the next ones to lift are the next on that list. A lift is
*close to* the engine's body, not the same: each lifted entry carries notes naming what differs (an aura counts whole
seconds, a data attack does not shoot a bow). The lifter refuses a row it does not know, a name the tables do not
have, and any entry the reader would not accept, with the reason.

**Closeness report.** The test `skill_host::closeness` runs each family twice on the same seeded game through the real
do core, once with the Rust body and once with its lifted copy, and compares damage, hit count, missile count and
timing (frames to the first hit, until the missiles are gone, until an aura's state ends). Set
`D2_CLOSENESS_REPORT=<file>` to get the markdown table; a row marked `!` differs by more than 25%. It is a report,
not a gate: a lifted skill is allowed to differ, and the report says where.

The same comparison runs on an install's own rows: the ignored test `closeness::real` reads the skills tables from the
archives in `D2_GAME_DIR`, lifts every row of a lifted family from that row's numbers and formulas, and runs both
ways on the same fixture (the row's missile and state stand for the fixture's; the numbers the lift reads are the
row's own). Set `D2_CLOSENESS_REAL_REPORT=<file>` for the markdown, which holds row numbers and counts only. A row whose
formula the data formulas cannot say (a call, a stat or a damage read such as `edmn`) is counted as left out with the
reason, as is a row the fixture cannot run. The formulas that read only the level and the Param columns (`lvl`,
`par1` to `par8`, `ln12` to `ln78`, `dm12` to `dm78`) are said as themselves: a data skill has the same Param columns
and the engine's evaluator.

The fixture's caster is a bare-handed player who deals 5 to 9 damage, and a swing's damage is counted on the frame of
the cast, so the melee families (`attack`, `jab`) really lose life in both runs and are compared on damage and hits.
The monster branches (`monster_mode`) do nothing for a player caster, and the engine's Smite needs a shield or a
monster caster, so Smite is reported as "not compared" until the fixture has one.

The same report repeats its tables for **the skills some monster has** (a `monstats` row names the skills row in one of
its eight skill columns), so the monsters' skills are counted separately, and ends with the do functions of those
skills that are not lifted yet, ordered by how many monster rows use them. That list is the order in which the next
families are worth lifting. On the install it was do function 30 (Curse; 5 skills, 66 monster rows), then 28 (10 skills,
40 rows), 97, 95, 67, 96 and so on. A curse is not an aura around the caster: it is a state with stats on the units around
the aim point, with the curse tests, so it has a `curse` part of its own and is lifted now. Meteor (28) is a missile
that appears on the aim point instead of flying from the caster, so it is a `missile` part with `at_aim`, and the
Zakarum heal (96) is a `heal` part; both are lifted now, and the list continues with the next do function (97, 95, 67
and so on).

**Skills that are not lifted** still expose their numbers and events to a mod:

* the numbers are the row's own cells (damage, mana, ranges, formulas), changed with a table patch;
* `engine_body` runs the Rust body of one slot inside a data skill, with a level of your choosing, so parts can be
  added **before** or **after** it: a mod can add a nova ring to an engine skill's cast without lifting it;
* the events are the trigger events: `skill_used` with `skills = [N]` is **on-cast**, and `hit` / `kill` with
  `by_skill = N` are **on-hit** and **on-kill** for that skill (see [Events](#events)).

## Rules the host keeps

* **Server-decided.** The parts run on the server in the skill's do step. Nothing the client sends picks a target,
  a number or a roll.
* **The engine's damage.** Melee and area parts go through the engine's hit test (melee), resistances, kill credit
  and damage events, so a part's kill is credited like an engine skill's. An area hit plays no get-hit reaction unless it declares crowd control (see [crowd-control.md](crowd-control.md)).
* **Bounded.** At most 16 parts, 64 units per area, 64 missiles per ring, arc or volley, 16 jumps per chain, 8 pets per summon, 6 stats per aura; formulas have no loops.
* **Deterministic.** Random rolls draw only from the casting unit's seed, so a replay gives the same game.
* **Aura states keep the aura rules.** An `aura` part refreshes its state like an aura does. Use a `state` part for
  stack rules.

## Errors

A wrong file is refused whole when the mod loads, with the entry, the key and the reason:

```
invalid skills.toml setting: `skill[0].part[1]`: formula `lvl *`: ...
```

Unknown keys are errors too (`deny_unknown_fields`), so `radius_` or `elment` is caught and not ignored.

# Triggers (`files/triggers.toml`)

A trigger says "when this happens to a unit, maybe do this". It has a **source** (where it lives), one **event**, a
**chance**, **conditions** and **actions**. A mod with no `triggers.toml` plays exactly like D2Gamma: the engine only
tells the mod about an event when the mod has triggers.

```toml
version = 1

[[trigger]]
name = "Reaction window"
on = "attack"                  # the event, see below
while_skill = 391              # the source: while the unit has this skill (its level is `lvl`)
chance = "4 + lvl"             # percent, a formula; default 100

[[trigger.condition]]          # all of them must hold
kind = "lacks_state"
state = 381

[[trigger.action]]             # run in order
kind = "apply_state"
state = 381
seconds = 1
```

## Source

Exactly one of:

| Key | The trigger is there while... | `lvl` in the formulas |
| --- | --- | --- |
| `while_skill` | the unit has that skill | the skill's level |
| `while_stat` | the unit's stat (an `itemstatcost` row) is above 0. An **item property** gives the stat, so a trigger on an item is `while_stat` plus the property that gives the stat. | the stat's value, so `chance = "lvl"` makes the property's number the chance |
| `while_state` | the unit is in that state | its stack count |

## Events

The unit is the one the event happens to; the **other** unit is the one hit, the attacker or the victim.

| `on` | Happens when |
| --- | --- |
| `attack` | the unit swings at something in melee (hit or miss) |
| `hit` | the unit deals damage (melee or missile) |
| `being_hit` | the unit takes damage (melee or missile) |
| `skill_used` | the unit used a skill that did something. `skills = [391, 392]` limits it to those skills. A skill a trigger casts does **not** count |
| `kill` | the unit kills something |
| `state_start` | a state with rules in `states.toml` (`state = N`) turns on for the unit. Another stack is not another start |
| `state_end` | that state runs out or is cleared by the game. A state the **mod** removes (a trigger action, a skill part) does not end: the mod asked for it |

`attack`, `hit` and `kill` triggers can add `by_skill = N`: they run only while the unit is using skill `N`. A hit counts
when it lands during the cast (a melee swing, an area, chain or `engine_body` part); a missile's later impact is not
the cast any more. With `skill_used` and `skills = [N]` this gives a skill **on-cast**, **on-hit** and **on-kill**
events, for a data skill and for an engine skill that has not been lifted.

`state_start` and `state_end` work for states that have an entry in `states.toml`; the host refuses a trigger that
names another state.

## Conditions

`kind` is `has_state`, `lacks_state`, `stacks_at_least` (`state`, `count`), `life_below` or `life_above` (`percent`).
Each looks at the unit holding the trigger, or with `who = "other"` at the other unit of the event (a condition on a
missing other unit does not hold).

## Actions

| `kind` | Does | Keys |
| --- | --- | --- |
| `apply_state` | puts the state on (its `reapply` says what a second application does) | `state`, `seconds`, `who` |
| `add_stack` | adds `count` stacks (the state comes on first when off) | `state`, `count` (formula, default 1), `seconds`, `who` |
| `remove_stack` | takes `count` stacks off; the state ends with the last | `state`, `count`, `who` |
| `remove_state` | ends the state | `state`, `who` |
| `cast_skill` | casts a skills row as the unit, at the other unit (`at = "other"`, default) or at itself (`at = "self"`) | `skill`, `level` (formula, default `lvl`), `at` |
| `crowd_control` | hits the unit with a hit that does no damage and causes the effect: `effect = { kind = "stun", seconds = 0.5 }` (`interrupt`, `knockback`, `stun` or `freeze`; see [crowd-control.md](crowd-control.md)) | `effect`, `who` (default `other`) |

`who` is `self` (default) or `other`. `seconds` is a formula; leave it out when the state's `duration_seconds` in
`states.toml` gives the time (the host refuses an action that would never end). A cast skill needs no mana, no
ownership and no `itemeffect`; it can be a block skill of this mod.

## Limits and order

* At most 64 triggers, 8 conditions and 8 actions each. Triggers run in file order, after the game's own handlers
  of the event.
* Triggers whose actions set off more triggers stop 3 levels down, so a pair of triggers cannot loop forever.
* The chance is rolled from the holder's own random seed, only when the trigger gets that far (the conditions held),
  so a replay gives the same game. A chance of 100 or more rolls nothing.
* Everything runs on the server, in the same tick as the event.

# Worked example: a proc, a window and charges

Everything above in one mod. The folder `templates/proc-charges` is a complete, game-file-free template of this
example, and the tests play it end to end (so it is a mod that is known to work). Copy it, change the ids and the
numbers.

> 8% chance on a melee attack: a one-second reaction state opens. Using skill X, Y or Z while it is open gives +1
> charge (at most 5) and closes the window. A window that runs out unused costs a charge. With 3 or more charges,
> skill X does +25% damage. The HUD shows the charges.

Three small files carry it:

* `files/states.toml`: state 190 is the window (`duration_seconds = 1`); state 191 is the charges (`max_stacks = 5`,
  `reapply = "add_stack"`, a duration for the charges as a group, an `icon`). Its one `[[state.effect]]` gives stat
  340 with `per_stack = 25`, `min_stacks = 3` and `flat = true`: nothing under 3 charges, +25 from 3 on, however many
  more there are.
* `files/skills.toml`: skill 390 is an `area_hit` whose damage has `bonus_stat = 340`, so it reads the stat as a
  percent on the caster. Skills 391 and 392 are other parts (a nova around the caster, a melee hit); any block
  skill, or any engine skill, can be in the list.
* `files/triggers.toml`, three triggers on the passive skill 393 (or an item property, with `while_stat`):
  1. `on = "attack"`, `chance = 8`, condition `lacks_state` 190, action `apply_state` 190: the proc.
  2. `on = "skill_used"`, `skills = [390, 391, 392]`, condition `has_state` 190, actions `add_stack` 191 and
     `remove_state` 190: the charge, and the window is used up.
  3. `on = "state_end"`, `state = 190`, condition `has_state` 191, action `remove_stack` 191: the missed window.
     It works because removing the window from trigger 2 is the mod's own doing and is **not** a state end.

The game still needs the table rows (a table patch): the skills rows with `srvdofunc = 400`, the two state rows and
the stat row. The template's README lists them.

Things that are good to know:

* The cast that earns the charge is already done, so it does not get the charge's bonus; the next one does.
* The HUD needs nothing extra: a state with an `icon` shows in the state icon row with its stack count.
* **The server judges the window.** The window opens on the server and a skill counts when the server sees it used
  inside the window, so a player with a high ping can miss a window a local player would hit. Making this fair for
  latency (judging by when the player pressed the key) is a later milestone (M2) and is not built yet.

## Timed class buffs

`timed_buff` applies a self buff through the defensive-buff state path, including exclusive state groups, removal callbacks and animation refresh. Its `frames` formula counts base frames (25 per second), preserving sub-second durations. `stat` accepts up to eleven `{ stat, value }` entries for aura/passive stats. `event` accepts up to three `{ event, callback }` pairs, using events-table IDs and the filled combat callback slots 1–31. These callbacks retain their existing engine meanings.

The do-18 lifter reads these values from the author's tables and writes editable parts; Studio provides stat and event controls. No Rust body runs on behalf of the part. The normal game continues using its existing bodies. Authoring resolves table names in linked formulas to numeric row references. Unknown names and unsupported bytecode remain explicit needs. The data part dispatches state/event cleanup on timer expiry, whereas the existing table-buff timer path leaves that callback undispatched; the report flags observed expiry differences. “same” measures only the fixture's observed stats, state coverage and expiry, alongside the existing metrics.

## Druid spirits, wolves and bears

`druid_summon` selects the monster `class`, `pet_type`, initial `mode`, `pet_max` formula and monster `level` formula. It uses the existing summon/ownership seam, joins the owner's target list and applies pet base and skill stats. It does not add a follow path: that is a separate part. The do-119 lifter resolves the author's monster, pet-type and mode names and preserves the cap and level expressions. Studio edits these fields directly.

The closeness fixture measures owned pet count, their total levels and a setup stat over three casts, including eviction when the cap is exceeded. It uses a nonzero skill row so passive and granted-skill setup can run, with generated monster, animation and pet-type fixtures standing for the install's rows. This checks setup and ownership; it does not compare the summoned monsters' AI attacks or their art. Other summon families retain their own setup rules and are lifted separately.

## Warcries

`warcry` sends a ring of 64 missiles and applies an optional timed `state` to its caster. Its `frames` and up to six `stat` formulas are also used when those missiles hit other units. The selected missile row retains its party/enemy filtering and damage rules; choose a shout-state hit rule for an editable buff/debuff. With no data warcry for the skill, missile hits keep their normal table effect.

The do-68 lifter resolves the missile, optional state, duration and stat formulas. Studio edits these fields. The report compares ring count, state coverage, stat values and expiry; a separate wired test compares table fallback with edited effects on player and monster targets. Progression-dependent missile selection and full party combat scenarios remain outside the install fixture.

## Placed sentries

`sentry` places a monster at the aim point, checks its room and `in_town` permission, and sets its `class`, `pet_type`, initial `mode` and `pet_max` formula. It resolves a monster caster's player owner, uses normal pet cap eviction, applies pet base/skill stats and alignment, then makes a real mode request. The sentry's monster AI owns shooting, shot limits and death; this part supplies placement and setup.

The do-45 lifter retains these settings. Studio edits them directly. Generated fixtures measure owned pets, levels, setup stats and initial modes across repeated casts; separate tests check edited caps, exact placement, out-of-room refusal and a real town's permission. They do not compare trap AI damage or art. With no data definition the native skill body remains active.

## Forms and vines

A `form` part supplies a restrictive state, a `frames` formula, a toggle `delay` formula and up to six `stat` entries. Configure the state's exclusive group and restrictive flag and the skills' allowed form states in the mod tables: ordinary skill-use gating reads those values. Casting again clears the current exclusive form and starts the toggle delay. The opt-in removal callback clears the form and restores the skill entry mode when its timer expires. D2Gamma's default shape callback dispatch is unchanged; the generated closeness row reports that native expiry gap explicitly.

A `vine` part supplies `class`, `pet_type`, `state`, `pet_max` and `level` formulas. It summons once in mode 8, joins the owner's target list, applies its movement state and level-only setup, then the existing pet skill stats. The lifter exports the vine movement state explicitly, so modders may edit it. The chosen monster AI still controls attacks and healing; this part does not replace that AI.

## Damage auras

A `damage_aura` part supplies a caster `state`, optional `target_state`, `period`, `range`, fixed-point mana `cost`, raw aura `filter`, damage flags and separate `own_stat` (up to five) and `stat` (up to six) lists. A pulse applies caster passive stats, skips outgoing effects in town, rolls the skill row's elemental damage once, then applies the shared roll and target stats to accepted nearby units. The ordinary table scheduler still drives automatic aura pulses. The part does not spend mana: its threshold and regeneration-state behavior follow the damaging-aura path.

`freeze = true` additionally selects Holy Freeze's callback, difficulty-specific monster cold-effect gate and target-seed shatter roll. Stat durations extend to the next aligned period. Damage curves and mastery remain the mod skill row's; editing that metadata changes the roll without changing this part. D2Gamma's native aura bodies remain the default.

## Learned-skill passives

The `passive` part runs when a learned skill refreshes. Keep its cast slot unchanged; it is not a cast action. Set a positive `state`, an item-type `layer` (zero for unlayered bonuses), optional `blocked_by` aura state, and up to five `stat` formulas. A refresh writes the actual layered state list and its skill/level markers. Repeating the same level does not accumulate the bonuses; zero level or an active blocking aura detaches the list. Skills without a data passive use the existing table refresh.

Studio exposes the state, layer, blocking state and formulas. The lifter preserves `passivestate`, `passiveitype`, `passivestat1..5` and their calculations without replacing the do function. The closeness fixture refreshes a nonzero learned entry and measures all five layered values and both markers; no passive casts are fabricated. The normal learning/list path still owns passive state flags.

## Class golems and ravens

The `golem` and `raven` parts expose the summon monster, pet type, initial mode, pet-cap formula and level formula. They compose the existing summon and pet setup primitives. Golems apply owner summon resistance, announce an ally and join owner targets; nonpositive level selects the engine's owner scaling. Ravens clear their target flag, zero regeneration and clamp their explicit level to at least one.

The lifter handles golem do 56 and raven do 114. These parts keep pet AI and attacks on the selected monster. Tests use generated animation and monster records and observe actual owned counts, levels, skill stats, initial modes, resistances, raven flags and regeneration. No game art or tables are included.

## Finding items and potions in corpses

The `corpse_find` part consumes a selectable corpse before rolling its `chance`. `potion=true` uses the normal act/difficulty potion selector with editable third/fourth category bands. Item finding uses all four explicit quality-band formulas and the corpse's treasure class. Reusing the consumed corpse refuses; failed potion and item chances retain their different native return values.

The generated economy fixture measures corpse consumption and actual placed item classes, with tests for guaranteed success and failed chance. Current treasure-drop wiring accepts a quality hint but does not apply it; the part preserves that shared behavior. Item rolls and potion selection stay on the existing economy paths. Rust skills still use the unchanged table-driven wrapper.

### Shadow companions

The opt-in `shadow` part exposes the companion class, pet list and cap, initial mode, life percentage, equipment item level, lifetime, optional state/modifier and up to eleven stat formulas. The SK2 lifter preserves the table shadow rule that all aura stats use the second aura formula and all passive stats use the second passive formula. Formulas run on the companion for its stat setup. Equipment uses generated monster equipment through the existing economy path; source linking, owner targeting, AI and lifetime timers use the shared host paths.

The generated closeness fixture observes real equipped item units and item levels, pet levels and stats, source links and timers. A separate native/data host comparison seeds nonzero life and checks scaling and the level-one skip; the generated summon has no monster life table. These are bounded setup comparisons. Companion AI and equipment choices from an install still require a mod-game check. Rust bodies remain the default.

### Martial charge-ups

The opt-in `martial_charge` part performs its hit through the shared queued melee combat path and adds a charge only when that actual queued record has landed. State, counter, duration, attack rating, source scale, dual-claw frame selection and one incremental stat bonus are explicit. A charge list retains the native three-charge cap, skill/level markers and expiry callback. The part does not duplicate generic weapon damage.

SK2 copies the six table charge-ups to this part. The bounded fixture compares melee damage and actual charge lists; direct lifecycle tests compare counter caps, cumulative bonuses and expiry. Native finishing and progressive release still read the skill's table metadata; this module exposes charge building, not a new progressive-release language. Modders changing the state/counter must keep corresponding progressive metadata consistent until release recipes are data. Finisher variants and equipped dual-claw fidelity remain listed as needs; no native body wrapper is emitted.

### AI curses

The opt-in `ai_curse` part makes Confuse and Attract explicit. `state`, `range`, `frames`, `filter` and `attract` control the effect; Confuse accepts up to five stat formulas corresponding to table slots two through six. Attract has no stat list. The shared monster eligibility checks, difficulty divisor, alignment changes, AI nodes, target overrides and ordinary expiry callbacks still run. Aim scans are bounded to 64 candidates. Rules without this part keep their native body.

Generated fixtures store real monster AI data and observe actual forced-target kinds, alignment, nodes, stat lists and callback expiry. These comparisons cover the control setup and expiry; inspect combat behavior in a mod game as well.

### Projectile fans

The opt-in `projectile_fan` part exposes spread volleys (Multiple Shot, Teeth and Shock Wave) and sequential jitter fans (Charged Bolt, Twister and Tornado). Missile rows, an optional non-bow missile, count, central ordinary-hit shot count, activation delay, jitter mode and the empty-count return policy are data. Counts are bounded to 64 and central shots to the total. Existing missile creation and initialization handle damage, movement and collision. Native bodies remain the default; progressive missile selection still needs explicit charge metadata and is not inferred by this part.

The closeness fixture counts actual emitted missiles and observes their hits and timing. Extreme authored counts are bounded, and changing a count changes emitted units. Compare different hands and full map flight paths in a mod game as well.

### Guided projectiles

The opt-in `guided_projectile` part exposes the missile and optional non-bow missile plus the damage-percentage initializer. It retains targeted and untargeted flight flags, target attachment and the native signed packed origin offset. Missile creation, damage initialization and flight use existing paths. Rules without the part retain the native body. The closeness row observes actual emitted units and their stored homing mode and offset; check flight and target acquisition across rooms in a mod game.

### Sight pulses

The opt-in `sight` part exposes Inner Sight and Slow Missiles as a caster-centered timed single-stat pulse. State, stat, value, range, duration and filter are explicit. The scan skips no-aura monsters and the ordinary timed state path applies and removes values; this does not add curse or AI scaling. Candidate scans are bounded to 64. Rules without the part retain the native body. Generated comparisons inspect actual target stat lists and expiry and prove no-aura exclusion. Check visibility and missile movement in a mod game after editing the respective stat.
