# Required files

D2Gamma needs the player's own, legally owned Diablo II: Lord of Destruction
install. Nothing from it is ever committed to git. For local
development, put a copy in `game/` (gitignored) or point `D2_GAME_DIR` at
it; tests that need it are `#[ignore]` and read `D2_GAME_DIR`.

## Data archives

- `d2data.mpq`: core classic data
- `d2exp.mpq`: Lord of Destruction data
- `d2char.mpq`: character graphics
- `d2sfx.mpq`, `d2speech.mpq`, `d2xtalk.mpq`: sound and speech
- `d2music.mpq`, `d2xmusic.mpq`: music (optional: without them the game
  plays silent music; an install that needs its CD keeps them on the discs)
- `d2video.mpq`, `d2xvideo.mpq`: not used; D2Gamma has no cinematics
- `patch_d2.mpq`: patch data

These total about 2 GB and stay on the player's machine. D2Gamma reads them
at run time (once converted into its native folder by `d2-convert`); `d2-verify` checks that they are present (it confirms the
player has the game, it never compares against fixed hashes).

## Game versions

The game reads its rules from the player's own tables, so the version of
the install is the version of the rules. Nothing checks the version: any
folder with the archives starts.

- **1.14d**: the recommended install; the game is built and tested on it.
- **1.13c / 1.13d**: play the same, single player and online. Older base
  installs keep the same base archives, and their `patch_d2.mpq` holds
  the same files as 1.14d's, packed differently. `d2sfx.mpq` and
  `d2char.mpq` can differ between installs in one 72-byte entry each
  (`curindx.wav`, `amblxbow.cof`) that holds per-install data, not a
  valid sound or animation. To check an install:
  `mpq-tool diff-installs <1.13 folder> <1.14d folder>` lists every game
  file whose copy differs, and for each archive that differs counts its
  entries that differ, named or not.
- **1.12 and older**: not supported. Their `patch_d2.mpq` predates 1.13
  (no respec token or essences, older skills, recipes and drops), so single
  player would play those older rules, and online the client would read
  items, tooltips and maps with other tables than the server. Update to
  1.14d (Blizzard's current Diablo II installer).

Online, the realm compares the game servers' data with its own; players'
installs are not part of that check.

## Saves

Single-player characters are `.d2s` files in the player's save folder
(`Documents\D2Gamma\saves`); other Diablo II `.d2s` saves load with
`--save` (`tools/d2s-tool` edits and checks them). Online characters live
on the main server in D2Gamma's own versioned format (`.d2rc`) and never
on the player's PC.
