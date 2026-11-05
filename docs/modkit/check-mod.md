# Check a mod

```
D2Gamma-Game check-mod <mod-folder-or-pack> --native <converted-install> --jobs 4 --out check-mod-out
D2Gamma-Game check-mod --base --native <converted-install> --jobs 4 --out base-check-out
```

This headless command uses no window or audio device and does not activate a mod,
change the selected game, migrate the user folder, or write player saves/settings.
It requires the player's converted base install. `D2_NATIVE_DIR` can supply
`--native`; otherwise the same default converted install as Play is used.
`--mods-dir DIR` selects installed dependencies (the default is
`Documents/D2Gamma/mods`). The native `mods/order.toml` is ignored. `--jobs` defaults to the available CPU
count capped at four; it only changes scenario concurrency.

All data sweeps run by default. Gameplay scenarios run only for changed or added
rows, comparing the effective patched text with base text (including dependencies
and formula edits). Direct identities cover items, cube recipes, skills, monsters,
NPCs, player classes, levels and door objects. Unchanged setup actors do not select
extra scenarios. Rows without a compatible generated scenario receive data checks
only; this includes shared tables such as treasure classes and item types.
`--full` runs every generated scenario, including panel combinations.
`--base` without `--full` runs all data checks and no gameplay scenarios.

Folders use the same `d2_modkit::resolve` path as `--mod-dir`, including API,
full-mod kind, builds-on permissions/version ranges, dependency order and depth.
Packs use that resolver on their manifest, then read assets directly from the
pack through the native layer stack. No game data is extracted or copied into
the mod. Both use `DataRules::Mod`; `--base` uses D2Gamma's default data rules.

The checks run in this order:

1. Manifest, API compatibility and dependency resolution.
2. Strict `files/rules.toml` parse in every resolved mod; absence uses defaults.
3. Building-block names and files (`states.toml`, `skills.toml`, `triggers.toml`,
   `crowd_control.toml`): names against the [block vocabulary](blocks.md), then the
   files as the game reads them.
4. Native layer file checks (format, allowed paths, sidecar/pixel pairs).
5. Decode effective mod assets, including files no table explicitly references;
   report a count of native files and base overrides.
6. Runtime table patch application, schema/compiler checks, table references,
   string references and explicit asset references (the same checks as playing).
7. Enabled cube recipe input bounds and outputs; treasure-class level/weighted
   boundary selection; monster drops at every difficulty/rank/quest combination;
   superunique drops; enabled monster HP/damage arithmetic across areas, modes
   and difficulties; seeded treasure-class drop walks; creation of every item
   at every requested quality,
   classic/expansion, normal/hell, and item levels 1/30/60/99. Expected classic,
   crafted-affix and ear-without-player refusals follow the existing item sweep.
   Skill and missile formulas run at levels 0 through 99 with no unit and a bare
   unit, including passive skills that the cast scenarios cannot exercise.
   Inverted monster HP ranges are informational: the runtime uses fixed minimum
   HP when the maximum is lower, which is valid in base data too.
8. Changed-row scenarios from the seven existing headless families (`items`, `skills`, `npcs`,
   `areas`, `panels`, `anims`, `doors`), including
   simple cube transmutation, equipment/pickup/drop, skill casts and town NPC
   interactions, area routes, panel states, animation rates and door interactions,
   with fresh games and save checkpoints. `--full` runs all of them.

Independent rules and asset checks can report together. A failed prerequisite
stops dependent checks; structural errors prevent scenario execution. Repair
the errors and rerun to obtain full coverage. The [content sweep guide](../tools/sweep.md)
documents the scenario generators' bounds and skip reasons. Passive skills,
special targets, conditional recipes and unreachable NPCs may be skipped;
the report preserves those counts. These checks do not prove visual/audio/map
playability or enforce the packager's base-content threshold.

Scenario assertion failures are **Unverified warnings**, as in the existing
sweep tool; they need reproduction before being treated as engine/content
defects. Runner failures/crashes are errors. Structural, load, reference, rules
and item-creation errors are errors. No failure is silently treated as a pass.
Global rules are parsed here; the scenario sweeps retain their existing rules
execution path. This command does not add new runtime rule installation.

The console report groups findings by severity and category, for example
`3 broken cube recipes`, followed by locations and explanations. Outputs are
`report.txt`, `report.json` and the families' existing scenario/repro
artifacts in each family's subdirectory.

Exit codes: **0** passed (possibly with Unverified warnings), **1** content
errors, **2** invalid invocation or failure to complete/write the report.
For content failures, `report.json` is still written. Use a fresh output folder
per run if you want an archive; an existing report is overwritten.

## Studio contract

Studio can invoke the command and read `report.json` without linking the client
or Bevy. Engine tools can also use
`d2_client::app::check_mod::check(&Options) -> Result<Report>` with explicit paths
and worker count. It has no process-wide activation. `Report::text()` renders
the same plain-English report used by the command.

JSON schema version 1:

```json
{
  "format_version": 1,
  "target": "my-mod",
  "exit_code": 1,
  "findings": [{
    "severity": "error",
    "category": "broken cube recipes",
    "location": "cubemain row 12",
    "message": "numinputs is 3 (cube capacity 12); active inputs require at least 4 items"
  }],
  "completed": ["manifest, API and dependency checks"],
  "sweeps": {},
  "gameplay_scope": "changed rows",
  "changed_rows": {"cubemain": [12]}
}
```

`severity` is `error`, `warning` or `info`. Locations retain native loader
table/row/column messages or generated scenario identities. `completed` names
finished stages; a missing sweep was not run. `sweeps` maps each executed family
to its full `sweep 1` report, including skips and Unverified signatures. Studio
must check `format_version` before interpreting the report. Informational file
and item-request counts are included as findings without affecting the exit.

`gameplay_scope` is `changed rows` or `full`. `changed_rows` contains zero-based
effective text row indices, including additions. `sweeps` contains only executed
families. Full gameplay is intentionally optional: on the real base, items alone
generated 2,749 scenarios; roughly 500 took five minutes with four workers.
