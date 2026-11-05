# Playing a mod-kit mod

A data-only mod made with the mod kit ([mod format](mod-format.md)) is started with:

```text
D2Gamma-Game --mod <id>          an installed mod, by id
D2Gamma-Game --mod-dir <folder>  a mod folder anywhere (a mod being tested, Mod Studio's play button)
```

`--mod <id>` looks in `Documents\D2Gamma\mods\<id>\`. A `--mod` value that is not a valid mod id is taken as a
folder. Both belong to `play` (the default command); `map` keeps its own `--mod NAME`. With neither flag D2Gamma
plays `base/` alone exactly as before: same layers, same frame hashes, and `mods/order.toml` is never read.

## What is stacked

`d2_modkit::resolve` turns the request into the ordered mod folders and refuses, in plain words, anything that
cannot run:

- the folder has no `mod.toml`, or it is a layer-only file without `api` (those load as layers of a mod game, not
  on their own);
- the manifest has a problem (every problem is listed, each naming its field);
- the mod's modding API version is outside the supported window (`NeedsUpdate` / `TooNew` messages);
- the mod is an add-on (add-ons run with the mod they are made for, not by themselves);
- the mod builds on another mod (`base`): that mod must be installed, allow derived mods and be in the version
  range; its own base comes first, up to 8 deep, loops refused;
- a mod it `requires` is not earlier in the stack.

The stack goes to the engine as the layer list `base/`, then each mod folder, bases first. An absolute folder in
`d2_native::layers::LayerStack::with_mods` is a named layer (the mod's `id`), and a kit mod under a native
`mods/` folder must sit in a folder named by its `id`. There is no second manifest parser: the layer stack reads
`mod.toml` through `d2_modkit::parse_mod_toml`.

## The mod's own game identity and folder

The mod's `name` is the window title and the version line, its `version` the version. Its user files live in
`Documents\D2Gamma\mods\<id>\`: `saves\`, `settings.toml`, `controls.toml`, `logs\` and the baked animations
(`cache\`). Nothing is shared with D2Gamma's own saves, and nothing is migrated in. The same folder holds the
mod's installed files (`mod.toml`, `files\`). With `D2RUST_HOME` set (tests, portable installs) the mod's folder is
`<D2RUST_HOME>\mods\<id>\`.

## The mod's rules

The mod's `files/rules.toml` is read at start into the mod's [data mod host](data-mod-host.md), which the local game
runs under as `Ruleset::Mod`; its tables load under `DataRules::Mod`. A file that cannot be used stops the start in
plain words, naming the file and the setting. A mod needs the converted native folder.

## Picking a mod from inside the game

For an offline `--mod-dir` game, [dev reload](dev-reload.md) watches the source files,
checks changes in the background and offers a rebuild/relaunch from a dev slot. Installed
mods selected with `--mod` and online games do not watch or reload.

The process-wide identity, user folder and data are fixed at start, so picking another mod starts D2Gamma again
with `--mod <id>` (`d2_client::kit_mod::child_args`) and the running process waits for it and exits with the same
code (`kit_mod::relaunch`). A launcher sees one game and one exit code.

### The D2Gamma Mods screen

With at least one mod installed, D2Gamma's main menu shows D2GAMMA MODS in the place of the (inert) Other
Multiplayer button; without mods the menu is unchanged. The screen (`screens/mods.rs`, list from
`d2_client::mod_menu::list`) shows, per mod in `Documents\D2Gamma\mods\`: name, author, version, description, the
mod's picture when it has `files/mod/picture.png` (PNG, at most 1024 x 1024 and 4 MB; shown at most 104 x 78) and
its state. A mod whose `api` is outside the supported window says "this mod needs updating" / "needs a newer
D2Gamma" (the `d2-modkit` messages); a mod whose `mod.toml` cannot be read is listed under its folder name with
the problem. Those rows cannot be started. Picking a ready mod (PLAY, Enter or a double click) leaves the front end
and restarts into it. Inside a mod the same button reads BACK TO D2GAMMA and restarts into plain D2Gamma.

The last played mod is remembered in `Documents\D2Gamma\mods\active.toml` (`last_played = "<id>"`, or `"none"` for
plain D2Gamma). A start with no flag opens it; if it cannot start (removed, broken, needs updating) D2Gamma says
so and opens plain D2Gamma instead.

```text
D2Gamma-Game --no-mod     plain D2Gamma this once (what was last played stays remembered)
D2Gamma-Game --mod none   plain D2Gamma, and remembered as the last played
```

`none` is reserved: no mod can use it as its id. Runs that are not a front-end session (`--new`, `--save`,
`--frames`, dumps, offscreen) never open the remembered mod unasked.

## The mod's icon and intro

A mod may replace its window icon and its intro. D2Gamma without a mod, and a mod that ships none of these, start
exactly as before (the exe's icon, the same trademark screen, same frame hashes).

**Window and taskbar icon.** `files/mod/icon.png`: a PNG, at most 1024 x 1024 and 4 MB (256 x 256 is plenty); the
window system scales it. It is set on every window the game opens (`d2_client::app::window_icon`). A derived mod
without its own icon shows the icon of the mod it builds on. A mod that also wants its desktop shortcut and the
launcher's entry to carry the icon ships `files/mod/icon.ico` too: the engine does not read the `.ico`, the
launcher does.

**The trademark screen.** `files/mod/intro.toml`, every key optional (an unknown key or a bad value is refused
with its name, and the game starts with D2Gamma's own intro and notes why in its log):

```toml
skip = false                   # true: the game opens on the main menu
seconds = 9                    # how long the screen stays when nobody clicks (1 to 60)
logo = true                    # false: no animated Diablo II logo on the screen
text = ["Ember Age", "by Ada"] # up to 4 lines of up to 120 characters, in place of the legal text; [] shows none
```

The derived-mod rule is the same: the nearest mod with an `intro.toml` decides.

**Intro music.** `files/mod/intro.wav` plays in the menus in place of the first track of D2Gamma's menu music (the
one heard on the trademark screen and the main menu); the other menu tracks that follow are the game's. It must be a
WAV the game's own music format allows: 16-bit PCM at 22,050 Hz, mono or stereo, at most 128 MB. The nearest mod
with an `intro.wav` decides (a derived mod's own file wins over its base's). A file that is not playable is not
used: the game starts with its own music and notes why in its log. Without the file nothing changes.

**Pictures.** Every front-end picture is replaced like any other sprite: the mod ships the converted file at the
same path in its `files/` layer, and the last layer that has it wins. The ones that make up the intro and the menu
backdrop (paths lower-case, as in the native layout):

| Picture | Path under `files/` |
| --- | --- |
| Trademark screen backdrop (Lord of Destruction / classic) | `data/global/ui/frontend/trademarkscreenexp.dc6`, `data/global/ui/frontend/trademark.dc6` |
| Main menu backdrop (Lord of Destruction / classic) | `data/global/ui/frontend/gameselectscreenexp.dc6`, `data/global/ui/frontend/titlescreen.dc6` |
| The animated logo, two halves (trademark screen and main menu) | `data/global/ui/frontend/d2logoblackleft.dc6`, `data/global/ui/frontend/d2logoblackright.dc6` |
| Buttons and the rest of the front end | the same folders (`data/global/ui/frontend/`, `data/global/ui/charselect/`, ...) |

`logo = false` removes only the animated logo halves; the "Expansion Set" lettering is part of the backdrop, so a mod
that wants a different title replaces the backdrop. There are no videos: a mod cannot add or replace a cinematic.

## Not in this step

`--mod` / `--mod-dir` start mod folders; the layer
stack can open a `.d2pack` too, but starting a signed pack by id (and checking its signature on load) waits for the
packager and key handling. Intro music is the `intro.wav` file, not an `intro.toml` key.


## Copy a D2Gamma character

A full mod may set `accepts_d2gamma_characters = true` in `mod.toml`.
The default is false. The maker is responsible for keeping the mod compatible
with base character progress, classes and item codes. Studio exposes the same
switch in the manifest form; add-ons cannot grant this permission.

From plain D2Gamma, select an accepting mod in **D2Gamma Mods** and choose
**Copy character**. Pick a character with the arrow buttons, then click **Copy**
to confirm. Enter does not confirm a copy. The original `.d2s` stays in
`D2Gamma/saves`; the converted `.d2m` is created in
`D2Gamma/mods/<id>/saves`. Existing characters are never overwritten, including
names that differ only by letter case. A conversion error creates no character.
The inventory, socketed items, corpse, hireling and golem items are converted
through the base tables. No copy takes place when the mod starts or when its row
is selected. Start the mod normally to play its independent copy.
