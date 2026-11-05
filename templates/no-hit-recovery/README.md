# No Hit Recovery (template)

A mod with one file: `files/rules.toml`. Being hit no longer interrupts you
or the monsters.

To make it yours:

1. Copy this folder, or start it from Mod Studio's New mod wizard.
2. Change `id`, `name`, `author` and `description` in `mod.toml`.
3. Want monsters to keep flinching? Delete the `[monsters.hit_recovery]` block.
4. Want a middle way? Replace `enabled = false` with
   `threshold_denominator = 12`. A hit then only interrupts when it takes at
   least one twelfth of max life.

Every key is explained in `docs/modkit/rules.md`. The full walk-through is
`docs/modkit/README.md`.
