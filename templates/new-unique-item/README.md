# New Unique Item (template)

A mod that adds one unique ring, **Emberheart**. It has no art. The ring uses
the normal ring picture.

Two files in `files/patches/`:

- `10-strings.d2patch` adds the name.
- `20-unique.d2patch` adds a row to the `uniqueitems` table and fills it in.

Read the comments in the second file. They explain every line.

To make it yours:

1. Copy this folder, or start it from Mod Studio's New mod wizard.
2. Change `id`, `name`, `author` and `description` in `mod.toml`.
3. Pick a new key instead of `emberheart` in both patch files. Use lowercase
   letters and numbers. The key is also the string key of the name.
4. Change the name text in `10-strings.d2patch`.
5. Change the numbers and the properties. Property names (`res-fire`,
   `mag%`, `mana` and so on) come from the `properties` table.

`add #402` is the number of rows `uniqueitems` has in the unmodded game. If
you add rows in another patch file that runs first, count them too. In Mod
Studio, use **Table editor**, pick `uniqueitems` and press **Add**. Studio
counts for you.

A picture of your own: ship `invfile`/`flippyfile` art as described in
`docs/modkit/assets.md`, then set the `invfile` cell.

The full walk-through is `docs/modkit/README.md`.
