# Synced values and server actions

Open **Network values & actions** in Studio to edit `files/net.toml`.
Add a synced value or a server action, give it a stable ID and a name, then
choose its source or validation rules. **Load tables** supplies effective
stat/state names. Script validators use modules from the Scripts pane and an
exported function name. Save, then run **Check** before packaging.

A minimal contract:

```toml
version = 1

[[values]]
id = 1
name = "strength"
source = "stat.strength"

[[values]]
id = 2
name = "rage"
source = "var.combat/rage"

[[actions]]
id = 7
name = "allocate"
stat = "strength"
delta = 1
minimum = 0
maximum = 200
cooldown = 25
cost_stat = "statpts"
cost = 1
# Optional additional predicate:
# [actions.validator]
# module = "combat.luau"
# function = "can_allocate"
```

Value sources are `stat.<name>`, `state.<name>` (0 or 1), or
`var.<module/name>`. Stat/state names include table patches. An M2 publication
such as `api.ui_variable(e.unit, "rage", 42)` from `combat.luau` supplies
`var.combat/rage` for that player. Unpublished variables begin at zero.
See [scripts](scripts.md) for event capabilities.

Actions carry a declared ID; the server supplies the player's identity and
current values. The shared [UI action rules](ui.md#server-actions)
check bounds, costs, town restrictions and cooldown before mutation. An
optional Luau predicate receives the current stat and must return a positive
integer, for example:

```lua
return {
    can_allocate = function(current)
        return current < 200 and 1 or 0
    end,
}
```

## Use the contract in a panel

The layout editor's binding/action pickers include declared network names.
This `files/ui/resource.toml` shows a counter and requests an allocation:

```toml
version = 1
name = "resource"

[[elements]]
id = "counter"
kind = "text"
x = 50
y = 50
[elements.bind]
source = "sync.strength"

[[elements]]
id = "allocate"
kind = "button"
x = 50
y = 85
width = 180
height = 30
text = "Allocate strength"
action = "net.allocate"
```

`sync.<name>` also works as `maximum_source` for a bar/range binding.
The client displays negotiated server values; a click never changes a stat
locally. Existing panel-local actions and `var.<module/name>` bindings remain
supported. Check rejects unknown network bindings/actions, unresolved stat/state
sources and missing predicate exports for both folders and signed packs.

## Versions and limits

Values/actions each have at most 256 declarations, separate IDs 1..65534 and
unique ASCII names up to 64 bytes. The file is capped at 256 KiB; unknown fields
are errors. A later mod layer replaces the whole contract. Keep existing IDs
stable and bump the mod version when shipping a change.

A nonempty contract negotiates its parsed schema and ordered mod versions.
A mismatch disconnects with **This mod needs updating** and the required server
version where available. Reconnecting negotiates again and resends values.
Empty contracts keep legacy behavior. D2Gamma without mods sends no extra bytes.

The existing mod envelope caps payloads at 505 bytes. Incoming traffic is capped
per player at 64 messages and 4096 wire bytes per second, with one-second bursts;
each action/hello is additionally limited to 8 messages per second. Declared
values/ACKs share 128 messages and 8192 wire bytes per second, with an 8-per-second
limit per value/control ID. Unchanged values send nothing; rotating order avoids
starvation. A full output queue retries the ACK before values and retries values
without advancing their last-sent cache. Limits use simulation frames at the
configured tick rate.
