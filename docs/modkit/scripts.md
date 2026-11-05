# Server scripts (API v1)

Scripts are Luau, vendored through `mlua`. Scripts execute on the authoritative server,
including single player. The client never runs them to predict an outcome.

Put source modules in `files/scripts/*.luau` (subfolders work too).
Each module returns a table of functions:

```lua
assert(SCRIPT_API_VERSION == 1)
return { double = function(value) return value * 2 end }
```

S1 supplies loading; S2 binds the versioned events and capabilities below.
S3 adds reusable skill/trigger calls and an asset-free playable template. Module names include `.luau`
and are relative to `scripts/`. Later layers replace earlier modules at the
same path. Packs and folders use the same reader; base is never searched.

Limits: 64 effective modules, 1 MiB total source, a separate VM per module with
a 4 MiB total Lua allocation ceiling and 10,000 interrupt checkpoints per load
or function call. Luau checks at calls and loop back edges; this is not an
exact bytecode instruction count. Native library operations are bounded by
memory rather than interrupted mid-operation. S1 call arguments/results are
signed 32-bit integers. Luau supports floating point internally; integer-first
refers to the engine API. Randomness comes only from the chosen unit's simulation seed.

No filesystem, require, io/os/debug, clock, ambient RNG, dynamic code loading,
environment manipulation, coroutines or console output is exposed. Libraries
and builtin metatables are sandboxed read-only. Protected calls are removed so
budget errors cannot be caught and retried forever. Only source is accepted,
never untrusted bytecode.

`check-mod` loads effective modules inside the sandbox; syntax, load, memory
and execution-limit errors name the module path. Runtime call errors include
module and function for the caller's log. Keep top-level code to definitions:
check-mod executes it too. Existing checked dev `reload`/auto-reload watches
`files/scripts/` and restarts from the slot with fresh VMs. Script state is
not saved. Base rules skip source discovery and VM construction entirely;
script-free mods also allocate no VM.

## Events

Return optional functions named `on_attack_start`, `on_attack_hit`,
`on_cast_start`, `on_cast_complete`, `on_skill_used`, `on_hit_taken`,
`on_state_start`, `on_state_end`, `on_kill`, `on_drop`, `on_buff_end`,
`on_timer`. A reserved handler with any other type fails check-mod.

```lua
return {
    on_skill_used = function(e, api)
        api.message(e.unit, "Skill used")
        api.timer(e.unit, 25, e.id)
    end,
    on_timer = function(e, api)
        api.message(e.unit, "Timer complete")
    end,
}
```

Each handler receives read-only `e` and `api` tables. `e` has `version = 1`,
`event` (the suffix above), `frame`, opaque `unit`, optional `unit_type`
(0 player, 1 monster, 2 object, 3 missile, 4 item, 5 tile), optional `other`, and `id`
(skill, state, timer cookie, or zero). A unit may have left the world by the
callback; APIs reject stale handles. Keep IDs within this game only.

Modules run in lexical path order. Combat callbacks follow the existing
engine events: attack_start is the melee swing; attack_hit includes melee and
missile damage; hit_taken is on the damaged unit; kill is on the killer.
Cast_start runs after core start checks and before the start body (which may
still refuse); cast_complete and skill_used run in that order after a successful
do step, excluding data-trigger casts. They describe do steps, not animation end.
State start/end observe actual common-wiring state-bit transitions, without
repeating a start on stack refresh. Buff_end describes a detached stat list's
remove callback, including clearing/replacement; it can occur while another
list still supplies the same state. Drop describes ground item placement,
including player redrops; `unit` is the item, with no dropper promised.
State, buff and drop events are queued, in insertion order, for the end of the
server tick. Engine internals that mutate raw state storage bypass the common
state transition seam; these are not an exhaustive state audit stream.
Then due timers run by deadline and insertion order, only in the scheduling
module. Removed timer units are discarded. Timers use simulation frames,
never wall time; delays must be positive, so a timer cannot recur in one tick.

## Capabilities

All numeric inputs must be exact integers. Values are i32 unless stated;
unit handles are u32, IDs/layers/stack counts are u16 and IDs must exist in the
loaded tables. Methods use dot calls (`api.stat(...)`).

| Method | Effect |
| --- | --- |
| `stat(unit, stat, layer)`, `base_stat(unit, stat, layer)` | Read live total/base stat |
| `set_stat(unit, stat, layer, value)` | Write base stat through engine callbacks |
| `has_state(unit, state)`, `state_stacks(unit, state)` | Read state/stack count |
| `apply_state(source, target, state, ticks, skill)` | Apply using states.toml rules, level 1 |
| `add_stacks(source, target, state, ticks, skill, count)` | Add stacks with those rules |
| `remove_stacks(unit, state, count)`, `clear_state(unit, state)` | Remove stacks/state |
| `damage(source, target, amount, element)` | Apply already-hit damage and reaction through combat; amount is fixed 1/256 life; element 0 physical, 1 fire, 2 lightning, 3 magic, 4 cold, 5 poison; cold/poison last 25 simulation frames |
| `spawn_missile(source, missile, skill, level, lob)` | Skill missile toward source aim; level 1..255, lob 0/1; handle or nil |
| `spawn_monster(owner, class, x, y, pet_type, max)` | Owned allied summon through engine pet placement; subtiles 0..65535, pet_type 0..18, max 1..32; handle or nil |
| `message(player, text)` | Private mod chat line, up to 200 printable ASCII bytes |
| `random(unit, upper)` | One unit-seed draw, result 0..upper-1, upper > 0 |
| `timer(unit, frames, cookie)` | Schedule this module's on_timer; 1..2160000 frames |

State fallback durations are base ticks (25/second), 0 means no expiry,
maximum 2160000; a states.toml duration takes precedence. No raw packet,
filesystem, arbitrary stat-list pointer or Rust object is exposed.

A callback can make 128 API calls. A game has at most 256 queued cleanup
notifications, 256 pending script timers and 256 handler invocations per frame;
excess cleanup notifications/handler invocations are dropped deterministically,
and excess timers/API calls report an error. The VM limits above reset per
callback. Synchronous events raised inside a script's engine operation are
suppressed to prevent recursive VM entry; deferred events arrive on later
queue drains. API tables/functions expire after the callback even if retained
by Lua. Errors name module/function and go to the server log; earlier writes
remain applied. Callbacks are not transactions. Script globals/timers are
per-game transient state and reset on reload/restart.

## Call-script blocks (S3)

Use the same block in a K7 skill's part list or a trigger's action list:

```toml
[[skill.part]] # or [[trigger.action]]
kind = "call_script"
module = "reaction.luau"
function = "react"
argument = "lvl * 2" # optional skill-formula expression, default 0
```

The skill must still name block slot 400 in its table row. Calls run in list
order, with the same live capabilities as event callbacks. The function gets
`e.event = "call"`, `version`, `frame`, `unit`, optional `other`, `id` and
`argument` (both the evaluated argument), `skill` and `level`. A skill call uses
the caster and target; a trigger call uses holder and event counterpart, with
the trigger's existing skill/level semantics. Return an i32: zero means no
block action, nonzero means acted. A trigger ignores that return value. Errors
are logged and count as no action for a skill part; later parts still run and
earlier writes remain applied. Budget, scope lifetime and recursion limits
are the same as events. A block called synchronously from inside a script's
API operation is refused instead of reentering the VM.

Module paths are relative, case-sensitive, at most 128 ASCII bytes, with
letters/digits/underscore/dot/hyphen in each path segment and no `.`/`..`
segments. Function names are ASCII identifiers of at most 64 bytes.
Check-mod, checked reload and the game builder verify all declared targets
exist and are functions before gameplay; they do not execute call bodies.
The empty/default ruleset has no calls and installs no runtime.

Open [Script Reaction](../../templates/script-reaction) for a complete
trigger-to-script-to-timer example that needs no added table rows. Studio's
skill/trigger forms can add and preserve call-script blocks. `call_script` is in
the block vocabulary (see [blocks.md](blocks.md)) as a skill part and as a
trigger action, so `check-mod` and Studio list it.

## Editing in Mod Studio (S4)

Open a project and choose **Scripts**. The file list includes nested `.luau`
modules in that project's `files/scripts` folder. **Add module** accepts a
relative path such as `combat/reaction.luau` and creates an unsaved buffer.
Switching files keeps every buffer; a dot marks unsaved changes. The editor
colors Luau keywords, strings, comments and numbers. It does not execute code.

Choose one of the twelve events and press **Insert event handler**. New modules
use `local M = {}; return M`; the picker also handles a final `return { ... }`
export table, including the Script Reaction template. It refuses duplicate
handlers and unfamiliar export shapes instead of rewriting arbitrary code.
Handlers receive `(e, api)`; the runtime and capability rules above apply.

Set **Tools & install paths** to the game executable, converted native install,
and installed mods folder if needed. **Save** writes the buffers and queues the
existing game **Check** command. The Problems panel reports syntax, sandbox,
module-load and declared-export errors; it does not execute gameplay callbacks.
When a tool is already running, the check waits for it and reads saved files;
newer unsaved edits stay in the editor. Missing tool paths produce a reminder
to configure them and run Check, not a successful validation. Invalid Luau can
be saved for repair. Play, Check and Package also save script/kit edits first.

Studio refuses to overwrite a file changed by an external editor. Copy your
unsaved edits before using **Reload selected from disk (discard its edits)**,
then reconcile them with the reloaded text. Reopening a saved project refreshes
the file list. Module names use the portable path
rules above. Symbolic links/reparse points are refused. The project limits are
64 modules and 1 MiB total source, matching the host. No base-game assets are
copied into scripts.

## Visual graph sources

Versioned `files/scripts/*.graph.toml` sources now compile deterministically to
Luau and use this same host, events, APIs and limits. Check-mod reports graph
errors by source path and node ID. See [visual graphs](visual-graphs.md) for the
format and control/data semantics. No DF2 dependency is needed.

UI publication (M3): `api.ui_variable(player, name, integer)` sends a transient
integer to that player's data-panel bindings. The module qualifies `name`:
`scripts/combat.luau` + `rage` becomes `var.combat/rage`. Republish initial state
and changes, including player re-entry. Names and API calls are bounded; only
player targets and i32 values are accepted. See `docs/modkit/ui.md`.
