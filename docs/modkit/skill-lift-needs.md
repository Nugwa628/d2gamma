# Player skill lift needs

Baseline: main `1df762e02a1d17c553aab1bcc7964bf5230156a4`; seven CI install reports, 210 class rows: 52 lifted, 52 same in the bounded fixture, 158 needing a part or fixture. Counts below are candidate unlocks, not claims of parity. Formula and host fixtures can reduce the immediately liftable count. No install cells or assets are included.

Each baseline row appears once below, grouped by its actual do function. Rows with no do function and no missile need passive/start classification; they must not become fake no-op casts.

| missing mechanism / fixture | do | candidate skills | classes (count) | ownership / dependency |
| --- | ---: | ---: | --- | --- |
| Passive stats / start-only rows (classify without inventing a cast) | 0 | 26 | Amazon (6), Sorceress (4), Necromancer (3), Barbarian (10), Druid (1), Assassin (2) | Table/passive classification; no cast part assumed |
| Timed self buff, passive stats and event callbacks | 18 | 9 | Sorceress (3), Necromancer (1), Paladin (1), Druid (1), Assassin (3) | mk-world; class-specific lift / fixture |
| Weapon attack with state | 2 | 8 | Amazon (2), Paladin (1), Barbarian (4), Druid (1) | mk-blocks; reuse merged generic part |
| Druid spirit/wolf/bear summons | 119 | 6 | Druid (6) | mk-world; class-specific lift / fixture |
| Placed sentry traps | 45 | 5 | Assassin (5) | mk-world; class-specific lift / fixture |
| Warcry ring and timed party state | 68 | 5 | Barbarian (5) | mk-world; class-specific lift / fixture |
| Damage aura pulse | 66 | 4 | Paladin (4) | mk-world; class-specific lift / fixture |
| Multiple-shot pattern | 8 | 3 | Amazon (1), Necromancer (1), Druid (1) | mk-world; class-specific lift / fixture |
| Fend/Zeal weapon sequence | 13 | 3 | Amazon (1), Paladin (1), Druid (1) | mk-blocks; reuse merged generic part |
| Martial-arts strike charge-up | 34 | 3 | Assassin (3) | mk-world; class-specific lift / fixture |
| Elemental claw charge-up | 35 | 3 | Assassin (3) | mk-world; class-specific lift / fixture |
| Golem summon | 56 | 3 | Necromancer (3) | mk-world; class-specific lift / fixture |
| Vine summons | 115 | 3 | Druid (3) | mk-world; class-specific lift / fixture |
| Amazon sight/debuff pulse | 6 | 2 | Amazon (2) | mk-world; class-specific lift / fixture |
| Guided missile | 10 | 2 | Amazon (1), Necromancer (1) | mk-world; class-specific lift / fixture |
| Inferno channel | 19 | 2 | Sorceress (1), Druid (1) | mk-blocks; reuse merged generic part |
| Blaze/Energy Shield attached state | 23 | 2 | Sorceress (2) | mk-world; class-specific lift / fixture |
| Skeleton corpse summon | 31 | 2 | Necromancer (2) | mk-world; class-specific lift / fixture |
| Shadow summons | 49 | 2 | Assassin (2) | mk-world; class-specific lift / fixture |
| Shapeshift form, group toggle and skill gating | 116 | 2 | Druid (2) | mk-world; class-specific lift / fixture |
| Twister/Tornado movement pattern | 118 | 2 | Druid (2) | mk-world; class-specific lift / fixture |
| Feral Rage/Maul charged weapon hit | 120 | 2 | Druid (2) | mk-blocks; reuse merged generic part |
| Armageddon/Hurricane periodic ground effects | 124 | 2 | Druid (2) | mk-blocks; reuse merged generic part |
| Jab weapon sequence | 7 | 1 | Amazon (1) | mk-blocks; reuse merged generic part |
| Frenzy weapon/charge state | 9 | 1 | Barbarian (1) | mk-blocks; reuse merged generic part |
| Charged strike | 11 | 1 | Amazon (1) | mk-blocks; reuse merged generic part |
| Strafe sequence | 12 | 1 | Amazon (1) | mk-world; class-specific lift / fixture |
| Lightning strike chain | 14 | 1 | Amazon (1) | mk-blocks; reuse merged generic part |
| Amazon decoy | 15 | 1 | Amazon (1) | mk-world; class-specific lift / fixture |
| Amazon Valkyrie | 16 | 1 | Amazon (1) | mk-world; class-specific lift / fixture |
| Charged bolt pattern | 17 | 1 | Sorceress (1) | mk-world; class-specific lift / fixture |
| Static-field life percentage | 20 | 1 | Sorceress (1) | mk-world; class-specific lift / fixture |
| Telekinesis target action | 21 | 1 | Sorceress (1) | mk-world; class-specific lift / fixture |
| Fire Wall ground missiles | 24 | 1 | Sorceress (1) | mk-blocks #1229; reuse when merged |
| Enchant target weapon buff | 25 | 1 | Sorceress (1) | mk-world; class-specific lift / fixture |
| Chain lightning pattern | 26 | 1 | Sorceress (1) | mk-world; class-specific lift / fixture |
| Thunder Storm periodic targeting | 29 | 1 | Sorceress (1) | mk-world; class-specific lift / fixture |
| Poison dagger weapon hit | 32 | 1 | Necromancer (1) | mk-blocks; reuse merged generic part |
| Psychic Hammer | 33 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Dragon Talon finisher | 42 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Shock Web placement | 43 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Blade Sentinel | 44 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Dragon Claw finisher | 46 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Cloak state and enemy debuff | 47 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Blade Fury channel | 48 | 1 | Assassin (1) | mk-blocks; reuse merged generic part |
| Dragon Tail finisher | 50 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Mind Blast control/damage | 51 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Dragon Flight finisher movement | 52 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Blade Shield attached damage | 54 | 1 | Assassin (1) | mk-world; class-specific lift / fixture |
| Corpse Explosion | 55 | 1 | Necromancer (1) | mk-world; class-specific lift / fixture |
| Iron Golem item conversion | 57 | 1 | Necromancer (1) | mk-world; class-specific lift / fixture |
| Revive corpse summon | 58 | 1 | Necromancer (1) | mk-world; class-specific lift / fixture |
| Attract AI curse | 59 | 1 | Necromancer (1) | mk-world; class-specific lift / fixture |
| Bone Wall placement | 60 | 1 | Necromancer (1) | mk-world; class-specific lift / fixture |
| Confuse AI curse | 61 | 1 | Necromancer (1) | mk-world; class-specific lift / fixture |
| Bone Prison placement | 62 | 1 | Necromancer (1) | mk-world; class-specific lift / fixture |
| Poison Explosion corpse | 63 | 1 | Necromancer (1) | mk-world; class-specific lift / fixture |
| Sacrifice weapon hit | 64 | 1 | Paladin (1) | mk-blocks; reuse merged generic part |
| Charge movement/weapon hit | 67 | 1 | Paladin (1) | mk-blocks; reuse merged generic part |
| Corpse potion finding | 69 | 1 | Barbarian (1) | mk-world; class-specific lift / fixture |
| Double Swing weapon sequence | 70 | 1 | Barbarian (1) | mk-blocks; reuse merged generic part |
| Taunt AI control | 71 | 1 | Barbarian (1) | mk-world; class-specific lift / fixture |
| Corpse item finding | 72 | 1 | Barbarian (1) | mk-world; class-specific lift / fixture |
| Blessed Hammer trajectory | 73 | 1 | Paladin (1) | mk-world; class-specific lift / fixture |
| Double Throw weapons | 74 | 1 | Barbarian (1) | mk-blocks; reuse merged generic part |
| Corpse Grim Ward | 75 | 1 | Barbarian (1) | mk-world; class-specific lift / fixture |
| Whirlwind movement/weapon sequence | 76 | 1 | Barbarian (1) | mk-blocks; reuse merged generic part |
| Leap movement | 77 | 1 | Barbarian (1) | mk-world; class-specific lift / fixture |
| Leap Attack movement/hit | 78 | 1 | Barbarian (1) | mk-blocks; reuse merged generic part |
| Conversion ownership | 79 | 1 | Paladin (1) | mk-world; class-specific lift / fixture |
| Fist of the Heavens bolt pattern | 80 | 1 | Paladin (1) | mk-world; class-specific lift / fixture |
| Holy Freeze damage aura | 81 | 1 | Paladin (1) | mk-world; class-specific lift / fixture |
| Redemption corpse aura | 82 | 1 | Paladin (1) | mk-world; class-specific lift / fixture |
| Raven summon | 114 | 1 | Druid (1) | mk-world; class-specific lift / fixture |
| Firestorm pattern | 117 | 1 | Druid (1) | mk-world; class-specific lift / fixture |
| Rabies propagation weapon hit | 121 | 1 | Druid (1) | mk-blocks; reuse merged generic part |
| Hunger weapon/life steal | 122 | 1 | Druid (1) | mk-blocks; reuse merged generic part |
| Volcano ground missiles | 123 | 1 | Druid (1) | mk-blocks; reuse merged generic part |
| Hydra elemental sentry | 144 | 1 | Sorceress (1) | mk-world; class-specific lift / fixture |
| Smite shield fixture and formula | 150 | 1 | Paladin (1) | mk-blocks #1222; shield/weapon fixture and calc2 opcode 0x01 |

## Formula and fixture prerequisites

- Authoring-time formula translation supports all built-in skill parameters, operators and the seven skill formula functions. Table names resolve once to stable numeric rows; unknown names, malformed stacks and unsupported bytecode stay explicit errors. Linked skills still need representative learned-level fixtures.

- Buffs need state duration, exclusive groups, aura/passive stats and event callbacks compared. Warcries also need missile count and caster/party state coverage.

- Summons need a generated monster fixture, pet owner/type/cap, class mode and skill/base stat setup. Druid, vine, shadow, trap and golem initialization must remain distinct. Reuse the existing summon seam; generic spawn stays with mk-blocks.

- Forms need exclusive-state toggle, duration, removal callback and actual form skill gating. Martial arts need charge accumulation, cap and consumption plus a weapon fixture.

- Corpse skills need dead eligible targets, consumption and deterministic item/drop or revive observations. Curses need actual AI disposition, not only state presence.

- All reports retain the Rust default and mark differences over 25%; "same" describes only observed fixture metrics.

## Baseline row audit

| class | needs | rows by do function |
| --- | ---: | --- |
| Amazon | 19 | 0: 9,13,18,23,29,33; 2: 14,19; 6: 8,17; 7: 10; 8: 12; 10: 22; 11: 24; 12: 26; 13: 30; 14: 34; 15: 28; 16: 32 |
| Sorceress | 18 | 0: 37,61,63,65; 17: 38; 18: 40,50,60; 19: 41; 20: 42; 21: 43; 23: 46,58; 24: 51; 25: 52; 26: 53; 29: 57; 144: 62 |
| Necromancer | 20 | 0: 69,79,89; 8: 67; 10: 93; 18: 68; 31: 70,80; 32: 73; 55: 74; 56: 75,85,94; 57: 90; 58: 95; 59: 86; 60: 78; 61: 81; 62: 88; 63: 83 |
| Paladin | 15 | 2: 111; 13: 106; 18: 117; 64: 96; 66: 102,118,119,123; 67: 107; 73: 112; 79: 116; 80: 121; 81: 114; 82: 124; 150: 97 |
| Barbarian | 29 | 0: 127,128,129,134,135,136,141,145,148,153; 2: 126,139,144,152; 9: 147; 68: 138,146,149,154,155; 69: 131; 70: 133; 71: 137; 72: 142; 74: 140; 75: 150; 76: 151; 77: 132; 78: 143 |
| Druid | 28 | 0: 224; 2: 239; 8: 243; 13: 248; 18: 235; 19: 230; 114: 221; 115: 222,231,241; 116: 223,228; 117: 225; 118: 240,245; 119: 226,227,236,237,246,247; 120: 232,233; 121: 238; 122: 242; 123: 244; 124: 249,250 |
| Assassin | 29 | 0: 252,263; 18: 258,267,278; 33: 253; 34: 254,265,280; 35: 259,269,274; 42: 255; 43: 256; 44: 257; 45: 261,262,271,272,276; 46: 260; 47: 264; 48: 266; 49: 268,279; 50: 270; 51: 273; 52: 275; 54: 277 |

## Current measured progress

| Class | Lifted | Same | Flagged | Needs |
| --- | ---: | ---: | ---: | ---: |
| Amazon | 23 | 23 | 0 | 7 |
| Sorceress | 21 | 21 | 0 | 9 |
| Necromancer | 21 | 21 | 0 | 9 |
| Paladin | 21 | 21 | 0 | 9 |
| Barbarian | 19 | 19 | 0 | 11 |
| Druid | 19 | 19 | 0 | 11 |
| Assassin | 19 | 19 | 0 | 11 |

Counts are from the seven deterministic CI install reports at the latest module, including generic parts merged from mk-blocks. "Same" remains bounded to the observed fixture metrics; flagged lifts are not parity claims.

### Remaining work ranked by measured unlock candidates

| Missing part / formula / fixture | Do | Skills | Classes |
| --- | ---: | ---: | --- |
| needs formula/fixture support: needs a generated sequence fixture | 13 | 3 | Amazon (1), Paladin (1), Druid (1) |
| needs part channel/start-state | 19 | 2 | Sorceress (1), Druid (1) |
| needs part persistent ground effect | 23 | 2 | Sorceress (2) |
| needs part summon/spawn or corpse control | 31 | 2 | Necromancer (2) |
| needs part weapon/melee variant | 120 | 2 | Druid (2) |
| needs part persistent ground effect | 124 | 2 | Druid (2) |
| needs part passive/stat or start-only behaviour | 0 | 1 | Druid (1) |
| needs formula/fixture support: the melee start function 35 is not lifted (only 6 and 32 are) | 2 | 1 | Paladin (1) |
| needs formula/fixture support: the melee start function 39 is not lifted (only 6 and 32 are) | 2 | 1 | Barbarian (1) |
| needs formula/fixture support: the melee start function 58 is not lifted (only 6 and 32 are) | 2 | 1 | Druid (1) |
| needs formula/fixture support: the melee start function 7 is not lifted (only 6 and 32 are) | 2 | 1 | Amazon (1) |
| needs formula/fixture support: the row adds `calc2` points of damage after the enhanced damage, which the melee hit cannot say yet | 2 | 1 | Barbarian (1) |
| needs formula/fixture support: the row's own state (`aurastate`) lasts to the unit's next mode change, which a timed state cannot say | 2 | 1 | Barbarian (1) |
| needs part weapon/melee variant | 9 | 1 | Barbarian (1) |
| needs part weapon/melee variant | 11 | 1 | Amazon (1) |
| needs part missile pattern/sequence | 12 | 1 | Amazon (1) |
| needs part weapon/melee variant | 14 | 1 | Amazon (1) |
| needs part summon/spawn or corpse control | 15 | 1 | Amazon (1) |
| needs part summon/spawn or corpse control | 16 | 1 | Amazon (1) |
| needs part target/area damage or debuff | 20 | 1 | Sorceress (1) |
| needs part target/area damage or debuff | 21 | 1 | Sorceress (1) |
| needs part timed buff/form or attached state | 25 | 1 | Sorceress (1) |
| needs part missile pattern/sequence | 26 | 1 | Sorceress (1) |
| needs part aura/pulse with damage or corpse effects | 29 | 1 | Sorceress (1) |
| needs part weapon/melee variant | 32 | 1 | Necromancer (1) |
| needs part target/area damage or debuff | 33 | 1 | Assassin (1) |
| needs part weapon/melee variant | 42 | 1 | Assassin (1) |
| needs part missile pattern/sequence | 43 | 1 | Assassin (1) |
| needs part summon/spawn or corpse control | 44 | 1 | Assassin (1) |
| needs part weapon/melee variant | 46 | 1 | Assassin (1) |
| needs part timed buff/form or attached state | 47 | 1 | Assassin (1) |
| needs part channel/start-state | 48 | 1 | Assassin (1) |
| needs part weapon/melee variant | 50 | 1 | Assassin (1) |
| needs part target/area damage or debuff | 51 | 1 | Assassin (1) |
| needs part teleport/leap/charge movement | 52 | 1 | Assassin (1) |
| needs part timed buff/form or attached state | 54 | 1 | Assassin (1) |
| needs part target/area damage or debuff | 55 | 1 | Necromancer (1) |
| needs part summon/spawn or corpse control | 57 | 1 | Necromancer (1) |
| needs part summon/spawn or corpse control | 58 | 1 | Necromancer (1) |
| needs part summon/spawn or corpse control | 60 | 1 | Necromancer (1) |
| needs part summon/spawn or corpse control | 62 | 1 | Necromancer (1) |
| needs part target/area damage or debuff | 63 | 1 | Necromancer (1) |
| needs part weapon/melee variant | 64 | 1 | Paladin (1) |
| needs formula/fixture support: the engine's Charge hands a target in reach to the normal attack mode and walks on the game's path code, which the fixture runs neither; the data copy is checked on its own | 67 | 1 | Paladin (1) |
| needs part weapon/melee variant | 70 | 1 | Barbarian (1) |
| needs part taunt/AI control | 71 | 1 | Barbarian (1) |
| needs part missile pattern/sequence | 73 | 1 | Paladin (1) |
| needs part weapon throw | 74 | 1 | Barbarian (1) |
| needs part corpse item/ward | 75 | 1 | Barbarian (1) |
| needs part whirlwind movement/weapon sequence | 76 | 1 | Barbarian (1) |
| needs part teleport/leap/charge movement | 77 | 1 | Barbarian (1) |
| needs part teleport/leap/charge movement | 78 | 1 | Barbarian (1) |
| needs part conversion | 79 | 1 | Paladin (1) |
| needs part missile pattern/sequence | 80 | 1 | Paladin (1) |
| needs part aura/pulse with damage or corpse effects | 82 | 1 | Paladin (1) |
| needs part missile pattern/sequence | 117 | 1 | Druid (1) |
| needs part weapon/melee variant | 121 | 1 | Druid (1) |
| needs part persistent ground effect | 123 | 1 | Druid (1) |
| needs part summon/spawn or corpse control | 144 | 1 | Sorceress (1) |
| needs formula/fixture support: the engine's Smite needs a shield (a player) or a monster caster; the fixture's caster is a bare-handed player | 150 | 1 | Paladin (1) |
