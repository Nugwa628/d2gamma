# Data packs: a mod layer in one file

A mod game ships its converted layer (`mod.toml` and `files/`: art, sound, UI, patches, side tables, strings) as
**one data pack file** next to its executable, instead of a folder of tens of thousands of files. The engine reads a
pack layer exactly like a folder layer: the same paths, the same bytes, the same mod checks. The player's own base
layer (`base/`, converted from their install) is never packed.

Code: `crates/d2-native/src/pack.rs` (format, writer, reader, verify) and `crates/d2-native/src/layers.rs` (pack
layers in the stack).

## Naming a pack layer

A layer list (`mods/order.toml`, `Layers::List`, `--mod`) entry can be:

| Entry | Layer |
|---|---|
| `my-mod` | the folder `mods/my-mod/`, or else the pack `mods/my-mod.d2pack` (both present is refused) |
| `my-mod.d2pack` | the pack `mods/my-mod.d2pack` |
| `/games/my-mod/my-mod.d2pack` | that pack, wherever it is (a game passes the pack next to its exe) |

The layer's name is the pack's file name without `.d2pack`. A mod enabled twice (as a folder and as a pack) is
refused. On the command line, `--mod some/dir/my-mod.d2pack` is read from that path; a bare `my-mod.d2pack` is looked
up under `mods/`.

```rust
let pack = exe_dir.join("my-mod.d2pack").to_string_lossy().into_owned();
GameData::open(&GameSource::Native(native_dir, Layers::List(vec![pack])))
```

The animation bake reads a pack the same way:
`d2-client bake-anims --native <native dir> --mod /games/my-mod/my-mod.d2pack --out <dir>`.

## Building and checking a pack

```sh
d2-convert pack --layer <mods/my-mod folder> --out my-mod.d2pack [--threads N]
d2-convert verify-pack --pack my-mod.d2pack [--against <mods/my-mod folder>]
```

`pack` writes `<out>.part` and renames it when complete; the same folder always gives the same bytes. `verify-pack`
reads and checks every file, and with `--against` compares paths and bytes with the folder. Exit codes: 0 pass,
1 failures, 2 bad usage.

## Format (version 1)

All integers little-endian.

```
header   magic "D2GPACK\x1a" (8), format version u32 (= 1), reserved u32 (= 0)
data     every stored file, back to back, in index order
index    count u32, then per file:
           path length u16, path (UTF-8, `/`-joined, relative to the layer folder),
           offset u64, stored length u64, raw length u64,
           method u8 (0 stored, 1 zstd), CRC-32 of the raw bytes u32
trailer  index offset u64, index length u64, SHA-256 of the index (32), magic "D2GPKEND" (8)
```

Choices:
- **Per-file zstd** (pure-Rust `ruzstd`, level "fastest", already in the dependency tree through Bevy): any file is
  one positioned read and one decode. A file is stored as is when zstd saves less than 1/32 of it (most PNGs), and
  every compressed file is decoded back and compared when the pack is built.
- **Positioned reads, not a memory map**: the workspace forbids `unsafe`, which mapping a file needs. One open handle
  serves every thread without a lock (`pread` / `seek_read`).
- **Index at the end** so the writer makes one pass. Paths are sorted and unique: a lookup is a binary search and a
  folder is one contiguous range.
- **Checks**: opening a pack checks the header, the trailer, the index's SHA-256 and every entry (path, order,
  bounds, method), so a cut-off or damaged pack is refused at start with the pack's path and the reason. Every read
  checks the file's CRC-32, so a damaged file is refused with its path.
- Not reused: the MPQ archive reader (`d2-formats`) is the player's install format, with hashed names and no
  directory listing, and the baked animation packs (`*.d2frames`) hold one animation each.
