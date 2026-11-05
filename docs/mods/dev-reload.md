# Dev reload for folder mods

Start `D2Gamma-Game play --mod-dir <folder> --native <converted-install>`
(the Mod Studio Play path). Enter a single-player game with a saved
character and leave the dev tools enabled. Installed mods selected with
`--mod` do not watch files. Online games never install the watcher.

The game polls the folder every 200 ms, including `mod.toml`, patches,
`files/rules.toml` and native assets. Content changes, creation, deletion
and editor file replacement count; `.git` and symlink targets are skipped.
Only `mod.toml` and `files/` are watched, so the mod's saves, logs and dev
slots cannot retrigger checks when source and user folders coincide.
It waits for 500 ms without another edit before checking. A background
worker reopens the manifest stack and strict rules file, recompiles the
effective tables with schema/reference/asset checks, loads the game data,
and decodes the mod's native assets. It discards a check if files changed
while it ran. No Rust compiler or pack signing step is needed.

The in-game dev overlay opens with either a problem naming the file or
an offer to reload. Fix a problem and save again to retry. The current
world keeps running with its existing data; failed checks never request
an exit or replace its rules. Checks report the first blocking problem,
as the normal loaders do. Indirect animation/map playability sweeps remain
the Check tool's responsibility.

Type `reload` in the dev console, or send `reload` through the local
`--control` channel. This checks again before saving `dev-slots/mod-reload`
beside the character save. The server writes the character, and the slot
records seed, difficulty, area, cell position and dev switches. After the
normal leave/save and window teardown, the slot character is restored and
the same executable is launched with `--save`, `--seed`, `--difficulty`,
`--dev-start` and the original `--mod-dir` and install/display options.
The private `--dev-window X,Y` argument restores the physical window
position. The parent waits and passes on the child's exit code.

This rebuilds and relaunches; the running world's data is never patched.
Monsters and ground items regenerate from the seed, as with dev `load`.
An in-memory character without a save cannot reload: the overlay explains
that a save is required. `--no-dev`, check runners without dev tools, and
the front end before entering a game do not watch. Large mods can take
longer than a second to check and start.

If changed data makes the snapshot character unreadable at startup, the
game opens a fresh unsaved character and explains the failure in the dev
overlay; the slot remains on disk. A failed area warp/placement uses the
existing dev-load error report and leaves the character in a usable game.
Keep the slot when removing item/skill/area records so you can repair the
mod and retry. Changes made after a successful check can still cause a
normal startup error in the child; this is a development workflow, not an
atomic published pack.

Changing the manifest's mod id requires a fresh Studio Play session: reload
does not carry a character into a different mod's identity or save folder.

Manual check: move a saved character to a non-town area, move the window,
edit a table patch, accept `reload`, and confirm the new value, character,
area/cell, difficulty and window position. Break a reference or WAV, save,
and confirm the overlay reports it while the current game remains usable.
