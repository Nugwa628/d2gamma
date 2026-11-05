# Visual Echo

A graph-only reaction mod. Kill events make one seeded draw: a result below 10
out of 100 publishes a small HUD pulse, sends a message and schedules a timer.
The timer clears the pulse. A separate buff_end event announces buff expiration.
There is no handwritten Luau or custom game asset.

Open this folder in Mod Studio and select Visual scripts, then echo.graph.toml.
Test-fire kill with a chosen seed and inspect the node/API trace. The real game
uses the unit's simulation RNG, not the Studio fake world's seed. Timers are
120 simulation frames (two seconds at the default 60 Hz).

Run Check before Play or Package. A timer only clears this example's cookie 71;
other timer events are ignored. Repeated successes restart the visible pulse,
but each queued timer still fires; an earlier timer can clear a later pulse.
This simple example intentionally demonstrates event wiring rather than a
refreshable buff system. Script state is transient and a player rejoining the
mod does not inherit an old pulse. The game remains server-authoritative.

See the [walkthrough](../../docs/modkit/visual-echo.md) and
[graph guide](../../docs/modkit/visual-graphs.md) for nodes, limits and the M2 API. All table/art/sound data comes from the player's own install.