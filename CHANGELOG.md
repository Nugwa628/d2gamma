# Changelog

## 0.1.1.3177

A hireling fix release.

### Hirelings

- Hirelings no longer turn invisible while attacking. Act 3 and act 5 hirelings (and an act 2 hireling's auras)
  vanished for every skill use, because the game took every hireling for an act 1 rogue.
- A revived hireling no longer stays lying dead at your feet: she gets up, follows you and fights again.
- Walking and the mouse no longer freeze after hiring or reviving a hireling. A hireling left dead out in the
  wild can still be revived later, and when the NPC refuses a service, the menu comes back and Esc ends the talk.

## 0.1.1.3162

The mod kit and Mod Studio arrive, online play gets ready for mod games, and a round of hireling and gameplay
fixes.

### Mod kit & Mod Studio

- **Mod kit:** a mod is now a folder of readable files that D2Gamma plays with `--mod <id>` or
  `--mod-dir <folder>`, or packed into one signed `.d2pack` file. Each mod has its own folder, saves and identity.
- **Mods screen:** pick a mod from the main menu, see when you last played it, or go back to plain D2Gamma. A mod
  can replace the window icon, the intro screen and the intro music.
- **Mod Studio**, a separate no-code editor: a new-mod wizard, start page and recent projects, plus editors for
  rules, tables, items, drops and Horadric Cube recipes (with drop and cube simulators), named stats,
  difficulties, acts, towns and waypoints, states, skills, triggers, quests, monster AI, scripts, visual scripts,
  HUD panels, visual effects, network values and art and sound imports. It can also check, package and play
  your mod.
- **Rules without code:** combat, drop, economy and crowd-control numbers, uninterruptible cast points and
  animation breakpoints are settings in `rules.toml`.
- **Tables:** table patches can turn rows off without shifting ids and can fill columns the base game leaves
  empty. Each mod's tables load under its own data rules.
- **New content as data:** named stats, extra difficulties with their own progress, acts and towns,
  hand-made and random levels with a map editor and seed previews, quests with objectives, rewards, quest
  items and a quest log panel.
- **Skills as editable data:** the player skills of all seven classes can be copied into a mod and edited,
  including auras, summons, Druid forms, Assassin traps and charge-ups, warcries, curses, walls, projectile
  fans and passives.
- **Monster AI as data:** monster behaviour graphs, presets that use D2Gamma's own AIs by name, boss phases with
  immunity windows, and an AI test arena that runs one monster against a test character. Many of the game's own
  AIs are available as editable graphs.
- **Scripts:** sandboxed server-side Luau scripts and visual script graphs, called from skills, triggers and
  events.
- **Mod UI:** mod panels and overlays can show server values, offer buttons the server checks, and read combat,
  skill, quest log, waypoint and NPC menu state. There are visual effects and monster wind-up telegraphs.
- **Checks:** `check-mod` tests a mod headless (stats, drops, items, skills, maps, animation rates), and a dev
  slot reloads folder edits in a running game.
- **Add-ons:** small add-ons can run on top of a mod when its maker approves them. Screen-only add-ons can also
  run on plain D2Gamma.
- **Guide and examples:** a modder's guide in `docs/modkit`, template mods, and the *Lantern Reach* showcase mod.

### Multiplayer and servers

- Online mod games: a mod game's servers use its own rules and data, announce its game id to the realm, and keep
  its stash, market and ladder apart from other games.
- A mod can have its own market window and stash panel. Stash moves keep each item in exactly one place, and the
  stash gold cap is enforced.
- Mod front screens and lobbies can use the realm session: login, game list, chat and join.
- Servers make their game data themselves from your own Diablo II, as one shared base per machine.
- Game server folders name their program and release channel, so one machine can run servers for several games.
- `server-ctl logs` finds the log of the program that is actually installed.
- The realm warns when a game server links from loopback without a public address.
- Online walks end standing, for your own character and for other players.
- Charge reaches other players' characters properly, and Leap Attack lands facing the right way.

### Gameplay fixes

- Hirelings: the hire flow and the hire list hover work again. The hireling portrait is fixed, and you can give
  a held item to your hireling by dropping it on her portrait.
- Pet icons are fixed, and so are Shift-clicked potions for your hireling.
- Charge restarts its sequence correctly.
- After a teleport, the client no longer keeps the player's old position.
- Charms with no affix are kept.
- Lair-type levels get their way in and way back.
- Spending a skill point follows the game's rules.

### UI

- Upscaled text no longer swaps letters when two text scales are drawn in the same frame.
- The automap is sized from the level data.
- `--game-dir` pointing at a folder that is not a Diablo II install is now an error with a clear message.
- The dev console's `level` command is fixed.

### Tools

- `d2-convert` converts the sounds that only `sounds.txt` names, skips the two cursor index files, and
  re-converts older installs when it needs to.
- `bake-anims --data-rules` compiles a mod game's tables under its own rules.
- Release builds report `"mods": true` in `version.json`.

## 0.1.1.2159

First public D2Gamma snapshot: the full game rewrite, the 60 Hz game loop, smooth animation, modern display
options, and online play with a main server and game servers.
