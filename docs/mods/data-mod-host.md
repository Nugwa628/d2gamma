# The data mod host

A mod made with the mod kit is files only, so the engine needs one built-in place that turns those files into
rules. That is `DataMod` (`crates/d2-sim/src/rules/data_mod.rs`), a `ModRules` that D2Gamma installs as
`Ruleset::Mod` when it was started with `--mod` / `--mod-dir` ([mod loading](mod-kit-loading.md)).

| Run | Ruleset | Table rules |
| --- | --- | --- |
| no `--mod` | `Ruleset::Original`, exactly as before | `DataRules::Original` |
| `--mod` / `--mod-dir` | `Ruleset::Mod(DataMod)` | `DataRules::Mod` (rows past the vanilla counts load) |

## What it reads

| File | Becomes | Effect |
| --- | --- | --- |
| `files/rules.toml` | `DataMod::global()`, answered by `ModRules::global_rules` | the [global rules](../modkit/rules.md) |
| `files/states.toml` | `DataMod::state_rules()`, answered by `ModRules::state_rules` | [states and stacks](../modkit/states.md) |
| `files/crowd_control.toml` | `DataMod::crowd_control_rules()`, answered by `ModRules::declared_cc` | [declared crowd control](../modkit/crowd-control.md) |
| `files/ai.toml` | `DataMod::ai_rules()`, answered by `ModRules::monsters` (a `MonsterProvider`) | [monster AI graphs](../modkit/ai.md) |

The file comes from the last mod in the stack that has one (a mod built on another mod replaces its base's file
whole; keys are not merged). A mod with no file, or an empty one, plays exactly D2Gamma's rules: an empty
`DataMod` is tested to give the same reactions as `Ruleset::Original`.

## Start-up

`d2_client::kit_mod::activate` reads and checks the rules file once at start, next to the mod's manifest, so a bad
file ends the start with the launcher's error file instead of failing in a game started later from the menu. The
message names the file and the setting:

```text
C:\mods\ember\files\rules.toml: invalid rules.toml setting: `players.knockback.duration_percent`: ...
```

`d2_client::kit_mod::ruleset()` then hands the ruleset to the game. It is installed on the local server (single
player); the client's views read the same value through `ClientMods::with_rules`. An online game uses the server's
rules: a client never sends or installs a ruleset.

## Where later parts plug in

`DataMod` answers a `ModRules` seam only for a part the mod's files define; every other seam keeps its default of
"no change". A new part of the mod kit (states and stacks, building-block skills, declared crowd control) adds:

1. a member to `DataModFiles` for its file, and a field on `DataMod` filled in `DataMod::from_files`;
2. the matching `ModRules` method on `impl ModRules for DataMod`, returning that field;
3. its file's errors through `DataModError`, so they name the file and the key like the rules file does.

The sim does no file work: `d2_client::kit_mod::host` reads the files of the mod stack and hands the text to
`DataMod::from_files`, so the host stays pure and deterministic.
