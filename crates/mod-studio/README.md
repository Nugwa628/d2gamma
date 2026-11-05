# Mod Studio

Separate authoring executable; players only need D2Gamma-Game. Build with
`cargo build -p mod-studio --profile dist`. The binary is `Mod-Studio.exe` on
Windows. eframe uses its small OpenGL backend; no Bevy, client, server or sim
crate is linked. `depcheck` enforces these boundaries. Studio release/signing
is separate from the game release.

On the start page, enter a folder path to open it, or choose New mod. Templates
are maintained under the repository's `templates/` and embedded at build time,
so the executable works outside the source checkout. Configure the installed
mods folder to offer installed folder-based mods that permit derived works.
Add-ons targeting D2Gamma are screen-only; add-ons targeting a mod require
its maker's approval before they can run. Installed pack browsing is future work.

Manifest controls use the modkit's strict validator. Rules controls come from
`docs/modkit/rules.md`; unchecked keys are omitted. The text view also permits
settings beyond that documentation table. Saving checks TOML syntax; gameplay
range validation happens in the game. Files are readable TOML. Save before
returning to the start page; unsaved projects are kept open. External file edits
should be made with the project closed, then reopened.

Configure the game executable, converted native install and installed mods folder
in Tools & install paths. Play saves and launches `D2Gamma-Game --mod-dir <project>`
with the configured native directory, then returns to editing. The game's dev
reload owns watching and restart behavior.

Check saves valid changes and runs the game's headless `check-mod` command.
Package runs `D2Gamma-Game package-mod` and shows the signed pack's sibling path
`<id>-<version>.d2pack`. The staged snapshot passes the complete checker before
signing; errors refuse publication and warnings remain visible. See
[checked publication](../../docs/modkit/package-checks.md).

Check/Package run off the UI thread with elapsed time, output and a Cancel button.
Cancel terminates the tool; cancelled jobs cannot claim success. Problems groups
the versioned JSON report by severity. Click a table/row/cell location or project
file to navigate; tables load in the background if needed. Scenario summaries
without a concrete editing location copy their details. Copy JSON preserves the
full report. File previews display the first 256 KiB. Unverified findings remain
warnings; they are candidates to reproduce, not proof of a bug.

Import art & sound offers Native asset library for an artist's `data` folder.
It validates and preserves native DC6/DCC/DT1 sidecars and their PNG frames,
COFs, DS1 maps and playable WAV files. Definitions such as scripts, rules and
tables belong in their editors and are refused by this importer. PNG icon / mod
picture preserves a PNG at `icons/name.png` or `mod/picture.png`. Existing files
require the explicit replacement checkbox. Screen-only add-ons retain their
file restrictions.

Configure the converted native install and installed mods folder, open a project,
then choose Table editor → Load tables. Studio resolves the same declared mod
stack as the game and reads `base/data/global/excel/*.txt` through d2-native.
It offers all 86 generated table schemas. Choose a table, select a row by its
number, and edit cells. Gold cells differ from the install. Reference selectors
show readable row names and filter by text; compound recipe references also
keep an expression field for modifiers. Flags use checkboxes; integers are
checked against their storage range (including signed values). Existing unusual
base cells remain accepted until edited. Search matches cells across all tables;
click a result to select its table and row. Add and duplicate need a new row key.
Disable retains the row id, strikes the row through and lists any incoming broken
references in Problems. Restore row undoes Studio edits to an existing row.

Save, Play, Check and Package save the canonical diff to
`files/patches/~studio.d2patch`. Existing patches and string edits are preserved.
Studio never writes a full game table. The overlay sorts after other patch files;
a later-sorting hand-authored file must be renamed first. Reopen a project after
external edits; saving refuses if patch files have changed on disk. Fix Problems
before saving. Screen-only add-ons cannot edit game tables. The Problems panel
uses K4a's library reference checks, including numeric and callback references;
the toolbar Check action also runs the K9 gameplay checks.

Settings and up to twelve recent projects are stored in versioned TOML at
`%APPDATA%/D2Gamma-Mod-Studio/settings.toml` (HOME on Linux).

For repeatable UI captures: `Mod-Studio --render start.png`, or
`Mod-Studio <synthetic-project-folder> --render project.png`. This renders the
actual egui window and saves the framebuffer, then exits. Linux CI uses Xvfb
and software OpenGL; no game files are needed.

For input-driven checks, run `Mod-Studio --input-script tour.json --captures out`.
The JSON contains a `steps` array. `click` targets exact painted `text`;
`fill` clicks beside a label and replaces the focused field using ordinary
keyboard input. `nth` selects repeated labels and `offset: [x, y]` adjusts
the target in logical points. `click_at`, `type`, `key`, `scroll`, `wait`,
`wait_text` and `capture` cover canvases, menus, asynchronous tools and captures.
For example:

```json
{"steps":[
  {"do":"click","text":"New mod"},
  {"do":"fill","text":"Name","value":"Lantern Reach"},
  {"do":"capture","name":"wizard"}
]}
```

The harness has no editor-model or project-write operations. A missing target
or timed-out wait exits with code 1. `result.json` records completed steps and
visible text; each capture writes a framebuffer PNG and text/bounds JSON.
Use a fresh settings HOME on CI to keep recent projects reproducible. Captures
of an empty project need no game assets; keep game-art previews out of docs.

Synthetic spreadsheet framebuffer captures (no game install needed):
`cargo run -p mod-studio --example render_tables -- tables.png`
and `cargo run -p mod-studio --example render_tables -- references.png dropdown`.
On CI, prefix each command with `xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1`.

## Monster AI

The Monster AI tab edits `files/ai.toml` (see [monster AI](../../docs/modkit/ai.md)). A behaviour graph is drawn as a
node graph: one box per state, one arrow per transition labelled with its conditions, and one orange box per boss
phase. Click a box to edit that state; drag a box to tidy the view (the arrangement is not saved). States, transitions,
conditions and actions are chosen from the engine's own lists, so the choices are what the game reads. The problems
list is the game's own reader (with the project's other rule files, so the immunity-state check runs too); *Show* jumps
to the state or phase. Save is refused while the engine objects. A file the editor cannot represent is shown as an
error and never overwritten. `cargo run -p mod-studio --example render_ai -- out.png [preset|phase]` captures the tab.

Under a graph, *Test arena* runs it: pick the monster, ticks, seed, distance and level, and *Run in arena* saves the
file and runs the game's `ai-arena` command (so the game executable and the converted native install must be set in the
settings; a run can be cancelled). The result is the arena's summary, a strip of the states over the run with the
wind-ups and phases marked, and the list of events. The boxes of the node graph show how many ticks the run spent in
each state. A trace of another graph, or of this one before you edited it, is flagged and its numbers are not drawn.

## Named stats

Choose Named stats, Add named stat, then enter its name and stable save ID.
The same pane adds add/convert/percent-of transformations using named source
and target stats. Engine validation runs before Save; malformed files and
external changes are preserved. Opening an empty pane creates no file.
Screen-only add-ons cannot author stats.

## Difficulties

Choose Difficulties to name the ordered difficulty IDs and append a new one.
The pane writes files/difficulties.toml through the engine validator. Opening
it without editing creates no file; screen-only add-ons cannot edit it.

## Acts and waypoints

Choose Acts & waypoints to define act ranges, town levels, permanent waypoint
IDs and NPC travel routes. Add matching level rows and geometry in the Table
and Map editors. New waypoints choose the first unused ID across all acts.
Save uses the game validator and preserves files changed outside Studio.

Visual graph properties include a searchable node selector, portable Node IDs,
precise X/Y positions and typed input/control pickers. Renaming updates all
wires and control edges. Canvas pin wiring and dragging remain available.
