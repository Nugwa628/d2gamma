# Studio state, skill, trigger and quest editors

The States, Skills, Triggers and Quests pages edit the four files the engine reads (see `states.md`, `skills.md` and `quests.md`): `files/states.toml`, `files/skills.toml`, `files/triggers.toml` and `files/quests.toml`. They also add the table rows those files refer to, through the same working set the table editor uses. Save writes the four files and `files/patches/~studio.d2patch`, nothing else.

## Files are written canonically

Studio loads each file into its own model and writes it back in a fixed key order. **Comments and the key order of a hand-written file are lost on save.** A missing optional field stays missing, so the engine default still applies. A file Studio cannot parse is never overwritten: the page shows the error and asks you to fix the file in a text editor. If a file changed on disk after it was loaded, Save refuses rather than overwrite it.

Validation is not re-implemented. Every problem shown comes from the engine readers (`StateRules`, `SkillRules`, `TriggerRules`, `QuestRules`) and their id checks, so Studio accepts exactly what the engine accepts. Save refuses while the engine would reject a file.

## States

A form per state: duration, max stacks, what happens when it is applied again, what happens when time runs out, icon, an overlay effect picked from the overlay table, and stat effects (`stat`, `per stack`, `from stack`, `flat`). The preview lists what the state gives at each stack count, for example "at 3 stacks: +25% damage".

## Skills

A skill is a list of parts run in order: missile, aura pulse, curse, heal, resurrect, wall, summon, apply state / add stack, melee hit, area hit. Each part shows the fields the engine reads for it. Formula fields are compiled live with the engine compiler (`Formula::compile`); a bad formula shows the compiler's message and a red mark.

"Numbers by skill level" draws every numeric formula of the skill for levels 1 to 30. The values come from `Formula::curve`, which runs the engine's own `eval_desc` on a one-row skills table carrying the Param1 to Param8 you enter, so the chart and the game cannot disagree. Without a unit in play `rand()` and stat reads give what the engine gives with no unit, which is not what a particular character would see.

### Copy a base skill

"Copy a base skill" (under the skill list, when the tables are loaded) lists the base skills the lifter knows: the
common attack, the plain missile skills, the nova, the basic aura, the curse, Meteor's drop and the Zakarum heal (see
[Base skills as data](skills.md#base-skills-as-data)). **Copy as parts** adds the skill to the kit with the id of its
table row and the parts the lifter wrote, and sets the row's `srvdofunc` to the block slot (a missile skill's
`srvmissile` is emptied, because the core would fire it a second time). The notes that say what the copy does
differently from the Rust body show under the list. The row's other cells stay, since the lifted parts read some of
them (a nova's `calc1`).

A copied skill has a **Differences from the base skill** fold-out: a line diff of the skill as the editor saves it
against what the lifter makes of the original (unpatched) row, `-` for a line only in the base and `+` for one only in
your copy. It says "Same as the base skill" until you edit. The base comes from the game's own tables, so it is still
there after the project is saved and reopened. Skills that were not copied (new ones, or rows that are not a lifted
family) have no diff.

## Triggers

Each trigger reads as a sentence, "When you attack (8%), if you are not in Reaction window: apply Reaction window". Select one to edit its event, chance, conditions and actions. Triggers, conditions and actions can be added, removed and moved up or down; order is the order the engine runs them.

## Quests

The list shows each quest as its name and what it asks. Select one to edit its id, name, log texts, giver NPC (picked from the monsters table), required quests, and its objective and reward blocks. The "Add" buttons list every objective and reward block the engine knows, with what it does; a block's picker uses the table names when an install is loaded. See `quests.md` for what each block does.

*Quest log panel:* the **Write quest log panel** button on this page writes `files/ui/quest_log.toml` (see `quests.md`).

## Ids and table rows

States, skills and triggers refer to table rows by id. With tables loaded, ids are picked by name. Adding a state or skill adds the row to the states or skills table in the working set (and to the patch file on save). Without an install loaded, ids are plain numbers and no rows are added.

## New from template

"New from template" offers the proc to charges example from `templates/proc-charges`: a state that opens a window, a state that stacks, and the triggers linking them. It lists the rows it needs, lets you pick ids, and can create the missing rows. Existing entries are never replaced.

## Rendering the pages

```
xvfb-run -a cargo run -p mod-studio --example render_kit -- states.png states
xvfb-run -a cargo run -p mod-studio --example render_kit -- skills.png skills
xvfb-run -a cargo run -p mod-studio --example render_kit -- triggers.png triggers
```
