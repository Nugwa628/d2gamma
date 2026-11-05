# Mod assets: strings, pictures, sounds and maps

A mod layer is `mods/<layer>/mod.toml` plus `mods/<layer>/files/` under
the player's converted native root. A `.d2pack` contains that same
`mod.toml` and `files/` tree. See [the manifest](../mods/mod-format.md)
and [data packs](../mods/data-pack.md). Layer selection belongs to the
game host; this document describes the asset contract once it selects a stack.

All paths below are relative to the mod's **`files/`** directory. Use
lowercase names and `/` separators on disk and in packs. Table values may
use backslashes and mixed case: the engine folds asset requests. A file
does not need an entry in the base `files.tsv` to be added by a mod.

The stack reads the player's `base/` first, then selected layers in order;
the last layer with an asset's primary file wins. It reads every companion
file from that same layer. Always ship the whole PNG/sidecar pair, or the
whole tileset, when replacing one. A missing or invalid companion is an
error, never permission to fall back to base pixels. Folders and packs
use exactly the same readers. File errors identify the asset and its layer.

## Strings

Write `patches/10-strings.d2patch` (UTF-8), for example:

```text
d2patch 1
stradd my_item_name [A new item]
strset my_item_name [A renamed item]
```

`strset` can change any existing key, including a base key; its numeric id
is preserved. `stradd` requires a new key; ids start at 30,000 and must stay
below 65,536. Duplicate additions, missing keys on `strset`, and exhausted
ids report patch file/line errors. Bracketed tokens support `\xHH` byte
escapes for newlines, colour codes, brackets and other special characters.
String edits run before tables compile, so an item's `namestr` can name a
new key in the same or a later layer. Edits currently apply to the selected
language's loaded strings; per-language patch routing is not implemented.

Do not ship `.tbl`, `.tbl.tsv` or `.tbl.toml` string table copies. String
changes use patches over the player's three base string tables. Font `.tbl`
metrics are a separate asset kind, described below.

## Item and other pictures

For `invfile = my_icon` in an item table, supply:

```text
data/global/items/my_icon.dc6.toml
data/global/items/my_icon.dc6.png
```

`flippyfile` (ground item art), `uniqueinvfile`, `setinvfile`, the unique
and set rows' own `invfile`, and `itemtypes.invgfx1` through `invgfx6` use
the same folder and implied `.dc6` suffix. Patch the item row as described
in [tables](tables.md). Adding files alone does not create an item row.

DC6 UI/item art and DCC animation art both use `P.toml` plus `P.png`,
where `P` includes `.dc6` or `.dcc`. PNGs must be **indexed colour type 3,
8-bit, noninterlaced**, with a PLTE palette. Pixels are indices, with
index 0 transparent. RGB/RGBA exports are rejected. The PNG palette is
for editor display; the game applies its act palette, lighting and colour
maps. A sprite need not bundle an act palette: the installed base supplies
one unless a mod overrides it.

The TOML starts with `native = "dc6"` (or `"dcc"`) and
`native_version = 1`. It describes directions, frames, sizes, offsets and
rectangles. Frames run left to right, directions on successive rows;
long rows wrap at 65,536 pixels. PNG dimensions must exactly match that
layout. Wrong kind/version, rectangles, sizes and PNG format are errors.
Use a converted sidecar as a template and edit its frame sizes together
with its pixels. Native readers/writers are specified in
`crates/d2-native/src/sheet/`; they preserve animation flags and offsets.
Zero-pixel sprites currently cannot be shipped without a companion PNG
through a mod layer's strict pair check.

Missile `CelFile` resolves to `data/global/missiles/<name>.dcc`, then
`.dc6`; overlay `Filename` resolves to `data/global/overlays/<name>.dcc`,
then `.dc6`.
Player/monster component files keep their canonical paths under
`data/global/chars/` and `data/global/monsters/`. COF layouts, animation
data and the corresponding component art must agree.

Other native art files are also replaceable at their canonical paths:

- Act palettes: `data/global/palette/<act>/pal.dat.pal`, JASC-PAL text
  with `JASC-PAL`, `0100`, `256`, then exactly 256 `R G B` lines (0–255).
  A short palette reports the missing colour. It can inherit from base.
- Palette transforms: `<path>.pl2.toml` plus `<path>.pl2.png`; each
  transform is a 256-index row, with metadata and row counts in TOML.
- Item colour maps: `data/global/items/palette/<name>.dat`, raw index
  maps, 21 rows of 256 bytes. Monster shifts:
  `data/global/monsters/<token>/cof/palshift.dat` or
  `data/global/monsters/randtransforms.dat`, maps of 256 bytes.
- Fonts: `data/local/font/<language>/<font>.tbl.toml` for glyph metrics,
  plus the font's `.dc6.toml`/`.dc6.png` pictures. Metrics alone do not
  add glyph pictures. The font TOML has `native = "font"`, version 1,
  and `[[glyph]]` tables.
- `data/global/animdata.d2.tsv`: animation records in the converter's TSV
  format; `data/local/color.txt`: copied switch text.
- Expansion field: `<path>/expfield.d2.toml` and `.png`.

These are native representations, not raw DC6/DCC/COF/PL2/TBL binaries.
Supplying a raw asset reports the primary converted filename to supply.
There is no automatic RGB quantizer, animation authoring tool or font
importer here; those belong to the later Studio importer work.

## Sounds and music

Replace a sound by shipping its existing canonical `.wav` path, or add a
new WAV and patch a `sounds` row's `FileName`. Paths follow the effective
row number through `d2_data::sounds::sound_path`: rows 2934–4656 use
`data/local/sfx/`, rows 4657–4698 use `data/global/music/`, and other rows
use `data/global/sfx/`. Append `FileName` to that folder. Keep `.wav` in
the table filename. Sound ids referenced by other tables must also exist.

Native WAV is RIFF/WAVE with exactly one 16-byte `fmt ` chunk then one
`data` chunk: PCM tag 1, signed little-endian 16-bit, 22,050 Hz, mono or
stereo. `block_align` is channels × 2, `byte_rate` is rate × block_align,
and samples must form whole channel frames. Convert/resample/export in an
audio editor before packaging; extra metadata chunks and compressed
audio are rejected. An invalid mod sound names the file and required
format instead of reaching playback and silently disappearing. Base
conversion remains lossless and retains its existing sound behaviour.

## Maps and presets

Replace `data/global/tiles/<area>/<piece>.ds1.toml` to replace that map
piece, or add a new path and patch `lvlprest.File1`…`File6` or `lvlsub.File`.
These table values are relative to `data/global/tiles/` and include `.ds1`.
`lvlprest`'s `0` filename means no piece. `levels` chooses the area's
preset/type; changing a piece does not add an act or a difficulty.

DS1 TOML starts with `native = "ds1"`, `native_version = 1`, `width`,
`height`, `act`, and the remaining map fields. `[[layer]]` grids have a
kind, index and `rows` array. Each row contains exactly `width` cells of
eight lowercase hex digits; there must be exactly `height` rows. Dimensions
are decoded cell counts. Objects, paths and groups are optional arrays;
the shadow layer is required. Invalid rows, missing layers and inconsistent
wall/orientation counts name the problem. Header dimensions never cause
an allocation before the actual rows are validated.

Tilesets use `data/global/tiles/<area>/<tiles>.dt1.toml` and
`<tiles>.dt1.d/0.png`, `1.png`, etc. Ship all tile images named by the
sidecar, even when only one changes. Indexed PNG rules are the same as
sprites. The sidecar stores tile ids, collision flags and block geometry;
its `layout` is `assembled` or `blocks`. `lvltypes.File 1`…`File 32`
values are relative to `data/global/tiles/`, including `.dt1`. DS1 editor
file lists can also name tilesets; the renderer trims their install prefix
to `data/` and translates `.tg1` to `.dt1`. Matching tile ids, collision
and map playability still need a map sweep/playtest; a valid TOML file
alone cannot prove the piece is playable. For opt-in level definitions, placements and generated map assets, see [authored levels](levels.md).

## Checks and preparation

Native table loading checks every schema `strkey` introduced or changed
by the patch stack, plus explicit file columns listed above for items,
missiles, overlays, sounds, `lvlprest`, `lvlsub` and `lvltypes`. Files must
exist and decode through the effective stack. Errors include table, row,
column, value and path. Filenames exceeding the compiled field's byte limit
are rejected before they can be silently truncated. References can target
later layers. Unchanged base
cells, empty optional cells and disabled rows preserve existing behaviour.
Token-composed unit animations, palette dependencies, glyph frame bounds,
DS1 tile ids and other indirect combinations require the appropriate
animation/map/content sweeps; this is not a replacement for K9.

The player's install is converted once with `d2-convert convert`. Mods
ship prepared native PNG/TOML/TSV/WAV files in their folder or pack. A pack
compresses those files; opening it decodes them and never imports or
generates replacement assets. Any quantization, resampling or generated
animation frames belongs at authoring/build time or in an explicit
installation step **before Play**. Ship the baked results; **never generate
at runtime**. See [animation bake](../mods/animation-generation.md) for
`bake-anims`, cache identities and output layout. This module does not
change the game's bake selection or introduce a runtime generator.

Mods may ship modified assets and individual base files, **never enough
base files to play the base game without an install**. The base game always
comes from the player's own install. Table/string copies still use patches.
Do not commit or bundle extracted game/Median files, even for tests; the
asset tests construct tiny synthetic files in temporary folders. Content
overlap thresholds and packager enforcement belong to K10, not the layer
reader. This rule does not assign a new numeric threshold.
