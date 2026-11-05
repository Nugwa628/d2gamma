# Random levels and seed previews

`files/levels.toml` version 2 adds a per-level `[level.generation]`. Version 1 files migrate to the same preset output without consuming additional random draws. Omitting generation means `kind = "preset"`.

```toml
version = 2
[[level]]
id = 1
size = [32, 32]
offset = [100, 100]
entry = [2, 2]
files = ["my-mod/base.ds1"]
tiles = ["my-mod/tiles.dt1"]
[level.generation]
kind = "maze"
cell = 3
floor = 1048577
wall = 2097153
orientation = 1
```

The source DS1 supplies dimensions and metadata. Maze and outdoor generation replace its grids before stamping presets, entry/warp markers and typed placements. `floor` and `wall` are DS1 cell values: low byte nonzero, main tile group in bits 20–25, sub tile in bits 8–15. Choose keys that exist in the named DT1 libraries. Hidden marker values cannot be brushes. Wall orientations are 1–9.

Maze uses a seeded depth-first traversal, with cell spacing 2–16; both dimensions must fit at least two cells. Outdoor uses `kind = "outdoor"` and `density = 25` instead of `cell`: the obstacle percentage is 0–80. Both open corridors from the entry to declared warps, objects and spawn regions. Stamped pieces and real DT1 subtile flags can still block movement; use Check before publishing.

The public game map seed and level ID determine each layout. Preview and gameplay share the generator and simulation RNG; visiting levels in a different order cannot change them. Preset levels retain their existing grid and random sequence.

Export up to 64 consecutive seeds without opening a window:

```sh
Mod-Studio seed-preview /path/to/mod /path/to/native /path/to/installed-mods 1 42 32 /path/to/output
```

The output contains realized native DS1 sidecars plus versioned `seeds.json` recording seeds and typed runtime units. Consecutive seeds wrap at the unsigned 32-bit limit. These files are inspection output; they do not replace your project's source map. Supply the converted install and installed-mod directory used by the game so inherited assets resolve identically. All examples refer to your own or generated tiles; no game assets are included.
