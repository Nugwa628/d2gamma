# Package a mod

Run `d2-convert package-mod <folder> --base <converted-install>`.
The output is `<id>-<version>.d2pack` in the current directory; `--out`
chooses another path. Usage errors return 2, failed checks return 1, success
returns 0. The editable folder is never changed. Test folders can still be
played with `--mod-dir`; publish a pack before sharing it.

Publication checks the manifest, native file layout, asset decoding, patched
table compilation and reference checks against the player's converted install.
`d2_native::package::package_with_checks` takes a check callback for Studio
and the full `check-mod` runner. The callback receives the staged folder and
native root, and must return an error when its report fails. This avoids a
dependency from the native library to the game client. The default command
uses the native checks; scenario sweeps belong to the full runner.

The pack contains the manifest's explicit mod API version and a version-1
`files/d2gamma-pack-signatures.toml` record. The record carries the SHA-256
content hash and the author's Ed25519 signature. The hash covers the exact
manifest bytes and every payload path and file; only the record itself is
excluded. Packaging checks a snapshot and writes that same snapshot.
The same folder and author key produce the same pack bytes.

The author key is generated on first successful publication and kept as
PKCS#8 at `%USERPROFILE%\.d2gamma\author-key.pk8` on Windows, or
`$HOME/.d2gamma/author-key.pk8` elsewhere (Unix file permissions 0600).
It is shared across that user's mods. Back it up privately: losing it loses
the ability to publish under the same public key. Never put it in a mod
folder, a repository, or a download. An invalid existing key is an error;
publication never silently replaces one.

The record reserves optional `reviewed_by_d2gamma`, with the same signature
text format over the same content hash. A later review service can insert
this field and rebuild the container without invalidating the author signature.
Both signatures are verified when present. Integrity alone does not establish
who owns a key or whether D2Gamma approves it; the later official mod list
must pin the trusted review key. A missing review signature is allowed.

`D2Gamma-Game --mod <path.d2pack>` verifies the pack before activation.
Installed packs may be named `Documents/D2Gamma/mods/<id>.d2pack`.
The layer loader also verifies signed packs before reading their assets.
A kit pack missing its author record is refused with a publication message.
Legacy layer-only packs retain their existing loading behavior.

## Content defaults pending the owner's decision

`REFUSE_UNMODIFIED_BASE_FILES` and `MAX_BASE_OVERRIDE_PERCENT` in
`d2_native::package` define both limits in one place:

- Any file identical to a base native file in the install's `files.tsv` is
  refused, including a renamed copy.
- More than 10% overrides in any of tables, art, sounds or maps is refused.
  Exactly 10% is allowed. New files do not count as overrides. Converted
  sidecars/pixels count as one logical asset. Patch target tables count as
  overrides. Excel/string tables are tables; WAV is sounds; DS1/DT1 are
  maps; other native kinds are art.

These are publication limits against the player's complete converted install,
not checks against fixed retail hashes. The normal unmodded game path is
unchanged. No install files or signing keys are included with this tool.
