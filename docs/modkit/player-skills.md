# Copy player skills as data

Load a mod project and its reference install in Mod Studio. Under **Skills →
Copy a player class**, expand the class's skill list to see the available
copies and the part each unsupported skill needs. **Copy Amazon** copies
every supported Amazon skill to editable parts and hands its table row to
the block slot. Save the project to write `files/skills.toml` and the Studio
table patch. Existing authored skills are kept when copying again.

Amazon is the first class pack. It selects `charclass = ama` from your own
tables, not a fixed list of row IDs. Missile and nova copies use the SK2
lifter already in the engine. Weapon variants, multi-shot/sequences,
summons, passives and special state effects stay Rust until their parts
exist; the list explains the missing mechanism for each row.

Sorceress (`sor`) copies the available missiles, novas and Meteor drops, plus Teleport
using the existing DF2 `teleport` part. The copy's notes warn that this part
uses placement without the Rust body's level/line teleport restrictions.
The comparison measures caster displacement as well as casts and missiles.
Defensive buffs, channels, chained or persistent effects and summons remain
listed as needing parts rather than hidden Rust-body calls.

The seeded install report is run on a Linux build machine with:

```sh
D2_PLAYER_CLASS=ama D2_PLAYER_REPORT=/tmp/amazon.md \
cargo nextest run -p d2-sim --run-ignored only \
  -E 'test(the_player_class_closeness_report_runs_on_install_rows)'
```

The table lists lifted, same, flagged and missing rows. “Same” describes
only the existing SK2 fixture's measured cast/state/missile metrics; its
one missile/state substitutes for the install's IDs, and weapon/damage
fidelity is limited. It does not certify the complete skill in gameplay.
The default game continues to run the Rust bodies.

## Necromancer

The Necromancer pack selects `charclass = nec`. Necromancer copies the supported missile, nova and curse families. Corpse explosions, skeletons/golems/revives, bone walls/prisons, poison melee and special AI control stay listed with their missing parts. Curse copies inherit the current lifter notes about AI effects; a fixture match does not prove those effects.

Run the class report with `D2_PLAYER_CLASS=nec` using the command above.

## Paladin

The Paladin pack selects `charclass = pal`. Paladin copies supported basic auras from the reference rows, including their state/stat formulas and available aura filters. Damage pulses, corpse redemption, Smite/melee variants, Charge, conversion and special missile sequences stay listed as needing their own parts.

Run the class report with `D2_PLAYER_CLASS=pal` using the command above.

## Barbarian

The Barbarian pack selects `charclass = bar`. Barbarian exposes all of its install rows and their missing mechanisms. Shout/taunt/control, weapon variants, item/corpse effects, leap/whirlwind/charge sequences and passives are not silently copied as a generic melee hit. A build with no supported row keeps the copy button disabled and shows the full needs-part list.

Run the class report with `D2_PLAYER_CLASS=bar` using the command above.

## Druid

The Druid pack selects `charclass = dru`. Druid copies supported plain missiles and Meteor-family drops from the reference install. Forms, summons, persistent storms/ground effects and special melee sequences stay listed as needing parts. The pack does not substitute a Rust engine-body call for those behaviours.

Run the class report with `D2_PLAYER_CLASS=dru` using the command above.

## Assassin

The Assassin pack selects `charclass = ass`. Assassin copies any supported plain missiles from the reference install. Charge-up/finisher, kick/claw, trap/shadow spawn, channel, cloak/control and attached-state behaviours stay listed as needing parts. These remain separate from mk-blocks vocabulary work.

Run the class report with `D2_PLAYER_CLASS=ass` using the command above.
