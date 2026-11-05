# Build a reaction with a visual graph

[Visual Echo](../../templates/visual-echo) is a complete mod with no handwritten
Luau, table copies or custom art. Copy that folder, open it in Mod Studio, and
choose **Visual scripts → echo.graph.toml**. The [format and editor guide](visual-graphs.md)
explains the controls and limits.

## Follow a kill

The `kill` event supplies the unit that made the kill. `is_player` compares its
unit type with zero, and `player_branch` skips monsters. This matters because
HUD messages and UI variables require a player.

`roll` calls the M2 `random` capability with an upper bound of 100. Its result
is an integer from 0 through 99, drawn from that unit's simulation RNG. The
`chance` condition compares the result with 10; `branch` follows its yes wire
for a 10% chance. A failed roll ends the path without publishing anything.

On success, `pulse` publishes `pulse = 1`, `message` announces the lucky kill,
and `schedule` requests a timer after 120 simulation frames with cookie 71.
The separate `timer` entry compares its ID with 71 before clearing the pulse.
At the default 60 Hz simulation rate this is two seconds. A `buff_end` entry
also demonstrates a player-only message when a buff expires.

Pure comparison nodes use coloured data wires. White wires carry execution:
kill → player branch → roll → chance branch → pulse → message → schedule.
The condition nodes evaluate as part of their consuming branch; they do not
need their own white control wires.

## Try it in Studio

1. Choose `kill`, enter a seed, and press **Test-fire fake unit**. Try other
   seeds until the trace includes `pulse`. The same seed repeats the same draw
   in each fresh fake world.
2. Select trace rows or **Next trace step** to see the control path. Open
   **View generated Luau** to inspect the compiler output.
3. Choose `timer` and ID `71`. Its trace reaches `clear`; another ID does not.
   Test-fire starts a fresh fake world and does not advance queued timers.
4. Select `chance` and change its `b` literal from 10 to 100 to exercise the
   success path every time. Restore 10 before publishing.
5. Save and run **Check**, then **Play** against your native install. Studio's
   fake world records message/timer calls; Play exercises the real host, HUD
   and simulation clock. Package only after Check succeeds.

## Show the value

`files/ui/echo.toml` adds a small bar at `(55, 95)` in the standard 1280×720
view. Its binding is `var.echo.graph.toml/pulse`, with range 0..1. M2 namespaces
UI variables by module path; graph modules retain their `.graph.toml` suffix.
You can move or restyle the bar in Studio's panel layout editor. No image asset
is required.

This example keeps the timer simple: each success queues a clearing event, so
an earlier timer may clear a later pulse. It is not a refreshable buff. The
pulse is transient and is not saved across rejoining a game. All gameplay
changes come from the graph and run on the server through the existing M2 host.
