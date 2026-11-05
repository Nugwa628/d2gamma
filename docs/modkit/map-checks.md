# Map connectivity checks

`check-mod` adds map checks when `files/levels.toml` is present. It rejects a level outside configured acts, cross-act warp destinations, missing return links, overlapping authored rectangles in the same act, and levels unreachable from their act town, waypoints or declared travel arrivals. Use `acts.toml` travel entries for movement between acts.

For each authored level it generates and streams the real rooms and DT1 tiles, reads their subtile movement collision, and searches from the declared entry. A blocked entry, unreachable warp centre, missing runtime warp unit/link, or unreachable typed monster spawn is an error. Findings identify the level, seed and warp slot where relevant. Drawable art alone does not establish walkability.

Preset-only registries with a single source variant check seed 0. If any level uses maze/outdoor rules or multiple source variants, the registry checks 32 seeds (0–31). The report states this sampling explicitly. Run additional seed previews and playtests for broader coverage. Static tile collision is checked; doors and other objects that change collision during play need human testing. D2Gamma reports and loading stay unchanged when no map definitions are present.
