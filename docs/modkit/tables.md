# Mod tables

Mods ship `files/patches/*.d2patch`, never full excel tables. Patches apply in
layer order over the player's installed data. Mod Studio can edit rendered
sheets and call `diff_tables` to write a patch.

## Disable a row

```text
d2patch 1
table itemtypes
remove mytype
# An index plus its current label is also supported:
# disable #123 mytype
```

`remove` and `disable` are synonyms. The slot and key remain reserved forever;
subsequent ids are unchanged and `add` still appends at the row count. Disabled
rows cannot be edited, removed again or used as templates. They keep their raw
cells and a reserved `__d2_disabled` metadata column, so render/reload and diffs
preserve the operation. Other patches cannot create or write that column.
The compiler reserves own keys and identity labels needed by hand-built linkers;
it clears gameplay fields. This prevents a removed treasure class from shifting
later classes. The format remains `d2patch 1`: older readers fail loudly with
an unknown-statement error rather than silently ignoring removal.

A disable is accepted only when no effective declared table reference points
at the row. Remove its users or clear those reference cells in the same patch
stack. Validation runs after the entire stack, so layer order does not force
a temporary missing reference to fail. Reports include every broken reference's
table, row id and key, column, value and target linker.

## Validation API

- `d2_data::patch::references(data)` checks effective declared lookup columns.
  It includes compiler-only key tables, shared item keys, raw item-code fields,
  numeric area/warp/sound ids, generated treasure classes and references in the
  five table-specific callbacks. Empty optional cells and disabled source rows
  are ignored.
- `check_references(base, patched)` preserves the engine's accepted base tokens
  for optional absent content. An identical unresolved base cell stays accepted;
  a new bad value or a newly disabled target is an error. This is used by native
  table loading and `compile_patched`. It does not excuse a new bad value just
  because that cell already held a different unresolved token.
- Callback validation uses compiler tokenization for cube inputs/outputs,
  monster placements, skill modes and component choices. Cube output numeric
  prefix/suffix ids are checked too. The schema marks these columns as
  `expression` and lists all possible target tables. Formulas remain compiler-validated; `links::validate` checks stored lookup
  indices in compiled tables. No new reference failure is converted silently
  into the compiler's legal missing-reference sentinel.

Id limits are checked before compiling the complete table set and before
appending past a mod table's capacity. Narrow lookup fields also refuse an id
that would truncate or collide with the no-reference sentinel. Errors name the
table/linker and limit. Wider state/stat ids do not change the item save format;
item stat ids still have the existing item-layout restrictions.

## Schema export

```sh
cargo run -p data-tool --bin data-tool -- schema /path/to/schema
```

This writes one `<table>.json` per game table, schema version 1. Columns carry
engine type vocabulary, compiler list, length, offset, linker/callback metadata
and referenced table filenames. `reference_encoding`, `target_column` and
`zero_is_none` describe code/numeric ids and their empty sentinel. A table can have multiple compiler lists;
columns are identified by their list and occurrence, not just their display name.
`max_rows_mod` is the engine's `DataRules::Mod` limit (null means no table-count
limit of that kind). Individual reference fields may have narrower capacities.
The source is `schema::schema()` plus the engine's runtime sound definitions;
no separate editor schema is maintained by hand.

`modkit_tables::schema_export_is_in_sync` compares each generated document's
SHA-256 to `tests/schema-golden.sha256`. To accept an intentional engine schema
change, run that test with `UPDATE_MODKIT_SCHEMA=1` and review the changed hashes.
The golden contains engine metadata hashes only, never installed game data.

## Table inventory

All 86 tables below are patchable. `sounds` is compiled as a key lookup
for other tables and served as text for its runtime fields; `soundenviron`
is served as text. All other entries are compiled using the engine load lists.
The exported files distinguish these with `served_as_text`.

`arena`, `armor`, `armtype`, `automagic`, `automap`, `belts`, `bodylocs`, `books`, `charstats`, `chartemplate`, `colors`, `compcode`, `composit`, `cubemain`, `difficultylevels`, `elemtypes`, `events`, `experience`, `gamble`, `gems`, `hiredesc`, `hireling`, `hitclass`, `inventory`, `itemratio`, `itemstatcost`, `itemtypes`, `levels`, `lowqualityitems`, `lvlmaze`, `lvlprest`, `lvlsub`, `lvltypes`, `lvlwarp`, `magicprefix`, `magicsuffix`, `misc`, `misscalc`, `missiles`, `monai`, `monequip`, `monitempercent`, `monlvl`, `monmode`, `monplace`, `monpreset`, `monprop`, `monseq`, `monsounds`, `monstats`, `monstats2`, `montype`, `monumod`, `npc`, `objects`, `objgroup`, `objmode`, `objtype`, `overlay`, `pettype`, `playerclass`, `plrmode`, `plrtype`, `properties`, `qualityitems`, `rareprefix`, `raresuffix`, `runes`, `setitems`, `sets`, `shrines`, `skillcalc`, `skilldesc`, `skills`, `soundenviron`, `sounds`, `states`, `storepage`, `superuniques`, `treasureclassex`, `uniqueappellation`, `uniqueitems`, `uniqueprefix`, `uniquesuffix`, `uniquetitle`, `weapons`.
