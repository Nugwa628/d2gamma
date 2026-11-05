# Authored levels

A full mod can provide `files/levels.toml`. This file opts levels into the mod map host; a game without it retains the native generators, random draws and map-data hash. The last mod layer with the file supplies the whole registry. Folder and signed-pack assets load through the same native source.

Version 1 defines preset maps. Each `[[level]]` names an existing `levels`/`leveldefs` row (1..255), stored DS1 dimensions, an offset in tiles, an entry, 1..6 alternative DS1 paths and 1..32 DT1 libraries. Add new table rows using table patches. Names, population columns and other table metadata still come from those rows. DS1 files remain `.ds1.toml`; DT1 files remain `.dt1.toml` plus `.dt1.d/*.png`, under `files/data/global/tiles/`. Registry paths are lowercase relative paths beneath that directory, including `.ds1`/`.dt1`, without the native sidecar suffix.

```toml
version = 1
[[level]]
id = 1
size = [32, 32]
offset = [100, 100]
entry = [2, 2]
files = ['my-world/home.ds1']
tiles = ['my-world/stone.dt1']

[[level.preset]]
path = 'my-world/courtyard.ds1'
at = [8, 8]

[[level.warp]]
slot = 0
to = 2
at = [28, 28]

[[level.object]]
class = 119
at = [5, 5]
mode = 0

[[level.spawn]]
monster = 0
rect = [12, 12, 8, 8]
count = 6
```

Define level 2 and its return warp as well. Slots are explicit and stable, 0..7, and use byte-sized warp classes allocated from the effective table's unused IDs. Act travel handles cross-act links. `acts.toml` controls ownership and towns; a completely authored act should declare its own town map. Its map definitions supply placement directly instead of the native act placer. Other acts retain the native placer. Authored levels in native-placed acts retain their declared offsets and choices.

All positions in the registry are tiles, relative to the level, excluding the final DS1 border. `size = [32,32]` needs DS1 decoded dimensions 33x33. Entries and warp markers in the registry replace matching markers inherited from the DS1 sources. A preset stamp copies nonzero floor/wall/orientation/shadow cells, objects and paths with the requested offset. Object classes are direct `objects` row IDs; spawn classes are direct `monstats` row IDs, bypassing DS1's act-specific preset-ID remapping. Runtime unit positions use the tile centre, `(5*x+2,5*y+2)`.

Spawn regions select their positions through the simulation RNG, independently for each level from the public game seed. Alternative preset selection and authored placements use the same realization function as seed previews. A level's prepared cache path is private to that level, so reusing the same input DS1 for several definitions does not share modified markers/units.

The loader rejects unknown fields/versions, unsafe paths, sizes outside 8..256, offsets outside movement/wire coordinates, duplicate level IDs or warp slots, out-of-bounds placements, missing assets, mismatched variants and invalid unit classes. Limits are 255 levels, 64 stamps, 256 objects, 64 spawn regions and 256 spawned monsters per level, with a two-million-cell registry budget including alternatives. DT1 collision data determines movement; art alone does not establish walkability.

Use only art you made or may distribute. Studio's starter tiles and the test fixtures are generated; no game assets are included in the repository or templates.

[Random rules and seed previews](random-levels.md) extend this registry with format version 2. Version 1 maps migrate to the same preset output.

[Map connectivity checks](map-checks.md) validate the graph and streamed movement collision before publication.
