# Quests made of blocks

A mod can add quests with `files/quests.toml`. A quest is a name, a log text and two lists of blocks: the
**objectives** it asks for and the **rewards** it gives. D2Gamma's own quests stay Rust code and are not touched;
this file adds quests next to them. A mod without the file plays exactly as before.

```toml
version = 1

[[quest]]
id = 1
name = "Pest Control"
text = "Kill three rats."
done_text = "The cellar is quiet."
giver = 7                      # talk to NPC class 7 to start it (optional)
requires = []                  # quests that must be done first (optional)

[[quest.objective]]
kind = "kill"
monster = 12
count = 3

[[quest.reward]]
kind = "stat"
stat = 14                      # itemstatcost row: gold
value = 500

[[quest.reward]]
kind = "message"
text = "Thank you."
```

A quest is done when **all** its objectives are. Ids are 1 to 65535 and unique. A file has at most 128 quests and a
quest at most 8 objectives, 8 rewards and 8 required quests. A requirement must name a quest in the file, and the
requirements may not loop. Names are up to 60 characters, texts up to 240, with no control characters.

## Objectives

| `kind` | Keys | Counts when |
| --- | --- | --- |
| `kill` | `monster` (monstats row), `count` (default 1) | the player kills a monster of that class. A kill by the player's summon counts. |
| `enter` | `level` (levels row) | the player enters that level (by walking, a portal or a waypoint). |
| `talk` | `npc` (monstats row) | the player opens the chat of an NPC of that class. |
| `use_skill` | `skill` (skills row), `count` (default 1) | the player casts the skill, which may be an item's or a charge. |
| `reach_stat` | `stat` (itemstatcost row), `value` | the player has at least `value` of the stat. Once met it stays met. |
| `collect` | `item` (item code, up to 4 characters, e.g. `hp1`), `count` (default 1), `consume` (default false) | the player carries that many items of the code in the inventory, belt or on the cursor (a stack counts its quantity; worn items and the stash do not). |

`collect` counts what the player holds, so dropping the items takes the count back until the quest is done. Finishing the quest takes nothing from the player
unless the objective sets `consume = true`: then `count` pieces are taken from the inventory, belt or cursor at the next
item pass (within a fifth of a second; a stack larger than what is left is replaced by a stack of the rest). If the
player dropped some of them in that moment, less is taken, and the quest stays done. The count is looked at every fifth of a second. A quest started by
a giver counts from the moment it starts; kills and casts before that do not count (items held already do).
A thing that is not an item (an effect of a skill, say) can still be asked for through a stat of the mod's own (see
[stats](stats.md)) and `reach_stat`.

## Rewards

| `kind` | Keys | Does |
| --- | --- | --- |
| `stat` | `stat`, `value` (not 0, may be negative) | adds to the player's base value of the stat: gold, stat points, skill points, or a stat of the mod's own. |
| `message` | `text` | puts a line in the player's chat log. |
| `item` | `item` (item code), `count` (default 1, up to 20) | puts fresh items (normal quality, identified) in the inventory. With no room they wait, the player is told once to make room, and they are given as soon as there is. A code the game does not have is reported to the player and dropped. |

## Progress

A player's progress is kept in the game and in the character's **mod save** (the section `QSTS`), so it survives
leaving and rejoining. A quest or objective that the file no longer has is dropped when the character loads; counts
are kept up to the new target. Quests started and finished tell the player in the chat log, and the `/quests` chat
command lists the log with the counts.

## Quest log panel

The quest runtime publishes each player's log as transient UI variables over the mod network messages (the LCUI
service of [ui](ui.md)), whenever it changes and when the player joins or the character loads:

| Variable | Value |
| --- | --- |
| `quests/q<id>` | 1 while the quest is open, 2 when done. A quest not begun has no variable, so a panel shows a dash for it. |
| `quests/q<id>o<n>` | the count of objective `n` (0 is the first), up to its target. |

A panel binds them as `var.quests/q<id>` and `var.quests/q<id>o<n>` (a bar for the state with `maximum = 2`, a text
with `format = "range"` and `maximum` set to the target shows `2/3`). At most 192 variables are published to a
player (the client keeps 256 names in all, shared with scripts); past that, quests begun later are not shown.
Mod Studio's **Write quest log panel** button (Quests page) writes `files/ui/quest_log.toml` from the quests, with
`replace = "quest_log"`, so it opens and closes with the game's own quest log key. It lists as many quests as fit on
the 1280 by 720 screen and says how many did not. A file there that Studio did not write is never replaced.

## State flags

The save section is the mod's own flag store: a quest being open or done is the flag other quests read through
`requires`. A mod that wants a flag for something else makes a quest for it (an objective of one `enter` or `talk`).

## Mod Studio

The **Quests** page lists the quests and edits one as a form: name, texts, giver NPC, required quests, and the
objective and reward blocks (each added from a menu that shows what the block does). Save writes
`files/quests.toml`, and refuses while the engine would refuse it. See [studio-kit](studio-kit.md).

## Limits of this version

* The quest log panel is generated (one row per quest and objective, with the quest's name and the objective
  in words as fixed text); a quest's log text and "done" text are in the chat and `/quests`, not in the panel.
* `collect` does not take the items when the quest is done, and an item reward is plain (no stats, no quality).
* No client quest-log panel: the log is chat text and `/quests`.
* An item reward is plain (no stats, no quality).
  A reward waiting for room is lost if the player leaves the game first.
* `talk` and `use_skill` are taken up within a fifth of a second of the event, not on the same tick. Kills, levels
  entered and stat checks happen on the event or the same fifth of a second.
* A quest that needs the player to be somewhere in the world (a position) is not expressible yet.
* The base quests stay Rust presets; they cannot be listed or changed from this file.
