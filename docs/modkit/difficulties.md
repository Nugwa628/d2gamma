# Difficulties as data

A data mod may supply `files/difficulties.toml`:

```toml
version = 1
names = ["Normal", "Nightmare", "Hell", "Eclipse"]
```

Names are unique (case insensitive), nonempty, trimmed text, at most 63 characters.
There must be 1–255 entries. IDs are zero-based row indexes: Eclipse is difficulty 3.
Append rows to `difficultylevels` using the table patch tools. The game rejects a
list longer than that table, or an out-of-range selected ID. A later mod layer
replaces the entire names file. Without the file the game keeps exactly three
difficulties, the existing progression unlock rules, and the existing menu.

Every configured difficulty is selectable from character select. Lists of more
than six entries have Previous/Next buttons, Page Up/Page Down and wheel paging.
`--difficulty` accepts configured names or numeric IDs while a data mod is active.
The initial new-character game starts on the first difficulty.

The chosen difficulty's `difficultylevels` row supplies penalties, skill bonuses
and other row values. Tables with three native columns (monster stats, level
sizes, treasure classes, palettes) use their Hell column beyond index 2; these
columns do not change the game's selected difficulty ID.

`.d2m` saves reserve the versioned `DPRG` section for independent quests, NPC
first-talk/intro bits, waypoints and last town for every difficulty. Older mod
saves without it seed the first three records from their base fields; added
records start fresh. The base `.d2s` fields and format still contain three
records. Legacy status/title progression remains limited to the first three;
custom difficulty progress is held in its own record. Custom unlock conditions
and quest blocks are future modules.

Do not renumber existing difficulty names/rows once saves are distributed.
Removing a difficulty hides it; retained save progress remains available if it
is restored. The maximum count is bounded by the existing one-byte game wire.
