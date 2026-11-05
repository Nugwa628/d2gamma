# Visual effects and imports

Mods define recipes in `files/effects/*.toml`. Base-game sources contribute no recipes. Folder and pack layering follow normal later-path wins rules. Each document is at most 256 KiB, at most 64 documents, 128 recipes per document and 512 unique recipe names in total. Unknown keys, duplicate names, unsupported versions and unknown missile/overlay table names fail Check and startup.

```toml
version = 1
[[effects]]
name = "ember/windup"
duration = 25
# Optional names from the effective patched tables:
# missile = "my_missile"
# overlay = "my_overlay"
[effects.light]
radius = 8
red = 255
green = 180
blue = 80
[effects.particles]
count = 16
speed = 4
spread = 32
gravity = 0
color = 255
```

Names use 1–128 ASCII letters, digits, `_./-`. A recipe needs at least one component. Duration is 1–3600 ticks, default 25, for the recipe preview. The runtime trigger supplies the authoritative lifetime (for example, the AI wind-up duration). Lights use the existing engine light list: radius 1–18, RGB 0–255. Particles use the existing indexed frame and palette-map path: count 1–128, speed 0–64, spread 0–512 pixels, gravity -64–64 and nontransparent palette index 1–255. Defaults are shown above. Particle offsets use a private deterministic seed and the shared 25 Hz preview function, consuming no simulation RNG. Particles use the weather pixel pass with world clipping; missile/overlay sprites retain the existing art loader and draw ordering.

`world_view::mod_effects` resolves table names with the engine compiler's name linker. It owns at most 128 live instances and 4096 visible particle draws per frame. Its internal named trigger takes an authoritative source and duration; duration zero cancels the same source/name instance. Replacement, expiration, missing sources, clock rewind and act changes remove owned lights and sprite handles. It reads the client model and cannot change game outcomes. U6 supplies the named visual-event adapter. No recipe plays merely because it is present.

## Studio

The Visual effects pane edits the same validated files. Add/delete recipes, pick effective missile/overlay names, and edit light/particle properties. Advanced TOML preserves the complete schema. Save validates all files and unique names before writing. The preview shares particle math, shows light radius/color, and can load native missile and overlay frames from the project and configured native base. Sprite animation previews one frame per tick in the first direction; actual blend, world lighting, perspective and occlusion are confirmed through Play. Missing palettes preview in gray.

## Offline art and sound import

Import art & sound accepts paths to artwork or audio you own. Conversion runs in an authoring worker and writes only after conversion and native read-back succeed. Existing files require the explicit replacement checkbox. Save and tool launches wait for conversion. Imports are project files and are included in normal packaging; no runtime converter is installed.

PNG sprite grids have frames across and directions down. Choose frame width/height, offsets, an engine destination such as `data/global/missiles/ember.dc6`, and a native JASC palette `.pal` (normally `base/data/global/palette/act1/pal.dat.pal`). Alpha below 128 becomes index zero; other colors quantize to the nearest opaque palette entry. The importer writes the engine's native `.dc6.toml` and indexed `.dc6.png` pair. Refer to its basename in a patched missile `CelFile` or overlay `Filename`; the existing loader falls back from DCC to DC6. Limits: 64 MiB input, 4096×4096, at most 256 frames across and 32 directions down.

WAV import accepts mono/stereo RIFF PCM 8/16/24/32 or finite float32 at 8–192 kHz, including padded ancillary chunks. It bakes linear resampling and sample conversion into canonical 22,050 Hz PCM16 at a `data/...wav` path, with a waveform preview. Limits: 64 MiB input and ten minutes output. For a normal `sounds` row, import to `data/global/sfx/my-sound.wav` and set its `FileName` to `my-sound.wav` (relative to SFX, not the full engine path). The engine uses `data/local/sfx/` for speech IDs 2934–4656 and `data/global/music/` for music IDs 4657–4698. Runtime audio loads the existing native WAV format.

All destinations are canonical relative lowercase `data/` paths; traversal, duplicate output names and symlink destinations are refused before any output is written. Synthetic tests and the capture example generate their own pixels/audio; no game art or sounds are shipped.

## Named telegraph events

U6's visual adapter is keyed directly by recipe name. No AI-side telegraph producer exists yet; this is the visual-side fallback. To connect a producer, call `d2_server::visual_events::emit(host, viewer, monster, Cue { name: "ember/windup", point: (x, y), duration: windup_ticks, follow: false })` once for each viewer eligible under the AI host's normal visibility rules. Call only for mod games. The hook derives the monster's wire GUID/type from the live server list, validates the addressed player, and sends the existing mod envelope. A duration of zero cancels the same monster/name. It performs no stat writes or RNG draws.

`d2-proto::mod_ui` owns the versioned LCUI opcode 3 codec (18-byte header and a 1–128 byte name). It carries source identity, subtile coordinates, a 0–3600 server-tick duration and a follow flag. It has no gameplay effect and cannot act as a button action. The client accepts it only on the existing mod wire. The bridge keeps at most 256 pending events, coalesces repeated source/name events, gives cancellation priority at capacity, and clears the queue with an epoch change on reconnect.

`world_view::telegraphs` routes valid live-source events into U5 recipes. Instances replace by source/name and stop on explicit cancellation, death, removal, expiry, clock rewind, act change or reconnect. Reconnect forgets old light handles because the replacement world's light IDs can be reused. Unknown recipes/sources draw nothing. It consumes no simulation seed and cannot select targets, deal damage, change stats or alter the AI's timing. The authoritative producer owns wind-up start/cancel and viewer selection.
