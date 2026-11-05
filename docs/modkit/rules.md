# Global rules (`rules.toml`, version 1)

The authoritative host parses `GlobalRules::from_toml(&text)` before starting
a game. Install `rules.into_ruleset()` on the host's rules configuration (and
`ActionHooks::rules` when using the sim directly). A host with additional mod
providers returns the parsed struct from `ModRules::global_rules` instead.
The `d2-server::game::GameBuilder::rules` builder method accepts this ruleset.
For a mod started with `--mod` / `--mod-dir`, the [data mod host](../mods/data-mod-host.md) finds the file
(`files/rules.toml` of the last mod in the stack that has one), parses it at start and installs it. Online clients send intents; they
cannot send or install these settings on the server.

An empty file is exactly D2Gamma's rules. Omitted keys and tables keep their
defaults. Unknown keys, unsupported versions, wrong types and invalid ranges
are errors. No game assets or tables belong in this file.

```toml
version = 1
block_animation = false
max_block_chance = 75

[players.hit_recovery]
enabled = false
duration_percent = 100

[monsters.stun]
enabled = true
duration_percent = 150
```

In the table below, `target` means either `players` or `monsters`. Each target
has independent settings; transformed players use the player settings.

| Key | Default | Effect |
| --- | --- | --- |
| `version` | `1` | Schema version. Only 1 is supported. |
| `drops.players_setting` | `true` | Honor the single-player `/players N` floor. |
| `drops.party_bonus` | `true` | Count nearby living party members fully for NoDrop. |
| `drops.other_player_divisor` | `2` | Divide the contribution of other players by this integer (1–8). |
| `drops.monster_spawn_cap` | `true` | Limit NoDrop exponent by spawn-time monster player count. |
| `drops.magic_find_percent` | `100` | Scale MF before diminishing returns (0–10000). |
| `drops.magic_find_cap` | omitted | Upper limit on scaled MF (0–10000). |
| `drops.nodrop_percent` | `100` | Scale raw TC NoDrop before player exponent (0–10000). |
| `socket_level_bands` | `[25, 40]` | Inclusive upper item levels of first/second socket bands. |
| `runeword_max_runes` | `6` | Maximum runeword length, 0–6; 0 disables table runewords. |
| `death_gold_percent` | `100` | Scale death gold loss before protection/allocation (0–10000). |
| `hireling_experience_curve_percent` | `100` | Scale hireling threshold and initial offer XP (1–10000). |
| `resistances.elemental_cap` | `75` | Base cap on non-monster resistance rows with a max-resistance stat, including magic. Adds the matching max-resistance stat bonus. Range 0–100. |
| `resistances.elemental_max_cap` | `95` | Ceiling after those bonuses. Range 0–100; must be at least `elemental_cap`. |
| `resistances.physical_cap` | `50` | Non-monster physical resistance cap, 0–100. |
| `resistances.other_cap` | `75` | Cap for remaining resistance rows without a max-resistance stat, including poison-length reduction, 0–100. |
| `resistances.expansion_penalties` | omitted | `[Normal, Nightmare, Hell]` signed penalties. Omitted uses difficulty table data (base data normally 0/-40/-100). Each entry must be -1000–1000. |
| `resistances.classic_penalties` | omitted | Same three overrides for classic games; omitted keeps 0/-20/-50. |
| `experience_curve_percent` | `100` | Scale all player level XP thresholds (1–10000). 200 requires twice as much XP; 50 requires half. Kill XP and hireling curves are unchanged. Includes formula-provided thresholds and both checked/unchecked threshold lookups. |
| `death_experience_percent` | `100` | Scale the computed death XP loss (0–10000), before the current-level floor and corpse recovery. 0 disables that loss, 200 doubles it. Existing player-side-killer exemption remains. Gold penalties are unchanged. |
| `block_animation` | `true` | Allow player and monster block animations. Turning this off preserves block damage prevention and the block result. |
| `max_block_chance` | `75` | Shield block cap, 0–100 percent, before running penalties. Existing formula providers retain their behaviour at the default cap; a changed cap also limits their result. Weapon block remains its own skill rule. |
| `crowd_control` | `"classic"` | `"declared"`: a hit stuns, interrupts, knocks back or freezes only when its source declares it ([crowd-control.md](crowd-control.md)). `"classic"` is D2Gamma's rule. |
| `players.crowd_control`, `monsters.crowd_control` | omitted | Override `crowd_control` for hits on that unit class (players include transformed players). |
| `cast.uninterruptible_after_percent` | omitted | Percent (0–100) of a skill's cast animation after which a hit or a new action cannot cancel it. Omitted or 100: cancellable until it ends. See [crowd-control.md](crowd-control.md#5-the-uninterruptible-cast-point-rulestoml). |
| `[[cast.skill]]` (`id`, `uninterruptible_after_percent`) | none | The same for one skills row; wins over the default. |
| `target.hit_recovery.enabled` | `true` | Allow damage get-hit interruption, including the stunned-defender reaction. |
| `target.hit_recovery.threshold_denominator` | omitted | Omitted: use D2Gamma's hit-class divisor and seeded random middle bands. Set N (1–2147483647) to require damage of at least max life / N instead, without those random bands. Tiny hits (below `minimum_damage`), poison-only hits, frozen targets and missing monster get-hit modes retain their guards. |
| `target.hit_recovery.duration_percent` | `100` | Scale get-hit animation duration by inversely scaling animation speed. |
| `target.knockback.enabled` | `true` | Allow knockback reactions, including the sandleaper reaction. |
| `target.knockback.duration_percent` | `100` | Scale travel duration through inverse velocity and animation speed; the path distance budget stays the same. Collision can end travel early. |
| `target.stun.enabled` | `true` | Allow stun state application. |
| `target.stun.duration_percent` | `100` | Scale duration after D2Gamma's stun caps; boss and immobile-monster immunity remains; unique resistance and duration caps are configurable. |
| `target.freeze.enabled` | `true` | Allow freeze application, including its fallback to cold slow on players, bosses, unique monsters and hirelings. Does not disable ordinary cold damage/slow. |
| `target.freeze.duration_percent` | `100` | Scale freeze length (including its cold fallback) before difficulty scaling. Existing immunity guards remain. |

Duration percentages accept integers 0–10000: 100 preserves timing, 200 doubles
duration, 50 halves it, and 0 disables that reaction/effect. Math truncates to
integer ticks or speed units and saturates on overflow. Positive animation
and movement speeds have a minimum of one when scaling is requested.

D2Gamma does **not** currently use a universal 1/12-max-life recovery threshold.
Its default path is preserved, including RNG draw order. A mod wanting a
deterministic 1/12 threshold can explicitly set `threshold_denominator = 12`.
The canonical Rust doc-comment setting table is on `rules::global::GlobalRules`
for future Mod Studio descriptions.

Resistance caps apply to non-monster defenders; monster immunities retain their
current rules. Explicit per-defender formula hooks still take precedence over
curated resistance settings; configured difficulty penalties take precedence
over the shared difficulty formula/table fallback. Physical and magic resistance
remain exempt from difficulty penalties. Existing resistance floors, pierce and
absorption rules remain in place.

XP thresholds use integer scaling, saturating at `u32::MAX`; very large multipliers
can make upper thresholds equal at that ceiling. Death loss multipliers above
100 are limited to XP available above the current level's floor. The corpse
keeps D2Gamma's existing recovery fraction of the resulting loss. Raw table
queries without a `Ruleset` keep returning raw table values.

Drop rules apply to every TC walk (monster deaths, Find Item, chests, objects and
quest TC helpers) through `treasure::rules::DropRules` and `DropSink::drop_rules`.
The item sink reads the authoritative host rules. Standalone sinks default to
D2Gamma's settings. Direct quest rewards that create a named item have no TC
NoDrop or quality roll and are unaffected.

The default NoDrop exponent is `party + (players - party) / 2` (integer division).
Party is the nearby living party count, capped at 8, or 1 without a party.
Players is the living count with the `/players N` floor in game types 1–3.
Monster drops cap this exponent at the monster's spawn-time count (at least 1).
Turning off `party_bonus` uses party = 1; changing `other_player_divisor` replaces
2 in the formula. Disabling `players_setting` removes only the simulated-player
floor. Disabling `monster_spawn_cap` uses current counts for monsters too.
These rules change successful drop counts through NoDrop; TC picks, negative
ordered picks and caller output limits are unchanged. Find Item continues to
bypass NoDrop. `nodrop_percent` scales the raw TC weight before exponentiation.
Zero removes NoDrop. Scaling truncates and saturates to a signed 32-bit integer.

MF scales the combined recipient/owner stat once, before quality diminishing
returns. The optional cap applies after scaling; it never raises negative MF.
Forced qualities and TC unique/set entries keep their existing precedence.

Socket bands select the itemtypes `maxsock1`, `maxsock25`, `maxsock40` columns;
item-row maximum sockets remains a limit. Bands must be two ordered positive
levels no greater than 2147483647. Runeword length can be lowered within the
six-rune table/save format. Existing item-provider runeword overrides take
precedence; quality, quest-item, socket-count and table type checks remain.

Death gold scaling applies to the computed level-based loss, before the
single-player protected balance and carried-gold limit. Non-default losses clamp
to available total gold. PvP and PvE retain their distinct allocation/drop
rules. Setting 0 removes the lost amount; PvE still drops carried gold on the
ground. Carried gold, banked gold, ground gold and `goldlost` use the same plan.

Hireling curve scaling covers thresholds, level restoration, next-XP, kill-share
caps and newly hired mercenary XP, including provider thresholds/offer XP. It
uses unsigned XP scaling, saturating at `u32::MAX`; changing curves in an existing
save can change its restored level. Hireling stats/skills, offer levels, maximum
level, XP sharing and per-kill cap fractions retain their existing rules.


## Animation feel and crowd-control numbers

All duration fields ending in `_frames` use **25 Hz data frames** (one frame
is 40 ms), converted to the host rate after existing duration scaling. They
are not 60 Hz simulation frames. Player and monster sections are independent;
transformed players keep the player control settings. Hireling, unique and
freeze-divisor fields only affect monsters, even though the shared target
schema accepts them in both sections.

| `animation.classic_25hz` | `false` | Opt into 40 ms animation steps; simulation and movement remain 60 Hz. |
| `animation.diminishing` | `[120,120,120,120,150]` | Coefficients for attack/recovery/cast/block/movement, each 1–10000. |
| `animation.attack_percent` | `[15,175]` | Inclusive lower/upper attack-rate percent bounds, ordered in 1–10000. |
| `animation.cast_base_percent` | `100` | Base cast percent before diminished bonus, 1–10000. |
| `animation.cast_max_percent` | `175` | Maximum cast percent, 1–10000. |
| `animation.recovery_base_percent` | `50` | Base hit-recovery percent, 1–10000. |
| `animation.block_base_percent` | `50` | Base block percent without Holy Shield, 1–10000. |
| `animation.holy_shield_block_percent` | `100` | Base block percent with Holy Shield, 1–10000. |
| `animation.block_min_speed` | `1` | Block animation speed floor, 1 through speed_cap. |
| `animation.movement_min_percent` | `25` | Walk/run movement percent floor, 1–10000. |
| `animation.movement_max_percent` | omitted | Optional walk/run movement percent cap, at least the floor and at most 10000. |
| `animation.other_percent` | `[15,175]` | Inclusive percent bounds for other animation modes, ordered in 1–10000. |
| `animation.speed_cap` | `32767` | Animation speed ceiling in 8.8 units, 1–32767. |
| `animation.sequence_attack_penalty` | `30` | Percent subtracted for player sequence attacks, 0–10000. |
| `animation.player_walk_speed` | `213` | Player walk animation base speed in 8.8 units, 1–10000. |
| `animation.player_run_speed` | `101` | Player run animation base speed in 8.8 units, 1–10000. |
| `animation.frame_bonus` | D2Gamma table | 12 weapon-type rows × 7 player-class columns, each 0–2 frames. |
| `target.hit_recovery.minimum_damage` | `256` | Minimum damage in 8.8 life units, 0–2147483647. |
| `target.hit_recovery.divisors` | `[8,16,32,64]` | Life divisors for light/default/heavy/long-sword hit classes, each 4–1000000. |
| `target.hit_recovery.band_denominators` | `[2,4]` | Divide the selected divisor by these ordered values for the seeded bands, each 1 through the smallest divisor. |
| `target.hit_recovery.band_roll_sides` | `[2,4]` | Reject each band when its seeded draw is zero; powers of two 1–256. |
| `target.control.stun_max_frames` | `250` | General stun cap before duration scaling, 1–1000000 data frames. |
| `monsters.control.hireling_stun_max_frames` | `13` | Hireling stun cap, 1–1000000 data frames; replaces the general cap once reached. |
| `monsters.control.unique_stun_resist_percent` | `90` | Unique monster stun resistance, 0–100 percent. Bosses and immobile monsters retain immunity. |
| `target.control.freeze_max_frames` | omitted | Optional freeze cap before duration scaling and difficulty; 1–1000000 data frames. Also caps the player/immune-monster cold fallback. |
| `monsters.control.freeze_divisors` | omitted | Normal/Nightmare/Hell freeze divisors, each 1–1000000. Omitted reads difficulty tables. Cold fallback keeps cold-divisor rules. |
| `target.control.knockback_velocity` | `4096` | Knockback path speed before duration scaling, 1–32767 fixed-point units per data frame. |
| `target.control.knockback_distance` | `5` | Knockback path distance budget, 1–255. |
| `monsters.control.sandleaper_knockback_distance` | `10` | Distance budget for the leaping monster family, 1–255. |

Hit-class divisor order is light (`1hss`, `1ht `, `bow `, `xbow`), default,
heavy (`2hss`, `club`), long sword (`2hsl`). With an explicit
`threshold_denominator`, the class divisors and seeded bands are bypassed;
minimum damage, poison-only, frozen and missing-mode guards still apply.
Otherwise, a band rejects a hit with probability 1 / `band_roll_sides`.
Defaults preserve both branch order and seed draws.

Breakpoints are computed from animation frame counts and integer speed math;
there is no separate list of item-stat breakpoint values. `diminishing` gives
c in `c * stat / (c + stat)` for each row. A zero denominator contributes zero.
Movement uses the path table coefficient when the movement row remains at its
default 150, preserving custom Rust path providers; a changed row overrides it
for both path speed and animation. `movement_max_percent` is absent by default
because D2Gamma has no movement-percent ceiling. Movement percent bounds
apply to both path travel and animation; `speed_cap` only limits animation.
Player walk/run speeds are animation bases, while physical base velocities
remain in character/monster tables. Monster animation bases remain table data.

The frame-bonus table is indexed by weapon type class (0–11), then player
class (0–6). Its default row 0 is `[1,1,0,0,0,0,0]`, rows 2–6 are
`[2,2,0,0,0,0,0]`, and all other rows are zero. It applies to attack modes
and the existing dual-wield skill modes; monster animations have no bonus.

```toml
[animation]
classic_25hz = true
diminishing = [120, 120, 120, 120, 150]
attack_percent = [15, 175]

[players.control]
stun_max_frames = 125
freeze_max_frames = 100
knockback_distance = 3

[monsters.hit_recovery]
minimum_damage = 128
divisors = [8, 16, 32, 64]
band_denominators = [2, 4]
band_roll_sides = [2, 4]
```

`classic_25hz = false` keeps D2Gamma's fine 60 Hz animation breakpoints.
The opt-in classic setting quantizes animation advancement, action events and
completion to the world's 40 ms animation grid. Speeds that finish within the
same data-frame count share a breakpoint. Sequences and animation variants use
the same clock. At 60 Hz, a 40 ms boundary maps to the next simulation tick,
so steps alternate two and three host ticks; animations starting between grid
boundaries share that world-clock phase. The host, input processing, movement,
state timers and network clock remain at 60 Hz. This is animation quantization,
not a per-mod change to the real simulation rate: changing that rate would also
require coordinated host pacing, client prediction and protocol changes.
Render interpolation remains the client's existing policy.
