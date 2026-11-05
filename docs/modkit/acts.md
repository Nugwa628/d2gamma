# Acts as data

A data mod can replace the act registry with `files/acts.toml`. The last layer containing the file supplies the complete registry. Without it D2Gamma retains its five acts, town IDs, boundary rules, generation order, quest gates and bytes.

```toml
version = 1
[[acts]]
name = "Home"
start = 1
end = 40
town = 1
waypoints = [{ id = 0, level = 1 }, { id = 1, level = 3 }]
travel = [{ npc = 155, action = 0, to = 40, tile = 0, text = 5000 }]

[[acts]]
name = "Beyond"
start = 40
end = 75
town = 40
waypoints = [{ id = 2, level = 40 }]
travel = [{ npc = 175, action = 0, to = 1, tile = 5, text = 5001 }]
```

Act IDs are zero-based row indexes. There are 1..128 acts, matching the mod save's seven-bit act field. Names are unique, trimmed, printable text of at most 63 characters. Level ranges are ordered, disjoint, half-open `start..end`, with nonzero IDs through 255 (the room-reveal wire field is a byte). Every town belongs to its act. Level/map table rows and map files must exist; the registry does not manufacture maps.

Waypoint IDs are explicit, stable bits 0..111 in each difficulty's independent record. Every waypoint level and bit is unique; each act has at most nine rows. New characters and added difficulties know the first act's town waypoint, if one is declared; loading preserves saved bits without granting bit 0. The menu displays the configured rows and cycles acts using the left/right halves of its title or the mouse wheel. Town flags and arrival tiles follow the registry. Keep IDs stable across releases of a mod to preserve saves.

Travel matches the source act, NPC class and action. `to` names a level in the registry; `tile` defaults to 0. `text`, when nonzero, adds a selectable travel row to that NPC's menu using the mod's string-table ID (at most four visible routes per NPC per act, 256 total routes per act). NPCs still need an interact-enabled monster row. Routes use the authoritative act-change queue and the normal destination placement. With an act file, built-in NPC act-travel destinations are disabled. Data routes are available without built-in quest gates; the existing NPC/quest providers are the seam for later quest blocks. Other NPC services remain available.

The server's dungeon, unit room lists, tick iteration and inactive records grow with the registry. Portals, level ownership, resurrection, LoadAct messages and saved towns use it. Additional act IDs persist in the versioned mod progress section, even when the mod keeps three difficulties. Base character fields stay byte-compatible.

Native outdoor placement remains available for an act whose boundaries and town match its built-in row. Remapped and additional acts use the existing table-driven preset and maze generators; provide those map definitions and files. New outdoor layout algorithms and quest blocks are separate work. Client art uses the existing native palette fallback for extra acts.
