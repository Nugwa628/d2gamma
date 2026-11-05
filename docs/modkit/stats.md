# Named stats

A mod can define `files/stats.toml`. The last file in the resolved mod stack
wins, like states and skills files. No file keeps D2Gamma's stat rules.

```toml
version = 1

[[stat]]
name = "focus"
id = 40000

[[stat]]
name = "ward"
id = 40001

[[behavior]]
kind = "add"
source = "focus"
target = "armorclass"
percent = 200

[[behavior]]
kind = "convert"
source = "maxmana"
target = "ward"
percent = 25

[[behavior]]
kind = "percent_of"
source = "focus"
of = "maxmana"
target = "ward"
```

Use these names in `stat` and `bonus_stat` fields in states, skills and
triggers, for example `stat = "focus"`. Engine stat symbols and their aliases
are also accepted, without case sensitivity. Existing numeric ids still work.

Names contain 1–63 ASCII letters, digits or underscores and start with a
letter or underscore. Engine names cannot be replaced. A file allows at most
256 definitions and 512 behaviors. Explicit ids are unique in 32768–65534.
If omitted, ids are allocated by sorted name, skipping explicit ids. Assign
explicit ids before releasing a mod so later definitions cannot renumber saves.

`add` contributes source points times `percent / 100` to the target.
`convert` also removes that fraction from the source; outgoing conversions
from each source total at most 100%. `percent_of` treats source points as a
percentage of `of`, and contributes the result to target. Its optional
`percent` further scales the contribution (default 100). All percentages are
integers in 0–10000, subject to the conversion limit.

Behaviors read the native evaluated totals before any behavior, simultaneously.
They do not chain into each other or back into itemstatcost formulas. Order and
mutual references therefore cannot create recursive conversions. Targets can
be engine stats or custom stats. Values convert between engine fixed-point
units and custom whole points, truncate toward zero, and saturate at i32 limits.
Only layer zero on players and monsters receives transformations; equipment
contributes its raw stats, so equipping an item applies a behavior once.
Changes and removal of a contributing stat immediately refresh the targets.

Data mods save characters as `.d2m`, version 0x100. Character and item stat
ids occupy sixteen bits (65535 terminates a list); custom values are signed
32-bit integers with unsigned 16-bit layers. The checked container preserves
bounded, uniquely tagged mod sections and rejects corrupt or unknown versions.
D2Gamma retains its existing `.d2s` and nine-bit item/character stat fields.
Existing mod `.d2s` files are left in place; automatic conversion is deferred.

On the mod wire, item records use the same sixteen-bit ids. Version 1 message
0xB8 carries a unit type, GUID, stat id and signed authoritative total. The
client uses it for custom stats and behavior outputs without reapplying item
or state contributions. Totals include zero so expiry and removal clear the
client. The base wire refuses the message. Rust hosts opt in explicitly with
`ModRules::wide_stat_ids`; the built-in data host opts in automatically.
