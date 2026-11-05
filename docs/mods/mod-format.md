# Mod format (`d2-modkit`)

The mod kit's file format, as a pure library crate (`crates/d2-modkit`) that D2Gamma (loading), Mod Studio
(editing) and the packager share. It has no game, renderer or Bevy code. The layer stack (`d2_native::layers`)
reads the same `mod.toml` through it, so there is one parser.

## Manifest: `mod.toml`

| Key | Type | Notes |
|---|---|---|
| `id` | text | Required. Lowercase slug: `a-z`, `0-9`, single hyphens; starts with a letter; 3 to 40 characters. `d2gamma`, `base`, `default` and `none` are reserved. |
| `name` | text | Required. Shown to players; also the window title. |
| `version` | text | Required. `MAJOR.MINOR.PATCH`, optional `-pre`. |
| `author` | text | Required. |
| `description` | text | Required (may be empty). |
| `api` | whole number | Required, 1 or higher. The modding API version the mod targets. |
| `kind` | text | Required. `"full"` or `"addon"`. |
| `base` | table | Full mods: `id` and `version` (a range such as `">=1.0.0, <2.0.0"` or `"^1.2.0"`) of the mod this one builds on. |
| `allow_derived` | bool | Full mods: others may build on this mod. Default false. |
| `accepts_d2gamma_characters` | bool | Full mods only. Default false. Offers an explicit copy from a plain D2Gamma character into this mod's save folder; the source stays unchanged. |
| `approved_addons` | list of ids | Full mods: add-ons the maker approves. |
| `for` | text | Add-ons, required: the mod id the add-on attaches to, or `"d2gamma"`. |
| `screen_only` | bool | Add-ons: changes only what is on screen. An add-on for `d2gamma` must set it. |
| `native_format_version` | whole number | Optional. The native file layout the mod follows; left out it means this build's. |
| `requires` | list of ids | Optional. Mods that must be loaded before this one. |

Parsing is strict. Unknown keys, wrong types and keys that do not fit the mod's kind are errors, all
reported together, each naming its field (``mod.toml: `version`: ...``).

**Old `mod.toml` files keep loading.** A file with only `name`, `version`, `native_format_version` and
`requires` (and no `api`) is a layer-only mod from before the kit. `parse_mod_toml` reads it as before
(`native_format_version` required, `version` free text, unknown keys an error), and
`d2_native::layers::ModInfo::from_toml` is a thin view over it: the same fields for both kinds, plus
`ModInfo::kit` for a kit manifest. Any key the old files never had (`api`, `id`, `kind`, ...) makes the file a
kit manifest, held to every rule above. Opening a kit mod in the layer stack also runs the API version check
and refuses a mod that is too old or too new with the messages below.

Two cross-mod rules are functions on `Manifest`, for the loader and the packager to call:
`check_builds_on` (the base allows derived mods and its version is in range) and `check_addon_for`
(the target's maker approved the add-on, or it is a screen-only add-on on plain D2Gamma).

## Folder layout (a mod being edited)

The folder is the layer folder the layer stack reads, so editing and loading use the same files:

```text
my-mod/
  mod.toml
  files/
    rules.toml        global rules (a placeholder until the rules module lands)
    patches/          table patches
    tables/           the mod's own side tables
    <native paths>    art, sounds, maps, fonts at their native paths
  README.md  LICENSE  kept with the mod, never packed
```

`ModFolder::scan` checks the shape (nothing else at the top level, no links, dot-files and OS leftovers
skipped) and that every path is safe on every platform (no `..`, no `\ : * ? " < > |`, no Windows-reserved
names, no two paths that differ only by letter case). What goes under `files/` is checked by the layer stack
when the mod is opened (no copies of game tables, sprite sidecars complete, ...). `layout::init_folder` starts
a new mod.

## Packs, content hash, signatures

The container is the data pack (`d2_native::pack`, `.d2pack`): one file holding `mod.toml` and `files/`
exactly. The kit does not write containers. It sees a pack through `pack::PackSource` (a list of paths and
the bytes of each); a `ModFolder`, an in-memory `MemoryPack` and `d2_native::pack::Pack` implement it, so
`read_manifest` reads a pack's manifest without unpacking the rest and `content_hash` gives the same hash for
a pack and the folder it was built from.

On top of that trait, independent of the container:

- `read_manifest(src)` reads `mod.toml` and nothing else.
- `check_pack_paths` accepts `mod.toml` and `files/...` only, every path safe.
- `content_hash(src)` is one SHA-256 over every path and every file's bytes in path order. A folder and a
  pack of that folder give the same hash. `verify_content` compares it with the recorded one.
- `sign::PackSignature` is an Ed25519 signature over a fixed prefix plus the content hash, with the public key
  beside it and a text form (`ed25519:<key hex>:<signature hex>`). `sign::verify_pack` checks the hash first,
  then the signature, and reports the signer (`Verified::signed_by` tests it against trusted keys). Which keys
  are trusted, key storage and where the signature travels with a pack are the packager's job.

Published kit packs use the versioned signature record and author key storage
described in [Packaging](../modkit/packaging.md). Use `publish::verify` for
these packs: it excludes the signature record itself from the content hash.
The lower-level `content_hash` still hashes every file it is given.

## Modding API versions

`api::CURRENT_API` is the version this build implements. A superseded version keeps working until
12 months after the next version's release date (`SUPPORT_MONTHS`). `check_api(mod_api, today)`
returns `Ok`, `NeedsUpdate(message)` (ask the author for an update) or `TooNew(message)` (update
D2Gamma). `removal_warning` gives the "stops working after <date>" notice for mods still inside
their window. Adding an API version means adding one line to `API_RELEASES` and bumping
`CURRENT_API`.
