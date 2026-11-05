# Harder Fire (template)

A mod with one file: `files/rules.toml`. Players cannot stack as much
resistance, and Nightmare and Hell take more of it away.

The caps are shared by fire, cold, lightning and magic resistance, so this
makes all of them harder. There is no fire-only switch yet. Fire is simply the
one most players meet first.

To make it yours:

1. Copy this folder, or start it from Mod Studio's New mod wizard.
2. Change `id`, `name`, `author` and `description` in `mod.toml`.
3. Change the numbers. Caps are 0 to 100. Penalties are three numbers: Normal,
   Nightmare, Hell. `[0, 0, 0]` removes the penalty.
4. Classic (non-expansion) games use `classic_penalties` the same way.

Every key is explained in `docs/modkit/rules.md`. The full walk-through is
`docs/modkit/README.md`.
