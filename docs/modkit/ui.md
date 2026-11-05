# Mod panel files

A mod may add `files/ui/*.toml`. Only enabled mod layers supply panels; the
base game supplies none. Later layers replace a document at the same path.
Documents are sorted by path. Duplicate panel names or replacement targets
are rejected rather than silently hiding another document.

```toml
version = 1
name = "resource_counter"
# replace = "hud"

[[elements]]
id = "background"
kind = "frame"
x = 480
y = 20
width = 320
height = 60
color = 255

[[elements]]
id = "label"
kind = "text"
x = 490
y = 30
width = 300
height = 20
text = "Custom resource"
```

Coordinates describe the 1280 by 720 standard view. The UI pass scales them
with the current view and uses the game's existing window/4K presentation.
Rectangles must fit in that view and have positive dimensions. Each panel
has at most 256 elements; at most 64 documents of 256 KiB each are accepted.
Unknown keys, duplicate element ids and unsupported versions are errors.

Kinds: `frame` draws an outline, `text` draws a label, `bar` fills its rectangle
by `value` (0–1000, default 1000), `button` draws a rectangle and label, and
`image` draws an existing panel-art frame. Images use `image` (relative to
`data/global/ui`, without `.dc6`) and `frame` (default 0). Asset conversion
remains an import-time operation. Text uses the engine's font 0. Colors are
palette indices (0–255). Buttons with an `action` send a declared server request.

`replace` accepts `hud`, `border`, `inventory`, `character`, `skilltree`,
`waypoint`, `stash`, `cube`, `quest_log`, `shop`, `message_log`, `escape_menu`,
`npc_menu`, `socket`, `dialog`, `gold_dialog`, `market`, `messages`, `overhead`,
`warnings`, `top`, or `hire`. A replacement keeps the built-in panel's open
state and stacking slot. Added panels stay visible. Existing panels and draw
requests remain untouched when the effective document set is empty.

Run `check-mod` to validate the effective documents. For a visual check, start
an enabled mod with the example above, inspect the frame and label at 720p
and 4K, then open a replaced inventory and verify its normal toggle controls
its visibility. Baseline frame-hash comparison must use the no-mod game.

## Live bindings

An element may have a `bind` table. Names in `stat.*`, `state.*`, and
`stacks.*` resolve against the effective `itemstatcost.txt` and `states.txt`
(including mod patches); typos fail `check-mod`. Values come from the local
player's server-synchronized model. A state is 0/1, stacks are the count.
Unreceived player, quest or script values show an em dash and an empty bar.

```toml
[[elements]]
id = "life"
kind = "bar"
x = 20
y = 650
width = 250
height = 20
color = 255
[elements.bind]
source = "stat.hitpoints"
maximum_source = "stat.maxhp"
divisor = 256
format = "percent"
[[elements.bind.colors]]
minimum = 0
maximum = 25
color = 3
```

`divisor` converts fixed-point values before display (positive, default 1).
`minimum` and `maximum` set the range (default 0..1000); `maximum_source`
overrides the upper bound. Bars clamp to that range. `format` is `integer`,
`percent` (position within the range), or `range` (`value/maximum`). Up to
16 inclusive color ranges select the first matching palette/text-color index;
otherwise the element color applies. Bound text uses text-color maps 0..12; 255 means plain text. Arithmetic uses wide integers, including
negative values and empty ranges. Text/button elements display the bound value.
Frames may use a binding for their color; images retain their authored art.

Quest records have numeric slots/bits, so give each referenced flag an explicit
name in its panel. This supports custom quest layouts without assuming base IDs:

```toml
[[quests]]
name = "den.complete"
slot = 1
bit = 0
# An element's bind.source can now be "quest.den.complete".
```

Scripts publish transient player-local integers with
`api.ui_variable(e.unit, "rage", value)`. From `scripts/combat.luau`, this
is `var.combat/rage`; subdirectories remain in the qualified name. Publish the
initial value and publish whenever it changes; republish when a player joins or
when rebuilding script state. These values are presentation state, not saved
variables or authority. Names use 1..128 ASCII letters/digits, `_`, `-`, `/`, `.`
including the module prefix. The client retains at most 256 names per session.
The versioned LCUI service uses the existing mod envelope and does not change
base-game messages. Scripts cannot read client input through this publication API.

## Server actions

A button references an action in the same panel. The client sends only
`panel/action`; the server obtains the player from the joined connection and
reads authoritative base stats. Names resolve against effective patched tables.

```toml
[[actions]]
name = "allocate"
stat = "strength"
delta = 1
minimum = 0
maximum = 500
cost_stat = "statpts"
cost = 1
cooldown = 25
# town = true
# [actions.validator]
# module = "allocation.luau"
# function = "allow"

[[elements]]
id = "allocate_button"
kind = "button"
x = 20
y = 80
width = 160
height = 30
text = "Allocate strength"
action = "allocate"
```

`delta` is a nonzero signed integer; current and resulting values must be
within `minimum..maximum` (default 0..2147483647). `cost_stat` requires a
positive `cost`, a different stat, and sufficient points. `town` defaults false.
`cooldown` is 1..3600 simulation ticks (default 25); eligible attempts consume
it even if a later bound or script check refuses them. Disconnect clears it.
There are at most 32 actions per panel and 256 per server. Unknown declarations,
malformed requests, missing world capabilities, overflow and script errors fail
closed. All checks precede either stat write. The server's usual stat updates
provide the result; the button never changes stats optimistically.

An optional M2 validator exports a bounded integer predicate, for example:
`return { allow = function(current) return current < 100 and 1 or 0 end }`.
It receives the authoritative current value and permits only a positive integer
result. This predicate has no world mutation capabilities; it uses the script
host's instruction/memory limits. `check-mod` and server construction reject
missing modules/exports. Action bounds and costs apply with or without a script.

The shared `d2-proto::mod_ui` LCUI v1 codec adds operation 2 inside the existing
mod envelope (6-byte header plus a 1..128-byte ASCII name). No player ID or
client-supplied stat value is accepted. All LCUI versions are reserved by this
service, including malformed/unknown versions, so they cannot fall through to
another mod handler. No-mod rules neither read declarations nor accept requests.

## Studio layout editor

Open **Panel layout** in Mod Studio. **New panel** creates an unsaved document
under `files/ui`. Click a palette item to add it at the center, or drag it onto
the canvas. Select and drag existing elements to move them; the editor clamps
their rectangles to the standard view. Use the property sheet for IDs,
coordinates, text, image/frame, fill/color, declared button actions and bindings.
The replacement picker contains the engine's built-in panel names.

The 1280×720 and 3840×2160 previews share the same document coordinates. Fit,
25%, 50% and 100% zoom control inspection; dragging at any zoom writes standard
coordinates. Image elements show labeled bounds and text uses the Studio font.
Use Play to confirm actual game art, font metrics and palette colors.

Load tables to populate the searchable stat/state/stack binding picker from the
effective edited names. Declared quest names also appear. Enter dynamic script
names directly as `var.module/name`. A sample value previews binding formatting.
The advanced TOML editor exposes actions, quests and color ranges, with strict
validation before Apply. Form edits preserve all of those fields.

Save panels, Ctrl+S, Play, Check and Package validate and save layouts through
the same schema. Invalid documents and duplicate panel/replacement names block
saving. Unmodified projects create no UI files. Check validates named bindings
and script links against the effective mod stack before shipping.

For shared `sync.<name>` bindings and `net.<action>` buttons, see
[network values and actions](network.md). Studio offers both in the layout
property pickers.
