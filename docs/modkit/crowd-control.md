# Declared crowd control (`crowd_control.toml`)

> Interrupts and stuns only from sources that declare it. No random projectile cancels my teleport.

D2Gamma lets almost any hit interrupt (the get-hit animation), and lets missile rows, skills and items stun, freeze
and shove. With the rules switch below, a mod turns that around: a hit **stuns, interrupts, knocks back or freezes
only when its source says so**. Everything is decided on the server, in the same tick as the hit.

A worked, game-file-free template is `templates/boss-interrupts-only`: the player is declared-only, one boss
interrupts with its first attack and stuns for 1.5 seconds with a special attack.

A mod can also name a state that makes its holder immune to all of it, for example a boss during a phase change
(`immune_state`, see the last section).

## 1. The switch (`files/rules.toml`)

```toml
version = 1
crowd_control = "declared"      # "classic" (default) or "declared"

[players]
crowd_control = "declared"      # optional: override for hits on players
[monsters]
crowd_control = "classic"       # optional: override for hits on monsters
```

The setting follows the **defender**: `players.crowd_control` applies to hits on players (a transformed player
too), `monsters.crowd_control` to hits on monsters; a class that has none uses the top-level key. `"classic"` is
D2Gamma's rule exactly. A game with no mod, or a mod that never writes this key, plays and hashes as before.

In `"declared"` mode a hit on that class:

* **does not interrupt** (no get-hit animation, no cancelled cast), whatever its size, unless its source declared an
  interrupt, a stun or a freeze;
* **does not stun, freeze or knock back** unless its source declared that effect, with the declared length;
* still does its damage, kills, and gives its events and triggers.

The `[players.hit_recovery]`, `.stun`, `.freeze` and `.knockback` switches of [rules.md](rules.md) still apply on
top: `enabled = false` still blocks the effect, and the duration percent still scales a declared length. A frozen
defender still cannot be interrupted. A stun implies an interrupt, as in the game.

## 2. The sources

A source says *what* it causes with an **effect**:

| `kind` | Does | Keys |
| --- | --- | --- |
| `interrupt` | Cancels what the defender is doing (the get-hit reaction) whatever the size of the hit; passes the "too small to flinch" thresholds. | `chance` |
| `knockback` | Shoves the defender back. | `chance` |
| `stun` | Stuns for `seconds` (1/25 s steps, 0 to 10 s). Interrupts too. | `seconds`, `chance` |
| `freeze` | Freezes for `seconds`. | `seconds`, `chance` |

`chance` is a percent, 1 to 100 (default 100), rolled from the **attacker's own** seed only when the hit lands, so a
replay gives the same game. A source has at most 8 effects.

### Skill parts (`skills.toml`)

`melee_hit` and `area_hit` parts (and `missile` parts, for the missiles they fire) take any number of
`[[skill.part.cc]]` blocks:

```toml
[[skill]]
id = 395
[[skill.part]]
kind = "area_hit"
radius = 8
around = "caster"
[skill.part.damage]
min = 40
max = 40
[[skill.part.cc]]
kind = "stun"
seconds = 1.5
```

A part with a `cc` block also plays the get-hit reaction, so an area hit that stuns shows the stun. A part without
one behaves as before (an area hit plays no reaction; under declared rules it causes no effect).

### Monster attacks, missile rows and skill rows (`files/crowd_control.toml`)

```toml
version = 1

[[source]]
monster = 156          # the monstats row
attack = "a1"          # a1, a2, s1..s4; leave out (or "any") for every attack
[[source.effect]]
kind = "stun"
seconds = 1.5

[[source]]
missile = 214          # a missiles row; or `skill = N` for the missiles that skill made
[[source.effect]]
kind = "interrupt"
```

Exactly one of `monster`, `missile` or `skill` per source; `attack` goes only with `monster`. Several sources
that match one hit all apply, in file order, and the longest declared stun or freeze wins. The file is checked when
the mod loads, with errors like ``source[2].effect[0]: `seconds`: a stun or freeze needs a length``. The same
declaration works under `"classic"`: it adds the effect to what D2Gamma already does.

### Items and triggers

* An item **property that gives a stun length** (the game's stun-length stat) and the item **freeze** event are
  declared by the attacker's gear: they keep working under declared rules.
* A [trigger](skills.md#actions) can cause an effect with the `crowd_control` action: for "this weapon causes stun
  0.5 s", give the item's property a stat and put a trigger on it:

  ```toml
  [[trigger]]
  on = "hit"
  while_stat = 340
  chance = 20
  [[trigger.action]]
  kind = "crowd_control"
  who = "other"
  effect = { kind = "stun", seconds = 0.5 }
  ```

  The trigger hits the unit it names (`who`, default the other unit of the event) with a hit that does no damage,
  so it follows that unit's own `crowd_control` setting like any other hit.

## 3. What is not declared (and so does not happen)

Under `"declared"` for the defender class: a plain melee swing by a monster that has no `crowd_control.toml` entry,
a missile whose row and skill have none, a thrown weapon, a poison tick and a hit of any size. An engine skill that
the game itself makes stun (for example a hammer's stun) is a *missile* or *melee* source: declare it by
`skill = N` or a row in `crowd_control.toml`, or the defender is not stunned.

## 4. Errors

A wrong file is refused whole when the mod loads, naming the entry and the key: an unknown key, `kind` that is not
one of the four, `seconds` on an interrupt or missing on a stun, `seconds` above 10, `chance` outside 1 to 100,
more than 8 effects, a `source` with none or several of `monster` / `missile` / `skill`, and an `attack` that is not
`a1`, `a2`, `s1`..`s4` or `any`.

## 5. The uninterruptible cast point (`rules.toml`)

Declared crowd control says *who* may interrupt. The cast point says *when it is too late*: a fraction of a skill's
cast animation after which nothing cancels it.

```toml
[cast]
uninterruptible_after_percent = 60   # default for every skill; leave out (or 100) for D2Gamma's rule

[[cast.skill]]                       # a skills row of its own
id = 54                              # Teleport: committed from the first frame
uninterruptible_after_percent = 0
```

* The number is a **percent of the cast animation** (0 to 100). 0 means from the first frame; 100 means never (the
  cast can be cancelled until it ends, as in D2Gamma). A `[[cast.skill]]` entry wins over the default, both ways
  (`100` makes one skill cancellable again). The default and the entries are the same for players and monsters; the
  rule applies to any unit casting or attacking with a used skill (attack, cast, throw, kick and special modes).
* Past the point a **hit does not interrupt** it (no get-hit, no knockback, declared or not) and a **new action does
  not replace it** (the skill-use gate refuses the new mode). A hit that kills still kills, and death still comes.
  Stun and freeze states still apply to the unit; they just do not cancel this cast.
* Before the point everything works as the rest of the rules say.
* **Server and client use the same point.** Both read the cast's animation frame against the same percent
  (`d2_sim::skills::cast_point`). The server decides; the client's model of the local player flips to "committed" on
  the same animation frame (`bridge::player_anim::cast_committed`), and a test steps one cast through both and checks that
  they agree on every frame. A cast the client has passed the point of is therefore one the server will not take
  back. A very laggy player can still be interrupted *before* the point on the server while the client is already
  past it; making the point fair to a high ping is the later latency milestone and is not built.
* A mod that wants "a stray hit never cancels my teleport, only the boss does" combines the two: declared crowd
  control on the player, and a point of 0 for Teleport.

### Block without the animation

`block_animation = false` in `rules.toml` ([rules.md](rules.md)) keeps a block's damage prevention and its `BLOCK`
result, but the defender plays **no block animation and is not held**: it keeps the mode it was in (a swing or a
cast goes on), the player's block lockout timer is not set, and no stun or hold state is put on it. A monster that
blocks without the animation only gets D2Gamma's "was just hit" AI mood (AI state 19, the same as a monster class
with no block animation has), which changes what its AI decides next but never stops it from acting.

## Immunity state (`immune_state`)

```toml
version = 1
immune_state = 900      # a state id of the mod's states.toml
```

A unit that holds this state takes **no** stun, freeze, interrupt or knockback from any hit, whether the source
declared it or not, under either rule. Damage is unaffected. Nothing else puts the state on a unit by itself: a
monster AI graph does it for a time at a boss phase (`immune_seconds` in `ai.toml`, see
[monster AI](ai.md)), and so can a trigger or a skill that applies the state.
