# Add-ons

A session runs one full mod, including any explicitly declared base chain.
An add-on attaches to that selected mod; it never changes the session identity.

```toml
# The full mod's mod.toml
kind = "full"
approved_addons = ["echo-hud"]
```

```toml
# echo-hud/mod.toml (also include id, name, version, author, description, api)
kind = "addon"
for = "echo-world"
screen_only = true
```

The player enables approved add-ons in `Documents/D2Gamma/mods/addons.toml`:

```toml
[enabled]
echo-world = ["echo-hud", "echo-labels"]
```

An absent file enables none. Lists are load order, after the selected full mod
and its bases. `requires` must already be present earlier. Approval does not
automatically enable anything. An unapproved or wrongly targeted add-on, two
unrelated full mods, duplicate ids, or a reversed dependency is a load error.
Folder and signed-pack installs use the same checks. Two enabled add-ons that
provide the same native path conflict: the message names both and the path;
disable one or ask their makers to combine them. Add-ons may override the base
mod, but may not silently replace each other.

The maker approval is the base manifest's declaration, not a claim an add-on
can make for itself. It currently names add-on ids, as the manifest contract
already specifies. Plain D2Gamma additionally requires the owner's separate
allow-list; a mod maker's approval does not grant that permission.

## Screen-only content

`screen_only = true` is enforced at load time and by check-mod, for folders
and signed packs. Allowed files are `files/ui/*.toml` panels and bindings,
`files/effects/*.toml` visual recipes, `files/patches/*.d2patch` containing
**only** `stradd`/`strset`, UI art under `files/data/global/ui/` in native
DC6 sidecar/pixel pairs, and the mod picture. Subfolders are allowed.
UI documents may neither declare server actions nor assign an action to a
button. Existing stats, states, quest flags and synced values may be read.
Effects refer to existing missile/overlay rows; they never alter those rows.

Rules, table patches, side tables, states, skills, acts, Lua, visual graphs,
network declarations/actions and unknown engine file kinds are refused.
The error names the forbidden file. Screen-only is checked for add-ons to
full mods too. A full mod's ordinary capabilities are unchanged.

## Plain D2Gamma

The owner controls `<native>/plain-addons.toml` (outside all add-on packs):

```toml
approved = []
```

The default and a missing file approve nothing. After owner review, put the add-on
id in this list and enable it in `mods/addons.toml` under `[enabled] d2gamma = [...]`.
Only `for = "d2gamma"`, `screen_only = true` add-ons can load there, including online.
They never install a rules host, alter save encoding, or contribute to the network
contract fingerprint. `--no-mod` bypasses add-ons for a safe start.
The Mods screen groups add-ons immediately below their target and shows enablement
and maker approval. Add-ons cannot be launched as full mods.

## Studio and packaging

Choose **New add-on**, name its id and maker, choose D2Gamma or an installed full
mod as target, and choose whether it is screen-only. A target need not allow derived
full mods. Screen-only projects start without `rules.toml` and use the Empty template.
Use Layout, Effects and string patches, then Check and Package with the native install
and installed-mods folder configured. Packaging runs the same checks on the exact
snapshot before signing it. A maker-approved mod add-on is checked with its target.
A plain-game add-on can be checked and packaged before owner review; its report
warns that activation still requires owner approval. Packs cannot grant approval.
