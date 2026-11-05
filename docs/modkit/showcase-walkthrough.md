# Lantern Reach showcase walkthrough

Lantern Reach is original template content by Nugwa. Its gameplay is entirely in mod files; the Rust engine supplies the shared modkit. The geometric tiles, Glass Warden sprites and reward icons are original procedural artwork. An installed, converted game supplies the engine's ordinary player and interface resources.

Copy `templates/showcase` to your mods folder as `showcase`, or open the folder in Mod Studio. Check it against your converted native folder, then Play. Create a fresh character. Character copying is deliberately disabled: the new world and item progression are independent.

1. Start in Lantern Harbor. The lantern HUD shows server-synced resonance. Click **Charge beacon** to add one, up to 20. The server checks both the declared bounds and the Luau validator; clicking a panel never writes a stat locally.
2. Activate the harbor waypoint, then take the exit into the Shifting Garden. Entering recovers two glimmers from its beacon. Open the quest log to see the collection count. The garden is generated from maze rules; its beacon clearing is an authored set piece.
3. Cross the garden to Lens Court. Select **Lantern Pulse** on a skill button. Each cast runs the visual graph to build resonance, calls Luau to apply Lantern Ward, then damages nearby enemies through skill blocks. The Ward indicator returns to zero through a state-end trigger calling Luau when the buff expires.
4. Fight the Glass Warden. Its graph winds up a visible lens pulse before attacking. Below half life it changes to its fractured phase and gains a two-second Prism shell that blocks damage and crowd control. Wait out the shell and finish the fight.
5. Carry the two glimmers when the Warden dies. **Quiet the lens** completes and grants a Beacon Seal. Check the quest-log panel and inventory, then save and rejoin to check progress.

The fourth difficulty is **Eclipse**; Dawn, Gale and Dusk precede it. Each difficulty keeps its own progress. The template is intentionally short and uses modest damage so its systems can be inspected.

To check and package from the repository root (replace `<native>` with your converted install):

```text
D2Gamma-Game check-mod templates/showcase --native <native> --out showcase-check
D2Gamma-Game package-mod templates/showcase --base <native> --out showcase.d2pack
D2Gamma-Game package-mod templates/showcase-notes --base <native> --out showcase-notes.d2pack
```

Install the full mod in the mods folder before checking its add-on, so the checker can find the maker's approval. Keep your private signing key out of the mod and package.

For the optional route-note overlay, install `templates/showcase-notes` beside the full mod and enable it in `mods/addons.toml`:

```toml
[enabled]
showcase = ["showcase-notes"]
```

The full mod explicitly approves that add-on. It is screen-only and adds no rules, scripts or actions.

The files to explore are `stats.toml`, `difficulties.toml`, `acts.toml`, `levels.toml`, `ai.toml`, `states.toml`, `crowd_control.toml`, `skills.toml`, `triggers.toml`, `quests.toml`, `net.toml`, `effects/lantern.toml`, `ui/`, `scripts/lantern.graph.toml`, `scripts/ward.luau`, and the guarded table patch. The generated Luau can be inspected in Studio's graph editor; only the graph source ships in the mod.

## Human review in the game window

The CI headless check measures all 49 showcase animation-rate scenarios: player town/field modes and the new boss. All pass the drawn-frame/table-rate comparison; none remain Unverified. Judge animation smoothness and visual transitions in the game window.

Judge movement and exit discoverability, the maze and set-piece appearance, tile seams and sprite alignment, wind-up readability, the immune window's visual clarity, combat timing and balance, HUD and quest-log legibility at 720p and 4K, and add-on placement. Headless checks establish state changes and server validation; they cannot establish that these look or feel good.
