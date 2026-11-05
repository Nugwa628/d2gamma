# Visual graph format (v1)

Save graphs under `files/scripts/*.graph.toml`; subfolders work. The effective
mod graph compiles to Luau in memory and runs on the existing M2 server host.
Ship the graph, without a generated `.luau` companion. Later layers replace the
same path; folders and signed packs behave alike. Base games discover no graphs
and allocate no VM. This format does not depend on DF2.

```toml
version = 1
[[nodes]]
id = "kill"
kind = "event"
event = "kill"
next = "greet"
x = 20
y = 50

[[nodes]]
id = "greet"
kind = "api"
name = "message"
x = 320
y = 50
[nodes.inputs]
unit = { node = "kill", pin = "unit" }
text = "A friendly echo"
```

IDs are unique ASCII identifiers, at most 64 bytes. Canvas coordinates are
integers and affect presentation only. Node array order has no execution effect.
`check-mod` reports the graph path and node ID when compilation fails.

| Kind | Fields and typed pins |
|---|---|
| `event` | `event`: one of the twelve [M2 events](scripts.md#events); `next` control output |
| `constant` | `inputs.value`: integer, Boolean, or text; `value` output |
| `math` | `op`: add, subtract, multiply, divide, modulo, min, max; integer inputs a/b, integer output value |
| `condition` | Integer comparisons equal/less/less_equal/greater/greater_equal; Boolean and/or/not; unit exists; Boolean output value |
| `branch` | Boolean input condition; yes/no control outputs |
| `loop` | Literal count 1..64; body and next control outputs |
| `variable` | name + op get/set; get yields integer value, set consumes integer value and has next |
| `api` | name from the M2 capability list; named, typed inputs; next; value output for reads/random/spawns |
| `return` | Integer input value; ends this event/call, including enclosing loops |
| `call` | Named exported entry for explicit script callers; next |

Unconnected control outputs end the current path. A loop runs its body to the
end of the path, repeats it count times, then follows next. Nested loops work.
Control cycles and data-expression cycles are errors; use a loop to repeat.
Variables are module-local, per-game transient integers, initially zero. They
are shared by that module's events and reset when the VM restarts. Use a stat
for per-unit storage, or publish a value through `ui_variable` for UI.

Input literals retain their types. A linked input is `{node="id",pin="value"}`;
it cannot contain source code. Event/call nodes expose unit, other (nullable unit),
id, frame, unit_type, argument, skill and level (unavailable integer fields are
zero). Test optional handles with exists before calling an API with them. API
result links must follow their producer on every incoming control path. Pure
math/condition/get nodes evaluate where consumed, so a loop sees current values.

API input names and types mirror [M2 capabilities](scripts.md#capabilities):
unit/target are unit handles; text/name are strings; remaining parameters are
integers. The compiler accepts only the explicit M2 capability whitelist.
Runtime checks still validate live handles, table IDs, costs and capabilities.
Division truncates toward zero, modulo uses Luau remainder, and arithmetic
rejects values outside signed 32-bit range. Division by zero is an error.

Limits: 256 KiB source, 256 nodes, expression depth 32/work 1024 per input,
256 KiB generated source, 4096 executed control nodes per invocation, plus the
M2 host's API-call, VM memory, callback and execution budgets. Nested loops
cannot evade the invocation budget. Errors are not transactions: earlier
engine writes remain applied, as for hand-written M2 scripts.

## Editing in Mod Studio

Open a folder project and choose **Visual scripts**. Name a module ending in
`.graph.toml`, then choose **New graph**. The searchable palette contains every
M2 event, the typed operators and the API capabilities listed above.

Click a node title to inspect it; drag the title to move it. Click a coloured
output circle, then a matching input circle to connect data. Integer pins are
orange, Boolean purple, text green and unit handles blue. Click a white control
output, then the destination node title to connect execution. Branches have
`yes`/`no`; loops have `body`/`next`. Disconnect controls and edit literal inputs
in the inspector. Deleting a referenced node leaves a visible compiler error
until you repair its wires. Positions affect only the canvas.

Compiler errors appear above the canvas and mark the offending node red.
**View generated Luau** shows the exact deterministic compiler output, read only.
**Test-fire fake unit** executes that output in a fresh sandbox with a seeded
RNG, player handle 1 and monster handle 2. Choose the event, ID and seed; select a
trace row or **Next trace step** to highlight an executed node. The trace records
control nodes and API calls, and retains the partial trace when execution fails.
Data expressions are evaluated inside their consuming control step.

The fake world starts with zero stats and no states. Stat and stack operations
use in-memory values; spawned handles are synthetic. It checks primitive API
ranges and player-only capabilities, but does not load game tables, simulate AI
or combat, advance timers or send HUD messages. Check and Play against a native
install to validate table IDs and actual game behavior.

Save writes graph sources and queues the same game Check used by the Luau pane
when the game executable and native directory are configured. Check problems
with node locations open the graph inspector. Invalid drafts can be saved and
repaired with **Advanced graph TOML**; packaging still requires a successful
Check. Studio refuses a graph save if an external editor changed that file since
it was opened. Copy your edits before **Reload graph (discard edits)**.


## Complete example

[Visual Echo](visual-echo.md) walks through a graph-only mod: a seeded 10% kill
reaction publishes a HUD pulse and schedules its clearing timer; a separate
buff-ending entry announces expiration. Copy `templates/visual-echo` to try it.
No handwritten Luau or additional table rows are needed.

## Calling a graph from blocks

The DF2 `call_script` action and skill part accept both `.luau` modules and
`.graph.toml` graphs. In Studio choose **Call script / graph**, enter the path
under `files/scripts`, and enter the **name** of a `call` node as the function.
The registry describes both forms; existing Luau calls keep their spelling.

```toml
[[skill]]
id = 1
[[skill.part]]
kind = "call_script"
module = "combat.graph.toml"
function = "strike"
argument = "lvl * 2"
```

Create a `call` node named `strike` in the graph. Its outputs include `unit`,
`other`, `skill`, `level` and `argument`. End the control path with a `return`
node supplying an integer (nonzero means the skill part succeeded). Trigger
actions ignore the result, just as Luau calls do. Graphs use the existing M2
capabilities, budgets and error reporting. Missing modules or call names fail
check-mod before gameplay; event nodes alone do not export arbitrary call names.
