# Monster AI graphs (`ai.toml`)

> A monster that stalks, telegraphs a leap, calls its pack when hurt and runs when nearly dead, without a line of code.

A mod gives a monster a **behaviour graph**: a few named *states*, and in each state the *transitions* that leave it
and the *actions* the monster takes on its turn. The graph runs on the server, at the game's tick rate, and uses the
same movement, skill and target helpers as D2Gamma's own AIs. A monster without a graph keeps the AI it has today,
bit for bit.

A worked, game-file-free template is `templates/monster-ai-graph`.

You do not have to write the file by hand: **Mod Studio has a Monster AI tab**. It draws the graph as boxes and arrows
(a box per state, an arrow per transition, an orange box per boss phase), and every part is chosen from a list: the
kind of condition and action, the state a transition goes to, the cooldown an action starts, the monster class a
summon creates. A preset entry shows the AI's settings by name, with a box to turn each on and a "per difficulty"
switch. Studio asks the game's own reader whether the file is good, shows what it objects to with a *Show* button that
selects the state or phase, and will not save a file the game would refuse. Studio writes the file back in its own
layout (comments and key order of a hand-written file are not kept). To start from one of D2Gamma's AIs, use *Add from
D2Gamma's AI*; to start from your own, *Duplicate*.

## 1. Which monsters

A graph answers one value of the `AI` column of `monstats`:

```toml
version = 1

[[ai]]
index = 200            # every monster whose monstats row says AI = 200
name = "Stalker"       # shown by tools; the game ignores it
target = "enemy"       # "enemy" (default), "any" or "none"
start = "stalk"        # default: the first state
```

* An `index` past D2Gamma's table (200 in the examples) is a new AI.
* An `index` equal to one of D2Gamma's AIs **replaces that AI for the mod**: every monster that uses it runs the graph.
* `target = "enemy"`: the graph thinks only when the monster has seen an enemy. `"any"`: it also thinks with no
  target (to wander, guard or call for help). `"none"`: it always thinks and never has a target.

## 2. Cooldowns

```toml
[[ai.cooldown]]
name = "leap"
seconds = 6
```

An action that names `starts_cooldown = "leap"` starts it when it acts; `{ kind = "cooldown_ready", name = "leap" }`
holds when it has run out (or was never started). A graph has at most 8.

## 3. States, transitions, actions

```toml
[[ai.state]]
name = "stalk"

[[ai.state.transition]]                 # checked in order, the first whose `when` all hold wins
to = "strike"
when = [{ kind = "distance_below", value = 3 }]

[[ai.state.action]]                     # tried in order, the first that acts ends the turn
kind = "use_skill"
slot = 2                                # the monster's Skill2 column
when = [{ kind = "cooldown_ready", name = "leap" }]
starts_cooldown = "leap"
```

On each think the graph follows transitions (at most 4 per think, so a ring of states cannot spin), then walks the
new state's actions. An action whose `when` does not hold, or that cannot be done now (no target, no such skill), is
passed over. If no action acts the monster idles for its normal think delay.

### Conditions (`when`, all must hold; at most 6)

| `kind` | Keys | Holds when |
| --- | --- | --- |
| `has_target`, `no_target` | | there is, or is not, a target |
| `distance_below`, `distance_above` | `value` (subtiles) | the target is closer than, or at least, `value` |
| `in_melee` | | the target is within melee reach (this is also the "combat range" the built-in AIs test; there is no separate condition) |
| `out_of_melee` | | the target is not within melee reach, or there is none: the opposite of `in_melee` |
| `has_owner`, `no_owner` | | the monster is, or is not, somebody's minion (a pet) |
| `owner_distance_below`, `owner_distance_above` | `value` (subtiles) | the owner is closer than, or at least, `value` away; false without an owner |
| `owner_target` | | the owner has an enemy in sight: what the owner's own target search finds (the Necromancer's pets read it) |
| `second_target` | | the monster sees a second target: the nearest enemy, which many built-in AIs act on instead of the main target |
| `second_distance_below`, `second_distance_above` | `value` (subtiles) | the second target is closer than, or at least, `value` |
| `undead_within` | `radius`, `at_least` (1) | undead monsters of the monster's side (not itself) are within `radius`: the scan the Greater Mummy makes |
| `corpse_within` | `radius` | an undead of its side lies fallen (and can be raised) within `radius` |
| `hurt_within` | `radius` | an undead of its side is hurt within `radius` |
| `class_count_below`, `class_count_at_least` | `class`, `also`, `count` | fewer than, or at least, `count` living monsters of the family (monstats `BaseId`) of `class`, or of `also`, are in the rooms near the monster: the Minion Spawner's cap |
| `thinks_at_least` | `count` | the monster has thought `count` times since it entered the state (this think included); the Sand Raider counts thinks to power up |
| `line_of_sight`, `no_line_of_sight` | | the monster can, or cannot, see the target |
| `life_below`, `life_above` | `percent` | life is below, or at least, that share of the maximum |
| `has_state`, `lacks_state` | `state`, `who` | the state is on, or off |
| `stacks_at_least` | `state`, `count`, `who` | the state has at least `count` stacks |
| `allies_within` | `radius`, `at_least` (1) | living pack mates (leader and its minions) are within `radius` |
| `enemies_within` | `radius`, `at_least` (1) | players or their allies are within `radius` |
| `cooldown_ready` | `name` | the cooldown has run out |
| `in_state_for` | `seconds` | the monster has been in this state that long |
| `alive_for` | `seconds` | the monster has had this graph that long (since it spawned, or since a phase switched to it) |
| `random` | `percent` | a roll on the monster's own seed comes up under `percent` |

`who` is `"self"` (default), `"target"`, `"second"` (the second target), `"corpse"` or `"hurt"` (the unit the undead scan found),
`"owner"` or `"owner_target"` (the monster's owner, and the enemy the owner has in sight).

### Actions (at most 8 per state)

| `kind` | Keys | Does |
| --- | --- | --- |
| `move_to` | `run`, `steps`, `speed`, `at` | walks, or runs, toward the target; `steps` stops that many steps short of it (default 1, all the way), `speed` walks faster (or slower, below 0) by that many percent (-100 to 100) |
| `hop_to` | `steps` (12), `keep` (0), `speed`, `at` | one hop of at most `steps` subtiles toward the target, keeping `keep` subtiles from it (it backs off when nearer), then the monster thinks again: how the built-in AIs close in. A `move_to` goes all the way before it thinks |
| `follow_owner` | `steps` (7), `quiet` (false), `at` | stays with the owner as the built-in pets do: keeps up (walking, running, or catching up when left far behind), makes room for the owner's other pets and loiters near it. `steps` is how far it may stray; `at` is the enemy it has in mind (`"target"`, `"second"` or `"owner_target"`). Does nothing, and lets the actions after it go on, when it is near the owner with an enemy to deal with, and, with `quiet`, when it is already where it should be; also when it has no owner |
| `move_away` | `run`, `steps` | moves `steps` subtiles straight away from the target |
| `flee` | `steps` | runs away, far |
| `move_around` | `steps` | circles the target |
| `attack` | `attack` (1 or 2), `at` | a melee swing |
| `use_skill` | `slot` (1 to 8), `at` (`"target"`, `"second"`, `"self"`) | uses the skill in that `monstats` column |
| `wander` | `radius` | walks to a random spot near by |
| `idle` | `seconds` | stands still |
| `wind_up` | `seconds`, `event` | stands still and reports `event`; holds the actions after it until the time is over |
| `call_allies` | `radius` | wakes pack mates within `radius`. **Instant** |
| `summon` | `class`, `count` (1 to 4), `spread` | creates minions. **Instant** |
| `set_state` | `state`, `on` | turns a state on or off on the monster. **Instant** |

`at` on `move_to`, `hop_to`, `attack` and `use_skill` is `"target"` (default), `"second"` (the second target), `"corpse"`
or `"hurt"` (the fallen or hurt undead of the undead scan, within `radius`, default 24), `"owner"` or `"owner_target"`;
`use_skill` also takes `"self"`. With no such unit the action is passed over.

*Instant* actions happen and the walk through the list goes on; any other action that acts is the monster's turn.

### Wind-ups and telegraphs

`wind_up` then `use_skill` is the pattern: the monster stands still for `seconds`, the graph reports a `WindUp` event
(the hook a telegraph effect or sound hangs on) and the strike that follows it comes when the time is over. A state
change cancels a running wind-up.

## 4. Boss phases

A graph can have *phases*: the monster changes behaviour, once, when a threshold or a timer is reached.

```toml
[[ai.phase]]
name = "enrage"
when = [{ kind = "life_below", percent = 50 }]   # same conditions as everywhere
state = "rage"                                   # go to a state of this graph...
immune_seconds = 3                               # ...and take no crowd control for 3 seconds

[[ai.phase]]
name = "rout"
when = [{ kind = "life_below", percent = 15 }, { kind = "alive_for", seconds = 60 }]
graph = 202                                      # ...or hand over to another graph of the file
```

* Phases start **in order, each once**. Only the next phase is looked at, so "rout" waits for "enrage" even when the
  monster is already at 10% life; a think that finds several due starts them one after the other (at most 4).
* Every phase reports a `PhaseChanged` event (and a `StateChanged` one when it moves the monster), the hooks for a
  roar, a screen shake or a message.
* A phase with `state` moves the monster there at once; with `graph` the monster takes the other graph's first state
  and its first turn a moment later (its phases count from zero). The monster keeps the target mode of the graph it
  started with. A phase with neither only reports the event (and gives the window).
* `immune_seconds` needs `immune_state = <state id>` in `crowd_control.toml`: that state, which the mod defines in
  `states.toml`, makes its holder immune to stun, freeze, interrupt and knockback, declared or not. The phase puts
  it on the monster for the time; a mod that asks for a window without naming the state is refused when it loads.
  The monster must be an attackable class (`isatt`), like any monster a state is put on.

## 5. Bounds and cost

At most 32 graphs per file, 24 states and 8 phases per graph, 8 transitions and 8 actions per state, 6 conditions per
transition, phase or action, 8 cooldowns and 8 wind-up event names per graph. One think looks at no more than **272** conditions and
actions, and a test runs the largest graph the limits allow against that number. Every random draw comes from the
monster's own seed, so replays and the 60 Hz frame hashes stay deterministic.

## 6. Mistakes

Errors name the file and the key: `ai[0].state[1].action[2].slot: must be 1 to 8`. A key an action does not read
(`steps` on an `attack`) is refused instead of ignored.

## 7. Presets: D2Gamma's own AIs by name

Writing a whole graph is not the only way. A mod can pick one of D2Gamma's 148 AIs by name and change its
parameters (the `aip1` to `aip8` columns of `monstats`), without touching the monstats row:

```toml
[[ai]]
index = 201                    # the monstats AI value of the monsters that should run it
name = "Angry zombie"
preset = "Zombie"

[ai.params]
approach = 70                  # one number: every difficulty
aware_dist = [8, 12, 16]       # or Normal, Nightmare, Hell
aip4 = 20                      # the raw column name works too
```

* The monsters run the preset's own body, exactly as D2Gamma plays it, with your numbers where you set one and the
  `monstats` columns everywhere else. With no `[ai.params]` the entry is just that AI.
* `index` equal to the preset's own row (`index = 3`, `preset = "Zombie"`) changes the knobs of every Zombie in the
  mod.
* Parameter names come from the descriptions in D2Gamma's AI table, so they are as good as those descriptions;
  question marks mean a guess. A parameter with no description is only set as `aipN`.
* A preset entry has no states, cooldowns, `start` or `target` (the preset decides); an unknown preset or parameter
  is refused with the list of valid ones.

### Copy a preset, then edit it

Start with a preset entry. When a knob is not enough, replace `preset` and `params` with `[[ai.state]]` tables:
the entry is then a graph, and everything in sections 1 to 6 applies.
The preset's own body is fixed code, so a graph is a new behaviour of the same shape, not a transcript of the
preset.

### The presets

| AI | Preset | Parameters (`[ai.params]` names) |
| ---: | --- | --- |
| 0 | `None` | none described (use `aip1` to `aip8`) |
| 1 | `Idle` | none described (use `aip1` to `aip8`) |
| 2 | `Skeleton` | `approach` (aip1), `stall_time` (aip2), `attack` (aip3), `att1_att2` (aip4) |
| 3 | `Zombie` | `approach` (aip1), `aware_dist` (aip2), `att1_att2` (aip4) |
| 4 | `Bighead` | `hurt_percent` (aip1), `circle` (aip2), `fire_while_healthy` (aip3), `fire_while_hurt` (aip4) |
| 5 | `BloodHawk` | `charge` (aip1), `wander` (aip2), `attack` (aip3), `run_vel` (aip4), `charge_vel` (aip5) |
| 6 | `Fallen` | `cmd_attack` (aip1), `attack` (aip3), `att1_att2` (aip4) |
| 7 | `Brute` | `circle` (aip2), `attack` (aip3), `att1_att2` (aip4) |
| 8 | `SandRaider` | `hurt_percent` (aip1), `circle` (aip2), `attack` (aip3), `approach` (aip4), `charge_time` (aip5), `charge_color` (aip6), `att1_att2` (aip7) |
| 9 | `Wraith` | `approach` (aip1), `stall_time` (aip2), `attack` (aip3) |
| 10 | `CorruptRogue` | `approach` (aip1), `stall_time` (aip2), `attack` (aip3), `run_vel` (aip4), `run` (aip5) |
| 11 | `Baboon` | `hurt_percent` (aip1), `circle` (aip2), `attack` (aip3), `att1_att2` (aip4), `regen_bonus` (aip5) |
| 12 | `Goatman` | `approach` (aip1), `stall_time` (aip2), `attack` (aip3) |
| 13 | `FallenShaman` | `rez_cmd` (aip1), `shoot` (aip2), `melee_circle` (aip3), `rez_dist` (aip4), `shoot_dist` (aip5) |
| 14 | `QuillRat` | `activate_dist` (aip1), `shoot` (aip2), `walk_dist` (aip4) |
| 15 | `SandMaggot` | `lay` (aip1), `spit` (aip2), `of_eggs` (aip3), `melee` (aip4), `min_down_up_time` (aip5) |
| 16 | `ClawViper` | `charge` (aip1), `charge_dist` (aip2), `attack` (aip3), `att1_att2` (aip4), `stall_time` (aip5), `charge_color` (aip6) |
| 17 | `SandLeaper` | `leap` (aip1), `attack` (aip2), `approach` (aip3), `circle` (aip4) |
| 18 | `PantherWoman` | `approach` (aip1), `attack` (aip2), `pack_dist` (aip3), `stall_time` (aip4), `regroup_from_melee` (aip5) |
| 19 | `Swarm` | `approach` (aip1), `stall_time` (aip2), `attack` (aip3) |
| 20 | `Scarab` | `attack` (aip1), `att1_att2` (aip2), `stall_time` (aip3), `jab` (aip4), `cmd` (aip5) |
| 21 | `Mummy` | `awakedist` (aip1), `approach` (aip2), `attack` (aip3), `att1_att2` (aip4), `stall_time` (aip5) |
| 22 | `GreaterMummy` | `melee_breathe` (aip1), `raise` (aip2), `heal` (aip3), `shoot` (aip4), `raise_range` (aip5) |
| 23 | `Vulture` | `attack` (aip1), `stall_time` (aip2), `wounded_percent` (aip3), `circle` (aip4), `move` (aip5) |
| 24 | `Mosquito` | `min_suck_loop` (aip1), `suck_loop_rng` (aip2), `attack` (aip3), `suck_or_att` (aip4), `max_ambient_loop` (aip5) |
| 25 | `WillOWisp` | `cast` (aip1), `melee` (aip2), `approach` (aip3) |
| 26 | `Arach` | `attack` (aip1), `circle_in_melee` (aip2), `engage` (aip3), `run_dist` (aip4), `hurt_percent` (aip5) |
| 27 | `ThornHulk` | `attack` (aip1), `att1_att2` (aip2), `melee_circle` (aip3), `frenzy` (aip4), `frenzy_speed` (aip5), `consec_a2` (aip6) |
| 28 | `Vampire` | `melee` (aip1), `cast` (aip2), `active_dist` (aip3), `upgrade_cast` (aip4), `spell_flags` (aip5) |
| 29 | `BatDemon` | `hurt_percent` (aip1), `disengage_on_hit` (aip2), `attack` (aip3), `att1_ltng` (aip4), `regen_roosted` (aip5) |
| 30 | `Fetish` | `attack` (aip1), `stall_time` (aip2), `attack_loop` (aip3), `weak_percent` (aip4) |
| 31 | `NpcOutOfTown` | none described (use `aip1` to `aip8`) |
| 32 | `Npc` | none described (use `aip1` to `aip8`) |
| 33 | `HellMeteor` | `attack` (aip1), `stall_time` (aip2), `range` (aip3) |
| 34 | `Andariel` | `spray_in_melee` (aip1), `stall` (aip2), `fire_or_engage` (aip3), `spray_from_dist` (aip4) |
| 35 | `CorruptArcher` | `approach` (aip1), `shoot` (aip2), `stall_time` (aip3), `run` (aip4), `always_run_dist` (aip5), `use_skill_2` (aip6), `use_skill_3` (aip7), `walk_tow_dist` (aip8) |
| 36 | `CorruptLancer` | `approach` (aip1), `attack` (aip2), `stall_time` (aip3), `run` (aip4), `always_run_dist` (aip5) |
| 37 | `SkeletonBow` | `shoot` (aip1), `stall_time` (aip2), `approach` (aip3), `walk_steps` (aip4), `tgt_dist` (aip5) |
| 38 | `MaggotLarva` | `attack` (aip1), `att_recovery` (aip2), `approach` (aip3), `idle_stall_time` (aip4), `circle` (aip5) |
| 39 | `PinHead` | `attack` (aip1), `att_stall_time` (aip2), `approach` (aip3), `idle_stall_time` (aip4), `smite` (aip5) |
| 40 | `MaggotEgg` | `stall` (aip1), `hatch` (aip2) |
| 41 | `Towner` | none described (use `aip1` to `aip8`) |
| 42 | `Vendor` | none described (use `aip1` to `aip8`) |
| 43 | `FoulCrowNest` | `spawn_interval` (aip1), `num_to_spawn` (aip3) |
| 44 | `Duriel` | `holy_freeze_level` (aip1), `smite` (aip2), `jab` (aip3), `att2` (aip4), `charge` (aip5) |
| 45 | `Sarcophagus` | `spawn_interval` (aip1), `num_to_spawn` (aip3) |
| 46 | `ElementalBeast` | `approach` (aip1), `activation_dist` (aip2), `idle_stall` (aip3) |
| 47 | `FlyingScimitar` | `approach` (aip1), `attack` (aip2), `stall_time` (aip3), `circle` (aip4) |
| 48 | `ZakarumZealot` | `attack` (aip1), `att2` (aip2), `hurt_percent` (aip3), `run` (aip4) |
| 49 | `ZakarumPriest` | `attack` (aip1), `blizzard` (aip2), `ltng` (aip3), `cast` (aip4), `spell_timer` (aip5), `heal_range` (aip6) |
| 50 | `Mephisto` | none described (use `aip1` to `aip8`) |
| 51 | `Diablo` | none described (use `aip1` to `aip8`) |
| 52 | `FrogDemon` | `attack_melee` (aip1), `shoot_melee` (aip2), `circle_melee` (aip3), `circle_ranged` (aip4), `shoot` (aip5), `shoot_dist` (aip6), `stall_time` (aip7), `emerge_dist` (aip8) |
| 53 | `Summoner` | `cast` (aip1), `weaken` (aip2), `pref_element` (aip3), `nova_timer` (aip4), `firewall_timer` (aip5), `walk_away` (aip6), `nova_dist` (aip7), `missile_dist` (aip8) |
| 54 | `NpcStationary` | none described (use `aip1` to `aip8`) |
| 55 | `Izual` | `attack` (aip1), `engage` (aip2), `nova_at_range` (aip3), `nova_in_melee` (aip4), `post_nova_doldrums` (aip5), `num_swings` (aip6) |
| 56 | `Tentacle` | `attack` (aip1), `submerge` (aip2), `submerge_secs` (aip3), `emerge_secs` (aip4), `stall_time` (aip5), `active_dist` (aip6) |
| 57 | `TentacleHead` | `shoot` (aip1), `submerge` (aip2), `submerge_secs` (aip3), `emerge_secs` (aip4), `stall_time` (aip5), `active_dist` (aip6) |
| 58 | `Navi` | none described (use `aip1` to `aip8`) |
| 59 | `BloodRaven` | none described (use `aip1` to `aip8`) |
| 60 | `GoodNpcRanged` | none described (use `aip1` to `aip8`) |
| 61 | `Hireable` | none described (use `aip1` to `aip8`) |
| 62 | `TownRogue` | none described (use `aip1` to `aip8`) |
| 63 | `GargoyleTrap` | `shoot_dist` (aip1), `shoot` (aip2), `fire_recovery` (aip3), `stall_time` (aip4) |
| 64 | `SkeletonMage` | `shoot` (aip1), `approach_dist` (aip2), `approach` (aip3), `too_close_dist` (aip4), `walk_away` (aip5), `fire_dist` (aip6), `circle` (aip7), `stall_time` (aip8) |
| 65 | `FetishShaman` | `heal` (aip1), `heal_capability` (aip2), `heal_range` (aip3), `circle` (aip4), `heal_search_range` (aip5) |
| 66 | `SandMaggotQueen` | `max_spawn` (aip1), `delay` (aip2) |
| 67 | `NecroPet` | none described (use `aip1` to `aip8`) |
| 68 | `VileMother` | `max_spawn` (aip1), `max_at_once` (aip2), `spawn` (aip3), `attack` (aip4), `approach` (aip5), `circle` (aip6), `stall_time` (aip7) |
| 69 | `VileDog` | `attack` (aip1), `melee_stall` (aip2), `approach` (aip3) |
| 70 | `FingerMage` | `attack_circle` (aip1), `cast` (aip2), `healthy_percent` (aip3), `hurt_percent` (aip4), `cast_range` (aip5), `max_run_tries` (aip6), `out_of_range` (aip7), `melee_stall` (aip8) |
| 71 | `Regurgitator` | `attack` (aip1), `eat_in_melee` (aip2), `approach` (aip3), `look_for_food` (aip4), `look_in_melee` (aip5), `smell_dist` (aip6) |
| 72 | `DoomKnight` | `attack` (aip1), `melee_stall` (aip2), `approach` (aip3), `regular_stall` (aip4) |
| 73 | `AbyssKnight` | `bone_armor_hp_percent` (aip1), `bonearmor` (aip2), `attack` (aip3), `melee_stall` (aip4), `no_fire_dist` (aip5), `fire_recovery` (aip6), `approach` (aip7), `active_dist` (aip8) |
| 74 | `OblivionKnight` | `flee_range` (aip1), `engage_range` (aip2), `curse_timer` (aip3), `curse` (aip4), `shoot` (aip5), `bonespirit` (aip6), `approach` (aip7), `approach_dist` (aip8) |
| 75 | `QuillMother` | `attack` (aip1), `approach` (aip2), `melee_stall` (aip3), `regular_stall` (aip4) |
| 76 | `EvilHole` | `num_to_spawn` (aip1), `spawn_delay` (aip2) |
| 77 | `Trap-Missile` | `distance` (aip1), `num_to_shoot` (aip2), `delay` (aip3) |
| 78 | `Trap-RightArrow` | `min_dist` (aip1), `max_dist` (aip2), `delay` (aip3), `delay2` (aip4) |
| 79 | `Trap-LeftArrow` | `min_dist` (aip1), `max_dist` (aip2), `delay` (aip3), `delay2` (aip4) |
| 80 | `Trap-Poison` | `range` (aip1), `num_times` (aip2), `delay` (aip3) |
| 81 | `JarJar` | none described (use `aip1` to `aip8`) |
| 82 | `InvisoSpawner` | `num_to_spawn` (aip1), `activation_dist` (aip2), `delay` (aip3) |
| 83 | `MosquitoNest` | `num_to_spawn` (aip1), `activation_dist` (aip2), `delay` (aip3) |
| 84 | `BoneWall` | none described (use `aip1` to `aip8`) |
| 85 | `HighPriest` | `engage` (aip1), `heal_at_range` (aip2), `heal_hydra_timer` (aip3), `hydra_at_range` (aip4), `ltng_at_range` (aip5), `disengage` (aip6), `ltng_engaged` (aip7), `range` (aip8) |
| 86 | `Hydra` | none described (use `aip1` to `aip8`) |
| 87 | `Trap-Melee` | none described (use `aip1` to `aip8`) |
| 88 | `7TIllusion` | none described (use `aip1` to `aip8`) |
| 89 | `Megademon` | `inferno_ranged` (aip1), `inferno_melee` (aip2), `swing_melee` (aip3), `approach` (aip4), `circle_melee` (aip5), `inferno_timer` (aip6) |
| 90 | `Griswold` | none described (use `aip1` to `aip8`) |
| 91 | `DarkWanderer` | none described (use `aip1` to `aip8`) |
| 92 | `Trap-Nova` | `range` (aip1), `num_times` (aip2), `delay` (aip3) |
| 93 | `ArcaneTower` | `num_sk1` (aip1), `sk1_stall` (aip2), `sk1_long_delay` (aip3), `num_sk2` (aip4), `sk2_stall` (aip5), `sk2_long_delay` (aip6) |
| 94 | `DesertTurret` | `short_delay` (aip1), `num_shots` (aip2), `long_delay` (aip3), `range` (aip4), `spread` (aip5) |
| 95 | `PantherJavelin` | `approach` (aip1), `throw` (aip2), `group_dist` (aip3), `walk_away` (aip4), `stall_time` (aip5), `throw_dist` (aip6) |
| 96 | `FetishBlowgun` | `stay_in_range` (aip1), `run_away` (aip2) |
| 97 | `Spirit` | none described (use `aip1` to `aip8`) |
| 98 | `Smith` | none described (use `aip1` to `aip8`) |
| 99 | `TrappedSoul` | none described (use `aip1` to `aip8`) |
| 100 | `Buffy` | none described (use `aip1` to `aip8`) |
| 101 | `AssassinSentry` | none described (use `aip1` to `aip8`) |
| 102 | `BladeCreeper` | none described (use `aip1` to `aip8`) |
| 103 | `InvisoPet` | none described (use `aip1` to `aip8`) |
| 104 | `DeathSentry` | none described (use `aip1` to `aip8`) |
| 105 | `ShadowWarrior` | `max_target_dist` (aip1), `max_boss_dist` (aip2), `attack_chance` (aip3), `skill_decrement` (aip4), `summoning_skill_min_to_use_skill_max_to_use_skill` (aip8) |
| 106 | `ShadowMaster` | `approach_dist_melee_bonus_progressive_bonus` (aip1), `random_pick_ignore_range_boss_leash` (aip2), `attack_chance` (aip3), `summoning_skill` (aip8) |
| 107 | `Raven` | none described (use `aip1` to `aip8`) |
| 108 | `DruidWolf` | none described (use `aip1` to `aip8`) |
| 109 | `Totem` | none described (use `aip1` to `aip8`) |
| 110 | `Vines` | none described (use `aip1` to `aip8`) |
| 111 | `CycleOfLife` | none described (use `aip1` to `aip8`) |
| 112 | `DruidBear` | none described (use `aip1` to `aip8`) |
| 113 | `SiegeTower` | `stall_time` (aip1) |
| 114 | `ReanimatedHorde` | `attack` (aip1), `melee_stall` (aip2), `charge_range` (aip3), `charge` (aip4), `follow` (aip5), `walk_forward` (aip6), `ranged_stall` (aip7), `reanimate` (aip8) |
| 115 | `SiegeBeast` | `call_imp_dist` (aip1), `attack` (aip2), `stamp_in_melee` (aip3), `melee_stall` (aip4), `stomp_ranged` (aip5), `charge` (aip6), `charge_vel` (aip7) |
| 116 | `Minion` | `attack` (aip1), `melee_stall` (aip2), `approach` (aip3), `ranged_stall` (aip4), `att1_att2` (aip5) |
| 117 | `SuicideMinion` | `blow_up_delay` (aip1), `stall_time` (aip2), `approach` (aip3) |
| 118 | `Succubus` | `attack` (aip1), `approach` (aip2), `curse` (aip3), `curse_range` (aip4), `melee_stall` (aip5), `ranged_stall` (aip6), `curse_level` (aip7) |
| 119 | `SuccubusWitch` | `attack` (aip1), `approach` (aip2), `walk_away` (aip3), `comfort_dist` (aip4), `shoot` (aip5), `stall` (aip6), `amp_dam_tgt_hp_percent` (aip7), `weaken_my_hp_percent` (aip8) |
| 120 | `Overseer` | `rally_spell_timer` (aip1), `heal` (aip2), `whip` (aip3), `comfort_dist` (aip4), `comfort_zone` (aip5), `attack` (aip6), `att1_att2` (aip7) |
| 121 | `MinionSpawner` | `num_to_spawn` (aip1), `stall_time` (aip2), `spawn_delay` (aip3), `activation_range` (aip4), `max_in_area` (aip5) |
| 122 | `Imp` | `refer_to_source` (aip1) |
| 123 | `Catapult` | `attack` (aip1) |
| 124 | `FrozenHorror` | `attack` (aip1), `approach` (aip2), `arctic_blast` (aip3), `stall_time` (aip4) |
| 125 | `BloodLord` | `attack` (aip1), `approach` (aip2), `frenzy_att1` (aip3), `stall_time` (aip4) |
| 126 | `CatapultSpotter` | `attack` (aip1), `delay` (aip2), `range` (aip3), `random_range` (aip4), `shots_per_skill` (aip5) |
| 127 | `NpcBarb` | none described (use `aip1` to `aip8`) |
| 128 | `Nihlathak` | none described (use `aip1` to `aip8`) |
| 129 | `GenericSpawner` | none described (use `aip1` to `aip8`) |
| 130 | `DeathMauler` | `attack` (aip1), `approach` (aip2), `shoot_range` (aip3), `shoot` (aip4) |
| 131 | `Wussie` | none described (use `aip1` to `aip8`) |
| 132 | `AncientStatue` | none described (use `aip1` to `aip8`) |
| 133 | `Ancient` | none described (use `aip1` to `aip8`) |
| 134 | `BaalThrone` | none described (use `aip1` to `aip8`) |
| 135 | `BaalCrab` | none described (use `aip1` to `aip8`) |
| 136 | `BaalTaunt` | none described (use `aip1` to `aip8`) |
| 137 | `PutridDefiler` | none described (use `aip1` to `aip8`) |
| 138 | `BaalToStairs` | none described (use `aip1` to `aip8`) |
| 139 | `BaalTentacle` | none described (use `aip1` to `aip8`) |
| 140 | `BaalCrabClone` | none described (use `aip1` to `aip8`) |
| 141 | `BaalMinion` | none described (use `aip1` to `aip8`) |
| 142 | `ClawViperEx` | `charge` (aip1), `charge_dist` (aip2), `attack` (aip3), `shoot` (aip4), `stall_time` (aip5), `charge_color` (aip6), `shoot_dist` (aip7), `shoot_timer` (aip8) |
| 143 | `ShadowMasterNoInit` | `approach_dist_melee_bonus_progressive_bonus` (aip1), `random_pick_ignore_range` (aip2), `attack_chance` (aip3), `summoning_skill` (aip8) |
| 144 | `UberIzual` | `attack` (aip1), `engage` (aip2), `nova_at_range` (aip3), `nova_in_melee` (aip4), `post_nova_doldrums` (aip5), `num_swings` (aip6) |
| 145 | `UberBaal` | none described (use `aip1` to `aip8`) |
| 146 | `UberMephisto` | none described (use `aip1` to `aip8`) |
| 147 | `UberDiablo` | none described (use `aip1` to `aip8`) |

## 8. Trying a graph in the test arena

`d2-client ai-arena` runs one monster against one test character, headless, and records what its AI did:

```
d2-client ai-arena --class 19 --ai 6 --ticks 400 --mod-dir MyMod --native NATIVE_DIR --out trace.txt
```

* `--class N` is the monstats row to spawn; `--ai N` is the graph (the `index` in `ai.toml`) it should run, used to
  put the state, phase and wind-up names in the trace. With `--mod` / `--mod-dir` the game plays the mod's rules (a mod
  needs the converted native folder: `--native DIR` or the default one); without them the monster runs D2Gamma's own
  AI, which is the baseline to compare a graph with.
* The character is new, with `--seed` (the same seed gives the same bytes), `--difficulty`, and stands still where it
  arrives. It is warped to `--level` first (default 2, the Blood Moor; `0` stays in the first town, where monsters do
  not fight). The monster is spawned `--distance` sub-tiles east of it (a tile is 5; default 24), then `--ticks` ticks
  (default 500) are recorded. The level's own monsters are in the game too and may come; the record is of the one that
  was spawned.
* `--hurt TICK:PERCENT,...` hurts the monster on a schedule: from each tick on its life is set to that percent of its
  maximum (once each, in tick order; the tick counts from the spawn). `--hurt 100:50,300:20` has it at half life from
  tick 100 and at a fifth from tick 300, so hurt states, retreats and boss phase changes can be reached and measured
  (`ai-closeness` takes it too, and applies the same schedule to the body and to the graph).
* The file is `ai-trace-1`, text, one line per tick (`s`: the graph state, the monster's mode, the distance to the
  character, both hit points) and one per event (`e`: a state change, a wind-up, a phase). `d2_sim::rules::data_mod::ai::trace`
  reads and writes it, and sums it up: ticks in each state, the number of transitions, wind-ups and phases, the closest
  and farthest distance, the damage each side took, the tick the monster died. The command prints that summary.

The arena is the server's game: the stand-in character neither fights back nor moves, and the headless client may
stop on art it cannot draw without ending the run.

**From Studio.** The Monster AI tab has a *Test arena* panel under the graph: it saves `ai.toml`, runs this command
and draws the result (the summary, a strip of the states over the run with the wind-ups and phases marked, the events)
and the ticks each state ran for on the node graph. It needs the game executable and the converted native install set
in Studio's settings.

**In the game.** Type `aiview` in the dev console (`D2RUST_DEV`, on by default in a playtest build) to see, bottom
left, the monsters nearest to you that run a graph: the graph, the state and for how many ticks, the last transition
and how long ago, the phase, and a wind-up in progress. Type it again to turn it off; while it is off the game reads
nothing and draws nothing extra.

## What a mod cannot do yet

A graph can read a monster's owner (`has_owner`, `follow_owner`) but not the commands other monsters give it, so the AIs
that take orders (the fetishes, the fallen) and town NPCs cannot be written as graphs yet. The arena runs a graph; it
does not judge it (section 9 does, and section 10 lists what cannot be lifted).

## 9. How close a data copy is to the Rust AI

A mod that lifts one of D2Gamma's own AIs into `ai.toml` (the `templates/ai-lift-*` folders do) can ask how close the copy
came:

```
d2-client ai-closeness --classes 141,142,143 --ai 30 --seeds 8 --ticks 400 --mod-dir MyMod --native NATIVE_DIR
```

For each class and each seed (counting up from `--seed`) it runs the test arena twice: on D2Gamma's own AI (the Original
ruleset, no graphs) and under the mod, whose graph has `index` = `--ai`, the AI table row it was lifted from. It prints, per
class, the means of the ticks the monster stood, walked, attacked and cast, how near and far it was from the character, the hit
points each side lost and how often the monster died, side by side. A number is marked `BIG` when the two differ by more than a
quarter of the larger and by more than its slack (a few ticks, a sub-tile, a hit point), so a small mean is not a big
difference. It is a report, not a check: the exit status says only whether it could be made. Use `--out FILE` to keep it.

## 10. Built-in AIs a graph cannot say (yet)

`templates/ai-lift-*` holds a data copy of the built-in AIs that more than one `monstats` row uses and that the vocabulary can
express; each folder's README has the numbers of how close it runs (section 9). The rest are listed here with the part a graph
cannot say. "Read" means I read the body's code; "keywords" means the AI was judged from the name of its body, the
parameters it reads and the calls it makes, without a line-by-line read, so a few of those could still turn out to be
expressible.

**Left out of a lift that exists** (the lift covers the rest of the body):

| AI | What is missing |
| --- | --- |
| NecroPet (67) | the ranged form (the skeleton mage), picked by the first AI param the raising skill writes; the arena gives a monster no owner, so closeness is not measured |
| MinionSpawner (121) | the cap on live minions (`aip2`) and the total cast cap (`aip1`); a graph can count a class near it, not total casts |
| SandRaider (8) | walking to the nearest evil monster below `aip1` life, and the overlay at the power-up think |
| SuccubusWitch (119) | the curse tests of Nightmare and Hell (`aip3`/`aip4` are 0 in Normal) |
| PantherWoman (18) | the pack scan (walk to a pack mate that is farther than `aip3`) |
| ClawViper (16) | the glow state after the first charge (a look only) |
| Zombie (3), Mummy (21), QuillMother (75) | the branch for the AI state the game sets on a monster (state 3 or 19) |
| GreaterMummy (22) | the quest call of Radament (class 229) and its wider scan |
| CorruptRogue (10) | the run distance that falls with the difficulty; the lift holds Normal's 20 |
| FrozenHorror (124), Megademon (89) | the distance of the cast is the skill level of skill 1 in the body; the graph holds the number |
| BloodHawk (5), PinHead (39) | the body's flag is kept in an AI param; the graph keeps it with two states and the think counter, which differs when the monster is walked into (BloodHawk runs partly close, 2 BIG numbers; PinHead is identical to the tick in the arena) |

**Not lifted, read** (in the order of rows):

| AI | Why a graph cannot say it |
| --- | --- |
| Fallen (6), FallenShaman (13), Scarab (20), FetishShaman (65), FetishBlowgun (96) | their commands: other monsters give them orders (go to a unit, attack a unit) and they flee from a fallen mate's corpse or raise one; a graph has no commands |
| Baboon (11) | a hit-and-run machine built on hit point regeneration stats and AI params |
| QuillRat (14) | commands and the AI-state branch; the rest is a plain walk and swing |
| Vulture (23) | the landing and flight machine in AI params |
| WillOWisp (25) | a ritual: the wisps find each other and dance on points fixed by the game frame |
| Arach (26) | a state machine in AI params, and the AI-state branch |
| CorruptArcher (35) | a step counter of the shot, and a secondary-target walk with a distance only it uses |
| Succubus (118) | the curse tests (target life, mana against life, is it a player) |
| Imp (122) | teleport and a quest hook |
| SuicideMinion (117), MaggotEgg (40), BaalTentacle (139), Hydra (86) | they die on a timer or when their owner dies; a graph has no "die" action |
| SiegeBeast (115), PutridDefiler (137), VileMother (68) | a scan for a rider or host unit, or a birth that creates a unit of a given class near it |
| BaalMinion (141) | a wait after every action (`aip4` frames) that a graph cannot append |

**Not lifted, judged from keywords** (not read line by line):

| AI | Why |
| --- | --- |
| SandMaggot (15), SandMaggotQueen (66), ThornHulk (27) | a state machine in AI params that counts thinks and compares against frame stamps |
| BatDemon (29), Mosquito (24), MosquitoNest (83), EvilHole (76), FoulCrowNest (43), Overseer (120), Towner (41) | the same: counters and frame stamps in AI params, or a wait after each action |
| ZakarumPriest (49), ZakarumZealot (48) | a scan for hurt allies of two named classes, and a frame-stamped cooldown |
| Hireable (61) | the hireling's order, follow and equipment logic |
| Regurgitator (71), FingerMage (70), OblivionKnight (74), HighPriest (85) | a scan for a food or target unit of a given class, or a birth |
| Catapult (123), CatapultSpotter (126), DesertTurret (94), AssassinSentry (101), Totem (109), ElementalBeast (46), Tentacle (56), TentacleHead (57), Ancient (133), AncientStatue (132) | stationary units (turrets, statues, tentacles): they attack with no target or on a timer |
| Trap-Missile (77), Smith (98), TrappedSoul (99), DruidWolf (108), CycleOfLife (111), NpcBarb (127), NpcOutOfTown (31), NpcStationary (54), Vendor (42), Andariel (34), Duriel (44), Diablo (51), FrogDemon (52) | scripted units: quest state, a die on a timer, or the boss logic of the game |

Several of these share a missing piece that would be worth a new condition or action: a "die" action, a command condition (what
order the monster was given), a frame-stamped timer. The ones that needed only a negated test or a counter were added
(`out_of_melee`, `thinks_at_least`).
