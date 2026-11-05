# Make a D2Gamma mod

This guide takes you from nothing to a mod you can hand to a friend. You do
not need to write code. A mod is a folder of readable files. Mod Studio
helps you edit them. D2Gamma plays them.

You will:

1. start a mod,
2. change the rules,
3. change items and other tables,
4. play it,
5. check it,
6. package and sign it.

Every step has a small example. Finished example mods live in
[`templates/`](../../templates): [no hit recovery](../../templates/no-hit-recovery),
[a new unique item](../../templates/new-unique-item),
[harder fire](../../templates/harder-fire), and the graph-only
[Visual Echo](visual-echo.md). Open them next to this guide.

## What a mod can change today

- **Global rules** in `rules.toml`: hit recovery, block, knockback, stun,
  freeze, resistance caps and penalties, experience, drops, death
  penalties, animation speeds. See [rules](rules.md).
- **Tables**: items, uniques, skills, monsters, cube recipes, treasure
  classes and many more (86 in all). You write *patches*, not whole tables.
  See [tables](tables.md).
- **Named stats** and add, convert and percentage behaviors. See [stats](stats.md).
- **States**: buffs and debuffs with a time, stacks and an icon. See
  [states](states.md).
- **Monster AI**: a behaviour graph of states, transitions and actions for
  any monster, in `ai.toml`. See [monster AI](ai.md).
- **Strings, pictures, sounds and maps.** See [assets](assets.md) and [authored levels](levels.md).
- **Look**: window icon, mod picture, intro screen and music. See
  [playing a mod](../mods/mod-kit-loading.md).

- **Quests** in `files/quests.toml`: objectives (kill, enter, talk, cast, reach a stat),
  rewards and a quest log text, kept in the character's mod save. See [quests](quests.md).
- **Server scripts** in `files/scripts/*.luau`, event callbacks and call-script
  blocks, edited in Studio's Scripts pane. See [scripts](scripts.md).

- **Visual scripts** in `files/scripts/*.graph.toml`: wire M2 events, typed
  operators and API calls in Studio. Start with [Visual Echo](visual-echo.md)
  and the [graph guide](visual-graphs.md).
- **Panels, effects and synced UI values**, with server-checked buttons. See
  [UI](ui.md), [effects](effects.md) and [network values/actions](network.md).

Mod distribution and shared online accounts come later.

## What you need

- **D2Gamma** and your own copy of Diablo II. A mod never contains the
  base game. Players need their own install.
- **Mod Studio** (`Mod-Studio.exe`). It is a separate program. Players do
  not need it.
- **The converted install.** D2Gamma reads a converted copy of your
  Diablo II files. Studio and the check need it. The usual place is
  `%APPDATA%\d2gamma\native`. If you have none, make it once:
  `d2-convert convert --install "C:\path\to\Diablo II" --out "<folder>"`.
- Any plain text editor, if you want to edit files by hand. Notepad works.

In Mod Studio open **Tools & install paths**. Set the game program, the
`d2-convert` program and the converted install. Studio remembers them.

## Part 1. Start a mod

1. Open Mod Studio.
2. Choose **New mod**.
3. Fill in the name, your name and a short description. Type an `id`. It
   is the mod's short name, like `ember-age`. It uses lowercase letters,
   numbers and single hyphens. Pick it once. Changing it later makes a new
   mod, with new saves.
4. Choose **Full mod**.
5. Pick a **Template**: *Empty mod* or *Balance tweak*. You can add more
   later by copying a folder from `templates/`.
6. Choose a folder and press create.

Studio makes this folder:

```text
ember-age/
  mod.toml            who you are and what the mod is
  files/
    rules.toml        global rules
    patches/          table patches and strings
    tables/           your own side tables
    states.toml       states (add it when you need it)
    mod/              icon, picture, intro
    data/...          art, sounds, maps at their normal paths
  README.md           notes for you; it is never packed
  LICENSE             your licence; it is never packed
```

Only `mod.toml` and `files/` go into the finished mod. Keep the folder
readable. It works well with git.

### mod.toml

Studio fills this in. This is what it looks like:

```toml
id = "ember-age"
name = "Ember Age"
version = "1.0.0"
author = "Ada"
description = "Fire is dangerous and fast."
api = 1
kind = "full"
```

| Key | What it is |
| --- | --- |
| `id` | Short name. Lowercase, 3 to 40 characters. `none`, `base`, `default` and `d2gamma` are taken. |
| `name` | What players see. It is also the window title. |
| `version` | `MAJOR.MINOR.PATCH`, like `1.2.0`. Raise it for every release. |
| `author` | Your name. |
| `description` | One or two sentences. It may be empty. |
| `api` | The modding API the mod was made for. Studio sets it. |
| `kind` | `full` for a mod. `addon` is for later. |

More keys (build on another mod, allow others to build on yours) are in the
[mod format](../mods/mod-format.md). A wrong key stops the mod from
starting, and the message names the key.

## Part 2. Change the rules

`files/rules.toml` holds the switches that change how the whole game plays.
An empty file plays exactly like D2Gamma.

In Studio open **Rules**. Tick a setting, type a number, save. Or edit the
file by hand. Here is a mod where being hit never interrupts you:

```toml
version = 1

[players.hit_recovery]
enabled = false
```

That is the whole [no hit recovery](../../templates/no-hit-recovery)
template. This one lowers the resistance caps and makes the harder
difficulties take more resistance away, as in
[harder fire](../../templates/harder-fire):

```toml
version = 1

[resistances]
elemental_cap = 60
elemental_max_cap = 80
expansion_penalties = [0, -60, -120]
```

Rules to remember:

- A key you leave out keeps D2Gamma's value.
- A key D2Gamma does not know is an error. Typos are caught.
- A number outside its range is an error, and the message names it.
- Percent settings use whole numbers. 100 means "unchanged". 200 doubles.
  0 switches the thing off.
- Fire, cold, lightning and magic share one resistance cap. There is no
  fire-only cap yet.

The full list of keys, ranges and defaults is in [rules](rules.md).

## Part 3. Change items and other tables

Diablo II keeps its data in tables: `uniqueitems`, `skills`, `monstats`,
`cubemain` and so on. A mod never ships a whole table. It ships a *patch*:
a short list of changes. Patches stack in order on top of the player's own
data. Yours go in `files/patches/`. Name them `10-something.d2patch`,
`20-something.d2patch` and so on. They run in name order.

### In Mod Studio

1. Open **Table editor** and press **Load tables**.
2. Pick a table, pick a row, change a cell.
3. A gold cell differs from the base game.
4. Pick other rows by name from drop-downs. Studio lists the name, not
   the number.
5. **Add** and **Duplicate** make new rows. **Disable** removes a row
   without shifting the numbers of the rows after it.
6. The **Problems** panel shows broken links while you type.
7. **Save** writes `files/patches/~studio.d2patch`. Studio only writes the
   changes. It never writes a whole table.

For items there is also the **Items** page. It shows a form for weapons,
armor, unique and set items, runewords and affixes. A tooltip next to the
form shows the item the way the game will. **Drops** shows how often an item
falls. **Cube** tries a recipe.

### By hand

A patch file starts with `d2patch 1`. Then you name a table and list the
changes:

```text
d2patch 1
table uniqueitems
add #402 emberheart
set emberheart lvl [] -> [20]
```

- `table NAME` picks the table. Later lines apply to it.
- `set ROW COLUMN OLD -> NEW` changes one cell. You say what is in the cell
  now. If the cell holds something else, the patch fails. That protects you
  when the base game changes.
- `add #N KEY` adds a row. `N` must be the number of rows the table has
  now. Rows count from 0. `KEY` is the name of the row, like `emberheart`.
  Add `like OTHERROW` to start from a copy of a row.
- `remove ROW` switches a row off. Its number stays reserved.
- `stradd KEY [text]` adds a new text. `strset KEY [text]` changes an old
  one.
- A value with spaces or special letters goes in square brackets.
  `\xHH` writes any byte.

[Tables](tables.md) has the rest. Studio's saved patch uses the same
words, so you can read what Studio wrote.

### A new unique item

Open [`templates/new-unique-item`](../../templates/new-unique-item). It adds a
unique ring in two small files:

- `10-strings.d2patch` adds the name `Emberheart`.
- `20-unique.d2patch` adds the row to `uniqueitems`.

The row's key is the text key of its name. That is how the game finds the
name.

### Names and text

Item names, descriptions and menu texts are *strings*. Add them with
`stradd`. Change one with `strset`. New strings get numbers from 30,000. Do
not ship copies of the game's `.tbl` files.

### Rules for table work

- Patches only. Never copy a whole game table into your mod.
- A new row needs the name text it points at. Add the string first.
- Removing a row that another row still uses is an error. The check lists
  every row that points at it.
- Row numbers and capacity limits are in [tables](tables.md).

## Part 4. States (buffs and stacks)

A state is a buff or a debuff. The `states` table says how it looks. The
file `files/states.toml` says how long it lasts, how many stacks it can
have and what each stack gives.

```toml
version = 1

[[state]]
id = 380
name = "Smolder"
duration_seconds = 8
max_stacks = 5
reapply = "add_stack"
decay = "lose_one"

[[state.effect]]
stat = 25
per_stack = 5
```

A mod without this file plays like D2Gamma. [States](states.md) has every
key, and the icon rules.

## Part 5. Pictures, sounds and maps

You can replace or add art, sounds and maps. Put each file at its normal
path under `files/`. Details, formats and limits are in [assets](assets.md).
The short version:

- Pictures are indexed-colour PNGs with a small TOML file beside each.
- Sounds are WAV, 16-bit, 22,050 Hz.
- Ship finished files. The game never converts art while you play.
- Ship the whole set of a picture, a tileset or an animation. A missing
  piece is an error. It does not fall back to the base game.

### The content rule

A mod may ship changed files. It may ship single base files. It may
**never** ship enough base files to play the base game without an
install. The base game always comes from the player's own Diablo II. The
packager refuses a mod that breaks this rule. Right now it refuses a file
that is a byte for byte copy of a base file, and a mod that replaces more
than 10 percent of the tables, art, sounds or maps. These numbers may
change before release.

Never put game files or files taken from other mods in a repository, a
template or a bug report.

## Part 6. Play it

In Studio press **Play**. Studio saves, then starts D2Gamma with your
folder:

```text
D2Gamma-Game --mod-dir "C:\mods\ember-age"
```

Your mod has its own characters, saves and settings in
`Documents\D2Gamma\mods\ember-age`. It never touches your normal D2Gamma
characters.

You can also start it by hand with that command. Once the mod is installed
in `Documents\D2Gamma\mods\ember-age`, players pick it from the
**D2GAMMA MODS** screen in the main menu, or the launcher lists it as
"Play Ember Age". **Back to D2Gamma** returns to plain D2Gamma.

While you test, the game can check your edits and reload them. Change a
file, open the dev panel and choose **reload**. If the check fails, the game
keeps running and shows the error.

If D2Gamma will not start your mod, it says why. Start plain D2Gamma with
`--no-mod` and open **D2GAMMA MODS** to read the message.

## Part 7. Check it

The check finds mistakes before your players do. In Studio press **Check**.
By hand:

```text
D2Gamma-Game check-mod "C:\mods\ember-age" --native "%APPDATA%\d2gamma\native" --out check-out
```

It does not start a window and it does not change your saves. It looks at:

- the manifest, the API version and the mods you build on,
- `rules.toml`,
- every file you ship, whether the game can read it,
- your patches, and every link from one table to another,
- cube recipes, drops, items at many levels, monster life and damage, and
  skill formulas,
- test games for every row you added or changed.

The answer is grouped by how bad it is:

- **Error**: fix this. The mod is not ready.
- **Warning**: look at it. Some warnings are "Unverified". They need a
  second look before you blame the mod.
- **Info**: counts and notes.

Exit code **0** is a pass. **1** means errors in the mod. **2** means the
command itself was wrong. `check-out/report.txt` is the plain-English
report, and `report.json` is the same for tools. Use `--full` to run every
test game, not only the ones for your changes. It takes much longer.

Each of the three templates passes this check.

## Part 8. Package and sign it

A package is one `.d2pack` file. It is what you send to players.

In Studio press **Package**. By hand:

```text
d2-convert package-mod "C:\mods\ember-age" --base "%APPDATA%\d2gamma\native"
```

You get `ember-age-1.0.0.d2pack` in the current folder. Use `--out` to pick
another place.

Packaging runs the checks first. If they fail, no file is made. Your folder is
never changed.

### Your signing key

The first time you package, a **key** is made for you. It signs every pack.
Players' D2Gamma uses the signature to see that the pack is yours and has not
been changed.

- Windows: `%USERPROFILE%\.d2gamma\author-key.pk8`
- Other systems: `$HOME/.d2gamma/author-key.pk8`

Back this file up and keep it private. Never put it in a mod folder, a
repository or an upload. If you lose it you can still make packs, but not
under the same signature. If someone else gets it, they can sign as you.

The same folder and the same key always make the same pack.

### Install a pack

Copy the pack to `Documents\D2Gamma\mods\ember-age.d2pack`. D2Gamma checks
the signature before it loads anything. A pack that was changed after
signing, or has no signature, is refused.

## The promise about versions

Your mod says which modding API version it was made for (`api` in
`mod.toml`). When D2Gamma brings out a newer API, the old one **keeps
working for 12 months** after the newer one is released. D2Gamma warns
before it stops, and the message says the date. After that the mod shows
"this mod needs updating". To update, rebuild it against the new API,
raise `version` and package again.

A mod made for a newer API than the player's D2Gamma shows "needs a newer
D2Gamma".

## Building on other mods

If a mod's maker allows it (`allow_derived = true`), you can make a mod
that builds on it. Put the base in `base` in your `mod.toml`. Ask the
author first. Studio lists only mods whose authors allow this. Add-ons
(smaller changes that run with a mod) come later.

## The templates

| Template | Teaches | Files |
| --- | --- | --- |
| [showcase](showcase-walkthrough.md) | An original town, maze, boss, skill, quest, synced HUD and approved add-on together. | mod files and original native art |
| [no-hit-recovery](../../templates/no-hit-recovery) | Rules only. | `rules.toml` |
| [harder-fire](../../templates/harder-fire) | Rules only: caps and penalties. | `rules.toml` |
| [new-unique-item](../../templates/new-unique-item) | A string, a new row in a table. | two patches |

To use one: copy the folder, change `id`, `name`, `author` and
`description` in `mod.toml`, and open it in Studio. Each template folder has
its own short README.

## When something goes wrong

| You see | Try |
| --- | --- |
| `unknown key` in `mod.toml` or `rules.toml` | A typo. The message names the key. |
| `this mod needs updating` | The mod's `api` is too old. Rebuild it. |
| `patch ... expected ... found` on a `set` | The cell holds something else than you wrote. Look at the table. |
| `id space exhausted` or a limit message | The table is full. See the limits in [tables](tables.md). |
| A broken link in the check | A row points at something missing. Add it, or fix the name. |
| Packaging refuses a file | It copies a base file. Remove it, or change it. |
| The mod starts but looks wrong | Check art paths and the sidecar pairs in [assets](assets.md). |

## Where to read more

- [Rules](rules.md): every global switch.
- [Tables](tables.md): patches, removing rows, limits, the 86 tables.
- [States](states.md): buffs, stacks and icons.
- [Blocks](blocks.md): the list of building blocks, their versions and how to add one.
- [Assets](assets.md): strings, pictures, sounds, maps.
- [Check a mod](check-mod.md): every check, the report format.
- [Package a mod](packaging.md): signing and the content limits.
- [Studio items](studio-items.md): the item, drop and cube tools.
- [Mod format](../mods/mod-format.md) and
  [playing a mod](../mods/mod-kit-loading.md): the manifest, folders, the
  Mods screen, icon and intro.

Server scripts: see [Luau API and call blocks](scripts.md) and the
[Script Reaction template](../../templates/script-reaction).

Map authoring: [levels](levels.md), [random rules and seed exports](random-levels.md), [connectivity checks](map-checks.md), [Studio map editor](map-editor.md).

[Add-ons: declarations, approval, load order and conflicts](addons.md).

Play the all-system [Lantern Reach showcase](showcase-walkthrough.md) to follow a complete mod, quest and approved add-on.

Recreate that mod through editor controls: [Studio tour](studio-tour.md).
